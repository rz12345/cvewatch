//! Redmine REST API（同步 HTTP，rustls）：建立議題、加註記、查專案/tracker/優先權/成員、連線測試。
//!
//! | 用途 | API |
//! |---|---|
//! | 建立議題 | `POST /issues.json` |
//! | 加註記 | `PUT /issues/{id}.json`（`notes`） |
//! | 專案 | `GET /projects.json?limit=100&offset=N`（分頁） |
//! | 專案 tracker | `GET /projects/{id}.json?include=trackers` |
//! | 優先權 | `GET /enumerations/issue_priorities.json` |
//! | 成員 | `GET /projects/{id}/memberships.json?limit=100&offset=N` |
//!
//! API key 以 `X-Redmine-API-Key` header 傳送；網址為 http（明文）時拒絕送出任何請求，也不跟隨轉址
//! （reqwest 跨主機轉址時不會移除自訂 header，跟隨會把 API key 送到別的主機）。
//! HTTP 401、403、404、422 為不可重試的錯誤，其他失敗（連線、逾時、5xx 等）交給送出佇列重送。

use std::path::Path;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use zeroize::Zeroizing;

use super::redmine_content::IssueDraft;
use super::settings::RedmineConfig;
use super::tls::{load_ca_file, PLAINTEXT_SECRET_REFUSED_REDMINE};
use super::validate;
use crate::config::HttpConfig;

pub const API_KEY_HEADER: &str = "X-Redmine-API-Key";
/// 清單分頁大小（Redmine 上限 100）。
pub const PAGE_LIMIT: usize = 100;
/// 分頁讀取上限，避免伺服器回應異常時無限迴圈。
pub const MAX_PAGES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedmineError {
    /// 可重送（連線失敗、逾時、5xx…）
    Transient(String),
    /// 不可重試（401/403/422、轉址）
    Permanent(String),
    /// 不可重試：HTTP 404（例如議題已在 Redmine 刪除）
    NotFound(String),
}

impl RedmineError {
    pub fn message(&self) -> &str {
        match self {
            RedmineError::Transient(m) | RedmineError::Permanent(m) | RedmineError::NotFound(m) => {
                m
            }
        }
    }
}

impl std::fmt::Display for RedmineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// 建立議題與加註記；正式為 [`RedmineClient`]，測試可替換。
pub trait RedmineApi {
    fn create_issue(&self, draft: &IssueDraft) -> Result<u64, RedmineError>;
    /// 專案中主旨含此 CVE ID 的議題（含已關閉）；重送建立前確認是否已建立過。
    fn find_issue(&self, cve_id: &str) -> Result<Option<u64>, RedmineError>;
    fn add_note(&self, issue_id: u64, notes: &str) -> Result<(), RedmineError>;
}

/// 清單項目（顯示為「名稱 (#ID)」）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Named {
    pub id: u64,
    pub name: String,
}

impl Named {
    pub fn label(&self) -> String {
        format!("{} (#{})", self.name, self.id)
    }
}

/// 連線測試結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    pub project: Named,
    pub tracker: Named,
}

pub struct RedmineSettings<'a> {
    pub redmine: &'a RedmineConfig,
    pub api_key: Option<Zeroizing<String>>,
    pub ca_file: Option<&'a Path>,
    pub allow_insecure: bool,
    pub http: &'a HttpConfig,
}

pub struct RedmineClient {
    client: Client,
    base: url::Url,
    api_key: Zeroizing<String>,
    config: RedmineConfig,
}

