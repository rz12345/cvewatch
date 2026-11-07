//! 對外 HTTP：統一 User-Agent、逾時、429/5xx/連線錯誤的指數退避，並遵守 `Retry-After`。
//!
//! reqwest 以 rustls 建置，預設讀取 `HTTPS_PROXY`。錯誤訊息中的 URL 一律經過遮罩。

use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response, StatusCode};

use crate::config::HttpConfig;
use crate::redact::redact_text;

pub const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);
/// 退避倍數：1s、4s、16s…
pub const DEFAULT_BACKOFF_FACTOR: u32 = 4;
/// `Retry-After` 超過此值時放棄，避免排程工作卡住。
pub const DEFAULT_MAX_RETRY_AFTER: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("無法建立 HTTP client：{0}")]
    Build(String),

    #[error("HTTP 請求失敗（{url}）：{message}")]
    Request { url: String, message: String },

    #[error("HTTP {status}（{url}）")]
    Status { url: String, status: StatusCode },

    #[error("伺服器要求等待 {wait_secs} 秒後重試，超過上限，放棄（{url}）")]
    RetryAfterTooLong { url: String, wait_secs: u64 },
}

impl HttpError {
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            HttpError::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub factor: u32,
    pub max_retry_after: Duration,
}

impl RetryPolicy {
    pub fn from_config(cfg: &HttpConfig) -> Self {
        RetryPolicy {
            max_retries: cfg.max_retries,
            base_delay: DEFAULT_BASE_DELAY,
            factor: DEFAULT_BACKOFF_FACTOR,
            max_retry_after: DEFAULT_MAX_RETRY_AFTER,
        }
    }

    /// 第 `attempt` 次重試（從 0 起算）前的等待時間。
    pub fn backoff(&self, attempt: u32) -> Duration {
        self.base_delay
            .saturating_mul(self.factor.saturating_pow(attempt))
    }
}

pub struct HttpClient {
    client: Client,
    policy: RetryPolicy,
    /// 需要從錯誤訊息中遮罩的已知秘密值（例如 header 內的 API key）。
    known_secrets: Vec<zeroize::Zeroizing<String>>,
}

enum Attempt {
    Done(Response),
    Retry {
        wait: Option<Duration>,
        error: HttpError,
    },
    Fail(HttpError),
}

impl HttpClient {
    pub fn new(cfg: &HttpConfig) -> Result<Self, HttpError> {
        Self::with_policy(cfg, RetryPolicy::from_config(cfg))
    }

    pub fn with_policy(cfg: &HttpConfig, policy: RetryPolicy) -> Result<Self, HttpError> {
        let client = Client::builder()
            .user_agent(cfg.user_agent.clone())
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(|e| HttpError::Build(e.without_url().to_string()))?;
        Ok(HttpClient {
            client,
            policy,
            known_secrets: Vec::new(),
        })
    }

    /// 回傳新的 client，錯誤訊息會額外遮罩這些秘密值。
    pub fn with_known_secrets(self, secrets: Vec<zeroize::Zeroizing<String>>) -> Self {
        HttpClient {
            known_secrets: secrets,
            ..self
        }
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }

    pub fn policy(&self) -> RetryPolicy {
        self.policy
    }

    pub async fn get(&self, url: &str) -> Result<Response, HttpError> {
        self.send(|c| c.get(url)).await
    }

