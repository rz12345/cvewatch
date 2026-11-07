//! 5.3：以 wiremock 模擬 429 → 200 等情境，驗證重試次數與等待。不連線外部服務。

use std::time::{Duration, Instant};

use cvewatch_core::config::HttpConfig;
use cvewatch_core::http::{HttpClient, HttpError, RetryPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(max_retries: u32) -> HttpClient {
    let cfg = HttpConfig {
        timeout_secs: 5,
        max_retries,
        ..HttpConfig::default()
    };
    let policy = RetryPolicy {
        max_retries,
        base_delay: Duration::from_millis(10),
        factor: 4,
        max_retry_after: Duration::from_secs(5),
    };
    HttpClient::with_policy(&cfg, policy).unwrap()
}

async fn received(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

#[tokio::test]
async fn honours_retry_after_on_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/feed"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/feed"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let started = Instant::now();
    let resp = client(3)
        .get(&format!("{}/feed", server.uri()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "至少等待 Retry-After"
    );
    assert_eq!(received(&server).await, 2);
}

#[tokio::test]
async fn retries_5xx_with_backoff() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let started = Instant::now();
    let resp = client(3).get(&server.uri()).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(received(&server).await, 3);
    // 10ms + 40ms
    assert!(started.elapsed() >= Duration::from_millis(50));
}

#[tokio::test]
async fn gives_up_after_max_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = client(2).get(&server.uri()).await.unwrap_err();
    assert_eq!(err.status().map(|s| s.as_u16()), Some(500));
    assert_eq!(received(&server).await, 3, "1 次 + 2 次重試");
}

#[tokio::test]
async fn does_not_retry_client_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let err = client(3).get(&server.uri()).await.unwrap_err();
    assert_eq!(err.status().map(|s| s.as_u16()), Some(404));
    assert_eq!(received(&server).await, 1);
}

#[tokio::test]
async fn refuses_excessive_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "3600"))
        .mount(&server)
        .await;

    let started = Instant::now();
    let err = client(3).get(&server.uri()).await.unwrap_err();
    assert!(matches!(
        err,
        HttpError::RetryAfterTooLong {
            wait_secs: 3600,
            ..
        }
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn sends_configured_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::header(
            "user-agent",
            cvewatch_core::config::DEFAULT_USER_AGENT,
        ))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let resp = client(0).get(&server.uri()).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn api_key_is_masked_in_connection_errors() {
    // 127.0.0.1:9 (discard) 通常沒有服務監聽，連線立即失敗。
    let url = "http://127.0.0.1:9/rest?apiKey=TOPSECRET123&x=1";
    let err = client(0)
        .with_known_secrets(vec![zeroize::Zeroizing::new("TOPSECRET123".into())])
        .get(url)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(!msg.contains("TOPSECRET123"), "{msg}");
    assert!(msg.contains("apiKey=***"), "{msg}");
}

#[tokio::test]
async fn post_is_not_retried_on_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let uri = server.uri();
    let err = client(3).send(|c| c.post(&uri)).await.unwrap_err();
    assert_eq!(err.status().map(|s| s.as_u16()), Some(500));
    assert_eq!(received(&server).await, 1, "POST 不重試，避免重複開案");
}

#[tokio::test]
async fn post_is_retried_on_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    let uri = server.uri();
    let resp = client(3).send(|c| c.post(&uri)).await.unwrap();
    assert_eq!(resp.status(), 201);
    assert_eq!(received(&server).await, 2);
}

#[tokio::test]
async fn honours_retry_after_on_503() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let started = Instant::now();
    client(3).get(&server.uri()).await.unwrap();
    assert!(started.elapsed() >= Duration::from_secs(1));
}

#[tokio::test]
async fn computed_backoff_is_capped_not_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let cfg = HttpConfig::default();
    let policy = RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(50),
        factor: 100,
        max_retry_after: Duration::from_millis(100),
    };
    let resp = HttpClient::with_policy(&cfg, policy)
        .unwrap()
        .get(&server.uri())
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
