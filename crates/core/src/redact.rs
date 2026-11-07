//! 日誌與錯誤訊息的秘密遮罩。

use std::sync::LazyLock;

use regex::Regex;

pub const MASK: &str = "***";

/// 遮罩時忽略過短的已知秘密，避免把常見字元全部換掉。
const MIN_KNOWN_SECRET_LEN: usize = 4;

const SENSITIVE_HEADERS: [&str; 9] = [
    "authorization",
    "proxy-authorization",
    "apikey",
    "api-key",
    "x-api-key",
    "x-redmine-api-key",
    "x-auth-token",
    "cookie",
    "set-cookie",
];

/// 名稱含 key / token / secret / pass / auth / sig 的 query 參數。
static SENSITIVE_QUERY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)([?&;][a-z0-9_.\-]*(?:key|token|secret|pass(?:word|wd)?|auth|sig(?:nature)?)[a-z0-9_.\-]*=)[^&#\s)"'<>]*"#,
    )
    .expect("固定的正規表示式必定可編譯")
});

/// `scheme://user:password@host` 的密碼部分。
static URL_USERINFO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z][a-z0-9+.\-]*://[^/@\s:]+:)[^/@\s]+@")
        .expect("固定的正規表示式必定可編譯")
});

/// 遮罩 URL（或含 URL 的文字）中的敏感 query 參數與帳密。
pub fn redact_url(text: &str) -> String {
    let masked = SENSITIVE_QUERY.replace_all(text, format!("${{1}}{MASK}"));
    URL_USERINFO
        .replace_all(&masked, format!("${{1}}{MASK}@"))
        .into_owned()
}

/// 敏感 header 的值以 `***` 取代，其他原樣回傳。
pub fn redact_header(name: &str, value: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if SENSITIVE_HEADERS.contains(&lower.as_str()) {
        MASK.to_string()
    } else {
        value.to_string()
    }
}

/// 先遮罩 URL 形式的秘密，再將已知秘密值逐一取代。
pub fn redact_text(text: &str, known_secrets: &[&str]) -> String {
    known_secrets
        .iter()
        .filter(|s| s.len() >= MIN_KNOWN_SECRET_LEN)
        .fold(redact_url(text), |acc, secret| acc.replace(secret, MASK))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_api_key_query_param() {
        let url =
            "https://services.nvd.nist.gov/rest/json/cves/2.0?apiKey=SECRET123&resultsPerPage=10";
        let out = redact_url(url);
        assert!(!out.contains("SECRET123"), "{out}");
        assert!(out.contains("apiKey=***"), "{out}");
        assert!(out.contains("resultsPerPage=10"), "{out}");
    }

    #[test]
    fn masks_various_param_names() {
        for name in [
            "api_key",
            "key",
            "access_token",
            "password",
            "client_secret",
            "sig",
        ] {
            let out = redact_url(&format!("https://h/p?a=1&{name}=VALUE"));
            assert!(!out.contains("VALUE"), "{name}: {out}");
        }
    }

    #[test]
    fn keeps_non_sensitive_params() {
        let url = "https://h/p?cveId=CVE-2024-1&page=2";
        assert_eq!(redact_url(url), url);
    }

    #[test]
    fn masks_userinfo_password() {
        let out = redact_url("smtps://alice:hunter22@mail.example.org:465");
        assert_eq!(out, "smtps://alice:***@mail.example.org:465");
    }

    #[test]
    fn masks_url_embedded_in_error_text() {
        let out = redact_url("error sending request for url (https://h/x?token=abc): timeout");
        assert!(!out.contains("abc"), "{out}");
        assert!(out.ends_with(": timeout"), "{out}");
    }

    #[test]
    fn masks_sensitive_headers_case_insensitive() {
        assert_eq!(redact_header("X-Redmine-API-Key", "k"), MASK);
        assert_eq!(redact_header("Authorization", "Bearer x"), MASK);
        assert_eq!(redact_header("apiKey", "x"), MASK);
        assert_eq!(
            redact_header("Accept", "application/json"),
            "application/json"
        );
    }

    #[test]
    fn masks_known_secret_values() {
        let out = redact_text(
            "header value was S3cretValue, again S3cretValue",
            &["S3cretValue"],
        );
        assert_eq!(out, "header value was ***, again ***");
    }

    #[test]
    fn ignores_too_short_known_secrets() {
        assert_eq!(redact_text("abc", &["a"]), "abc");
    }
}