    /// 送出請求；`build` 每次重試都會重新呼叫以建立新的請求。
    pub async fn send<F>(&self, build: F) -> Result<Response, HttpError>
    where
        F: Fn(&Client) -> RequestBuilder,
    {
        let mut attempt = 0;
        loop {
            let request = build(&self.client);
            let (raw_url, idempotent) = describe(&request);
            let url = self.redact(&raw_url);
            match self.classify(request.send().await, &url, idempotent) {
                Attempt::Done(resp) => return Ok(resp),
                Attempt::Fail(err) => return Err(err),
                Attempt::Retry { wait, error } if attempt < self.policy.max_retries => {
                    let delay = match wait {
                        Some(w) if w > self.policy.max_retry_after => {
                            return Err(HttpError::RetryAfterTooLong {
                                url,
                                wait_secs: w.as_secs(),
                            });
                        }
                        Some(w) => w,
                        None => self
                            .policy
                            .backoff(attempt)
                            .min(self.policy.max_retry_after),
                    };
                    tracing::warn!(
                        error = %error,
                        attempt = attempt + 1,
                        wait_ms = delay.as_millis() as u64,
                        "HTTP 重試"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Attempt::Retry { error, .. } => return Err(error),
            }
        }
    }

    /// 429 一律可重試（伺服器未處理請求）；5xx 與連線錯誤只對冪等方法重試，
    /// 避免 POST（例如 Redmine 開案）重複執行。
    fn classify(
        &self,
        result: Result<Response, reqwest::Error>,
        url: &str,
        idempotent: bool,
    ) -> Attempt {
        let resp = match result {
            Ok(r) => r,
            Err(e) if e.is_builder() || e.is_redirect() || !idempotent => {
                return Attempt::Fail(self.request_error(url, &e))
            }
            Err(e) => {
                return Attempt::Retry {
                    wait: None,
                    error: self.request_error(url, &e),
                }
            }
        };
        let status = resp.status();
        let error = HttpError::Status {
            url: url.to_string(),
            status,
        };
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Attempt::Retry {
                wait: retry_after(&resp),
                error,
            };
        }
        if status.is_server_error() && idempotent {
            return Attempt::Retry {
                wait: retry_after(&resp),
                error,
            };
        }
        if status.is_server_error() {
            return Attempt::Fail(error);
        }
        if status.is_client_error() {
            return Attempt::Fail(error);
        }
        Attempt::Done(resp)
    }

    fn request_error(&self, url: &str, err: &reqwest::Error) -> HttpError {
        HttpError::Request {
            url: url.to_string(),
            message: self.error_chain(err),
        }
    }

    /// reqwest 錯誤及其來源鏈，經遮罩。
    fn error_chain(&self, err: &reqwest::Error) -> String {
        let mut parts = vec![err.to_string()];
        let mut source = std::error::Error::source(err);
        while let Some(s) = source {
            parts.push(s.to_string());
            source = s.source();
        }
        self.redact(&parts.join(": "))
    }

    fn redact(&self, text: &str) -> String {
        let secrets: Vec<&str> = self.known_secrets.iter().map(|s| s.as_str()).collect();
        redact_text(text, &secrets)
    }
}

/// 請求的 URL 與方法是否冪等（GET、HEAD、PUT、DELETE、OPTIONS、TRACE）。
fn describe(request: &RequestBuilder) -> (String, bool) {
    request
        .try_clone()
        .and_then(|r| r.build().ok())
        .map(|r| (r.url().to_string(), r.method().is_idempotent()))
        .unwrap_or_else(|| ("<unknown>".to_string(), false))
}

/// 解析 `Retry-After`：秒數或 HTTP-date。
fn retry_after(resp: &Response) -> Option<Duration> {
    let value = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    parse_retry_after(value, chrono::Utc::now())
}

pub fn parse_retry_after(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let secs = at
        .with_timezone(&chrono::Utc)
        .signed_duration_since(now)
        .num_seconds();
    Some(Duration::from_secs(secs.max(0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_exponential() {
        let p = RetryPolicy::from_config(&HttpConfig::default());
        assert_eq!(p.backoff(0), Duration::from_secs(1));
        assert_eq!(p.backoff(1), Duration::from_secs(4));
        assert_eq!(p.backoff(2), Duration::from_secs(16));
    }

    #[test]
    fn parses_retry_after_seconds_and_dates() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-07T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(parse_retry_after("30", now), Some(Duration::from_secs(30)));
        assert_eq!(
            parse_retry_after("Sat, 07 Nov 2026 00:01:00 GMT", now),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            parse_retry_after("Fri, 06 Nov 2026 00:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("soon", now), None);
    }
}