impl RedmineClient {
    /// 驗證網址、API key 與 CA 檔並建立 client（尚未連線）。
    pub fn new(s: &RedmineSettings<'_>) -> Result<Self, String> {
        let base = validate::redmine_url(&s.redmine.base_url, s.allow_insecure)?;
        if base.scheme() != "https" {
            return Err(PLAINTEXT_SECRET_REFUSED_REDMINE.into());
        }
        let api_key = s.api_key.clone().ok_or_else(|| {
            "Credential Manager 中沒有 redmine_api_key；請在設定畫面輸入，或執行 \
             `cvewatch config set-secret redmine_api_key`"
                .to_string()
        })?;
        let mut builder = Client::builder()
            .user_agent(s.http.user_agent.clone())
            .timeout(Duration::from_secs(s.http.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true);
        if let Some(path) = s.ca_file {
            let mut certs = Vec::new();
            for pem in load_ca_file(path)? {
                certs.push(reqwest::Certificate::from_pem(pem.as_bytes()).map_err(|e| {
                    format!(
                        "CA 檔 {} 的憑證無法讀取：{}",
                        path.display(),
                        e.without_url()
                    )
                })?);
            }
            builder = builder.tls_certs_merge(certs);
        }
        let client = builder
            .build()
            .map_err(|e| format!("無法建立 HTTP client：{}", e.without_url()))?;
        Ok(RedmineClient {
            client,
            base,
            api_key,
            config: s.redmine.clone(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.as_str().trim_end_matches('/'))
    }

    fn send(&self, req: RequestBuilder) -> Result<Response, RedmineError> {
        let resp = req
            .header(API_KEY_HEADER, self.api_key.as_str())
            .send()
            .map_err(|e| RedmineError::Transient(request_error(&e)))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let url = resp.url().path().to_string();
        let body = resp.text().unwrap_or_default();
        Err(status_error(status, &url, &body))
    }

    fn get_json(&self, path: &str) -> Result<Value, RedmineError> {
        self.send(self.client.get(self.url(path)))?
            .json()
            .map_err(|e| {
                RedmineError::Transient(format!("Redmine 回應格式錯誤：{}", e.without_url()))
            })
    }

    /// 所有專案（分頁）。
    pub fn projects(&self) -> Result<Vec<Named>, RedmineError> {
        self.paged("/projects.json", "projects", parse_named)
    }

    /// 專案資訊與其啟用的 tracker。
    pub fn project(&self, project: &str) -> Result<(Named, Vec<Named>), RedmineError> {
        let v = self.get_json(&format!(
            "/projects/{}.json?include=trackers",
            encode(project)
        ))?;
        let p = &v["project"];
        let named = parse_named(p).ok_or_else(|| bad_format("project"))?;
        let trackers = p["trackers"]
            .as_array()
            .map(|a| a.iter().filter_map(parse_named).collect())
            .unwrap_or_default();
        Ok((named, trackers))
    }

    pub fn priorities(&self) -> Result<Vec<Named>, RedmineError> {
        let v = self.get_json("/enumerations/issue_priorities.json")?;
        Ok(v["issue_priorities"]
            .as_array()
            .map(|a| a.iter().filter_map(parse_named).collect())
            .unwrap_or_default())
    }

    /// 專案成員（使用者與群組，皆可指派）。
    pub fn members(&self, project: &str) -> Result<Vec<Named>, RedmineError> {
        let path = format!("/projects/{}/memberships.json", encode(project));
        let mut out = self.paged(&path, "memberships", |m| {
            parse_named(&m["user"]).or_else(|| parse_named(&m["group"]))
        })?;
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.dedup_by_key(|n| n.id);
        Ok(out)
    }

    fn paged(
        &self,
        path: &str,
        key: &str,
        item: impl Fn(&Value) -> Option<Named>,
    ) -> Result<Vec<Named>, RedmineError> {
        let mut out = Vec::new();
        // 伺服器可能限制每頁筆數，下一頁的 offset 以實際收到的筆數累計
        let mut offset = 0usize;
        for _ in 0..MAX_PAGES {
            let v = self.get_json(&format!("{path}?limit={PAGE_LIMIT}&offset={offset}"))?;
            let items = v[key].as_array().cloned().unwrap_or_default();
            out.extend(items.iter().filter_map(&item));
            offset += items.len();
            let total = v["total_count"].as_u64().map_or(0, |t| t as usize);
            if items.is_empty() || offset >= total {
                break;
            }
        }
        Ok(out)
    }

    /// 驗證 API key、專案與 tracker 可用（不建立議題）。
    pub fn test(&self) -> Result<TestReport, RedmineError> {
        self.config.ready().map_err(RedmineError::Permanent)?;
        let (project, trackers) = self.project(&self.config.project_id)?;
        let tracker = trackers
            .iter()
            .find(|t| u64::from(self.config.tracker_id) == t.id)
            .cloned()
            .ok_or_else(|| {
                let available: Vec<String> = trackers.iter().map(Named::label).collect();
                RedmineError::Permanent(format!(
                    "專案「{}」未啟用 tracker #{}；可用：{}",
                    project.name,
                    self.config.tracker_id,
                    if available.is_empty() {
                        "（無）".into()
                    } else {
                        available.join("、")
                    }
                ))
            })?;
        Ok(TestReport { project, tracker })
    }
}

impl RedmineApi for RedmineClient {
    fn create_issue(&self, draft: &IssueDraft) -> Result<u64, RedmineError> {
        self.config.ready().map_err(RedmineError::Transient)?;
        let body = issue_body(&self.config, draft);
        let v: Value = self
            .send(self.client.post(self.url("/issues.json")).json(&body))?
            .json()
            .map_err(|e| {
                RedmineError::Transient(format!("Redmine 回應格式錯誤：{}", e.without_url()))
            })?;
        v["issue"]["id"]
            .as_u64()
            .ok_or_else(|| bad_format("issue.id"))
    }

    fn find_issue(&self, cve_id: &str) -> Result<Option<u64>, RedmineError> {
        let path = format!(
            "/issues.json?project_id={}&status_id=*&set_filter=1&f[]=subject&op[subject]=~&v[subject][]={}&limit={PAGE_LIMIT}",
            encode(&self.config.project_id),
            encode(cve_id)
        );
        let v = self.get_json(&path)?;
        Ok(v["issues"].as_array().and_then(|issues| {
            issues
                .iter()
                .find(|i| {
                    i["subject"]
                        .as_str()
                        .is_some_and(|s| mentions_cve(s, cve_id))
                })
                .and_then(|i| i["id"].as_u64())
        }))
    }

    fn add_note(&self, issue_id: u64, notes: &str) -> Result<(), RedmineError> {
        let body = json!({ "issue": { "notes": notes } });
        self.send(
            self.client
                .put(self.url(&format!("/issues/{issue_id}.json")))
                .json(&body),
        )
        .map(|_| ())
    }
}

/// 建立議題的 JSON（專案、tracker、優先權、指派依當下設定）。
pub fn issue_body(cfg: &RedmineConfig, draft: &IssueDraft) -> Value {
    let project: Value = match cfg.project_id.trim().parse::<u64>() {
        Ok(id) => json!(id),
        Err(_) => json!(cfg.project_id.trim()),
    };
    let mut issue = json!({
        "project_id": project,
        "tracker_id": cfg.tracker_id,
        "subject": draft.subject,
        "description": draft.description,
    });
    if let Some(p) = draft.priority().and_then(|p| cfg.priority_id(p)) {
        issue["priority_id"] = json!(p);
    }
    if cfg.assigned_to_id != 0 {
        issue["assigned_to_id"] = json!(cfg.assigned_to_id);
    }
    json!({ "issue": issue })
}

/// 文字中出現完整的 CVE ID（前後不是英數字，避免 CVE-2026-1234 比對到 CVE-2026-12345）。
pub fn mentions_cve(text: &str, cve_id: &str) -> bool {
    let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
    text.match_indices(cve_id).any(|(i, m)| {
        boundary(text[..i].chars().next_back()) && boundary(text[i + m.len()..].chars().next())
    })
}

fn parse_named(v: &Value) -> Option<Named> {
    Some(Named {
        id: v["id"].as_u64()?,
        name: v["name"].as_str()?.to_string(),
    })
}

fn encode(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.trim().as_bytes()).collect()
}

fn bad_format(field: &str) -> RedmineError {
    RedmineError::Transient(format!("Redmine 回應缺少 {field}"))
}

fn request_error(e: &reqwest::Error) -> String {
    let detail = crate::process::error_chain(e);
    if e.is_timeout() {
        format!("Redmine 連線逾時：{detail}")
    } else if detail.contains("certificate") || detail.contains("UnknownIssuer") {
        format!("Redmine 憑證無法驗證（內部 CA 請設定 ca_file）：{detail}")
    } else {
        format!("無法連線 Redmine：{detail}")
    }
}

/// HTTP 錯誤：401/403/404/422 與轉址不重試；附上 Redmine 的錯誤訊息（`errors` 陣列）。
pub fn status_error(status: StatusCode, path: &str, body: &str) -> RedmineError {
    let errors: Vec<String> = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["errors"].as_array().cloned())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let detail = if errors.is_empty() {
        String::new()
    } else {
        format!("：{}", errors.join("；"))
    };
    let hint = match status.as_u16() {
        401 => "（API key 無效）",
        403 => "（沒有權限）",
        404 => "（找不到；請確認專案或議題）",
        422 => "（Redmine 拒絕內容，例如 tracker 不存在或未啟用）",
        300..=399 => "（伺服器要求轉址；為避免 API key 送往其他主機不跟隨，請確認 base_url）",
        _ => "",
    };
    let message = format!("Redmine HTTP {}{hint}（{path}）{detail}", status.as_u16());
    match status.as_u16() {
        404 => RedmineError::NotFound(message),
        300..=399 | 401 | 403 | 422 => RedmineError::Permanent(message),
        _ => RedmineError::Transient(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_classification() {
        let e = status_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "/issues.json",
            r#"{"errors":["Tracker is not included in the list"]}"#,
        );
        assert!(matches!(e, RedmineError::Permanent(_)));
        assert!(e.message().contains("Tracker is not included"), "{e}");
        assert!(matches!(
            status_error(StatusCode::BAD_GATEWAY, "/x", ""),
            RedmineError::Transient(_)
        ));
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, "/x", ""),
            RedmineError::Permanent(_)
        ));
        assert!(matches!(
            status_error(StatusCode::FOUND, "/x", ""),
            RedmineError::Permanent(_)
        ));
        assert!(matches!(
            status_error(StatusCode::NOT_FOUND, "/issues/1.json", ""),
            RedmineError::NotFound(_)
        ));
    }

    #[test]
    fn cve_must_match_whole_id() {
        assert!(mentions_cve(
            "[cvewatch][P1] CVE-2026-1234 PAN-OS — FW-01",
            "CVE-2026-1234"
        ));
        assert!(!mentions_cve(
            "[cvewatch][P1] CVE-2026-12345 PAN-OS",
            "CVE-2026-1234"
        ));
        assert!(!mentions_cve("XCVE-2026-1234", "CVE-2026-1234"));
        assert!(mentions_cve("CVE-2026-1234", "CVE-2026-1234"));
    }

    #[test]
    fn issue_body_maps_priority_and_assignee() {
        let cfg = RedmineConfig {
            enabled: true,
            base_url: "https://rm.example".into(),
            project_id: "12".into(),
            tracker_id: 3,
            priority_map: [("P1".to_string(), 5)].into_iter().collect(),
            assigned_to_id: 7,
            add_note_on_triage: true,
        };
        let draft = IssueDraft {
            cve_id: "CVE-1".into(),
            subject: "s".into(),
            description: "d".into(),
            priority: "P1".into(),
        };
        let v = issue_body(&cfg, &draft);
        assert_eq!(v["issue"]["project_id"], json!(12));
        assert_eq!(v["issue"]["priority_id"], json!(5));
        assert_eq!(v["issue"]["assigned_to_id"], json!(7));
        let p2 = IssueDraft {
            priority: "P2".into(),
            ..draft
        };
        let named = RedmineConfig {
            project_id: "infra-sec".into(),
            assigned_to_id: 0,
            ..cfg
        };
        let v = issue_body(&named, &p2);
        assert_eq!(v["issue"]["project_id"], json!("infra-sec"));
        assert!(v["issue"].get("priority_id").is_none());
        assert!(v["issue"].get("assigned_to_id").is_none());
    }
}
