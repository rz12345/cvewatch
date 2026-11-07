//! 6.6：tracing 呼叫不得帶出元件版本、命中明細、匯入原始內容或 HTTP 回應本文。
//!
//! 以文字掃描所有 crate 的 `src/`，在 `trace!`…`error!` 巨集參數中尋找禁用欄位名稱。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

/// 禁止出現在日誌欄位或格式字串中的名稱。
const FORBIDDEN: &[&str] = &[
    "version",
    "versions",
    "component_version",
    "installed_version",
    "body",
    "response_body",
    "response_text",
    "raw",
    "raw_row",
    "row_content",
    "cell",
    "cells",
    "hit",
    "hits",
    "match_detail",
    "evidence",
];

static TRACING_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\b(?:tracing::)?(?:trace|debug|info|warn|error)!\s*\((.*?)\)\s*;").unwrap()
});

static IDENT_USE: LazyLock<Regex> = LazyLock::new(|| {
    let names = FORBIDDEN.join("|");
    // 欄位 `name =`、`%name` / `?name`、簡寫欄位 `, name,`、格式字串 `{name}` / `{name:?}`
    Regex::new(&format!(
        r"(?:\b({names})\s*=[^=])|(?:[%?]\s*({names})\b)|(?:(?:^|[,(])\s*({names})\s*(?:,|$))|(?:\{{({names})[}}:])"
    ))
    .unwrap()
});

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn violations(source: &str) -> Vec<String> {
    TRACING_CALL
        .captures_iter(source)
        .filter_map(|cap| {
            let args = cap.get(1)?.as_str();
            IDENT_USE.is_match(args).then(|| args.trim().to_string())
        })
        .collect()
}

#[test]
fn lint_detects_forbidden_fields() {
    assert_eq!(violations(r#"tracing::info!(version = %v, "x");"#).len(), 1);
    assert_eq!(violations(r#"debug!(?body, "x");"#).len(), 1);
    assert_eq!(violations(r#"warn!(asset_tag, version, "x");"#).len(), 1);
    assert_eq!(violations(r#"info!("got {body}");"#).len(), 1);
    assert_eq!(violations(r#"error!("got {raw:?}");"#).len(), 1);
}

#[test]
fn lint_allows_counts_and_ids() {
    assert!(violations(r#"info!(rows = n, asset_tag = %t, cve = %id, "ok");"#).is_empty());
    assert!(violations(r#"info!(from = a, to = b, "資料庫 schema 已升級");"#).is_empty());
    assert!(violations(r#"warn!(body_len = n, body_sha256 = %h, "x");"#).is_empty());
}

#[test]
fn no_tracing_call_logs_sensitive_fields() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for member in std::fs::read_dir(&crates_dir).unwrap().flatten() {
        let src = member.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    assert!(!files.is_empty(), "找不到原始碼");

    let found: Vec<String> = files
        .iter()
        .flat_map(|f| {
            let text = std::fs::read_to_string(f).unwrap();
            violations(&text)
                .into_iter()
                .map(move |v| format!("{}: {v}", f.display()))
        })
        .collect();
    assert!(found.is_empty(), "日誌含禁用欄位：\n{}", found.join("\n"));
}
