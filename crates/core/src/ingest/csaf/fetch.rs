//! CSAF 文件下載與完整性：SHA-256/512 雜湊檔（存在時必須相符）、TLP 過濾、csaf_version。

use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};

use super::discovery::{tlp_allowed, Candidate};
use crate::error::CoreError;
use crate::ingest::source::SyncCtx;

const SHA512_SUFFIX: &str = ".sha512";
const SHA256_SUFFIX: &str = ".sha256";
const SUPPORTED_VERSIONS: [&str; 2] = ["2.0", "2.1"];

/// 通過檢查的文件。
#[derive(Debug, Clone)]
pub struct Document {
    pub url: String,
    pub raw: Vec<u8>,
    pub json: Value,
    pub tracking_id: String,
    pub version: String,
    pub title: Option<String>,
    pub tlp: Option<String>,
    pub published: Option<String>,
    pub updated: Option<String>,
    /// None：沒有雜湊檔；Some(true)：雜湊相符
    pub sha_ok: Option<bool>,
}

/// 下載結果：採用或略過（附原因）。
#[derive(Debug)]
pub enum Fetched {
    Accepted(Box<Document>),
    Skipped(String),
}

fn hex_digest(url: &str, bytes: &[u8]) -> Option<String> {
    let digest = if url.ends_with(SHA512_SUFFIX) {
        Sha512::digest(bytes).to_vec()
    } else if url.ends_with(SHA256_SUFFIX) {
        Sha256::digest(bytes).to_vec()
    } else {
        return None;
    };
    Some(data_encoding::HEXLOWER.encode(&digest))
}

/// 雜湊檔內容：`<hex>  <檔名>` 或只有 hex。
fn expected_hash(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?.to_lowercase();
    token
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then_some(token)
}

/// 依序嘗試雜湊檔；第一個存在者決定結果。
async fn verify(
    ctx: &SyncCtx<'_>,
    c: &Candidate,
    raw: &[u8],
) -> Result<Result<Option<bool>, String>, CoreError> {
    let hash_urls = if c.hash_urls.is_empty() {
        vec![
            format!("{}{SHA512_SUFFIX}", c.url),
            format!("{}{SHA256_SUFFIX}", c.url),
        ]
    } else {
        let mut urls = c.hash_urls.clone();
        // 優先 SHA-512
        urls.sort_by_key(|u| !u.ends_with(SHA512_SUFFIX));
        urls
    };
    for url in hash_urls {
        let resp = match ctx.http.get(&url).await {
            Ok(r) => r,
            // 404（或物件不存在時回 403 的 CDN）視為沒有雜湊檔
            Err(e)
                if matches!(
                    e.status(),
                    Some(StatusCode::NOT_FOUND | StatusCode::FORBIDDEN)
                ) =>
            {
                continue
            }
            // 其他錯誤讓整個來源失敗、游標不變，下次重試（不可略過後讓游標越過這份文件）
            Err(e) => return Err(e.into()),
        };
        let text = String::from_utf8_lossy(&ctx.http.bytes(resp).await?).to_string();
        let (Some(expected), Some(actual)) = (expected_hash(&text), hex_digest(&url, raw)) else {
            return Ok(Err("雜湊檔格式錯誤".to_string()));
        };
        return Ok(if expected == actual {
            Ok(Some(true))
        } else {
            Err("雜湊不符".to_string())
        });
    }
    Ok(Ok(None))
}

fn text_at(v: &Value, pointer: &str) -> Option<String> {
    v.pointer(pointer)
        .and_then(|x| match x {
            Value::String(s) => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

pub async fn fetch(ctx: &SyncCtx<'_>, c: &Candidate) -> Result<Fetched, CoreError> {
    let resp = match ctx.http.get(&c.url).await {
        Ok(r) => r,
        // 列表中有、但文件不存在：略過（其他錯誤讓整個來源失敗，保留游標下次重試）
        Err(e) if e.status() == Some(StatusCode::NOT_FOUND) => {
            return Ok(Fetched::Skipped(format!("{}：文件不存在（404）", c.url)))
        }
        Err(e) => return Err(e.into()),
    };
    let raw = ctx.http.bytes(resp).await?;
    let sha_ok = match verify(ctx, c, &raw).await? {
        Ok(v) => v,
        Err(reason) => return Ok(Fetched::Skipped(format!("{}：{reason}", c.url))),
    };
    let json: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(e) => return Ok(Fetched::Skipped(format!("{}：不是 JSON（{e}）", c.url))),
    };
    let version = text_at(&json, "/document/csaf_version").unwrap_or_default();
    if !SUPPORTED_VERSIONS.contains(&version.as_str()) {
        return Ok(Fetched::Skipped(format!(
            "{}：不支援的 csaf_version `{version}`",
            c.url
        )));
    }
    let tlp = text_at(&json, "/document/distribution/tlp/label");
    if !tlp_allowed(tlp.as_deref()) {
        return Ok(Fetched::Skipped(format!(
            "{}：TLP:{} 不採用",
            c.url,
            tlp.unwrap_or_default()
        )));
    }
    let (Some(tracking_id), Some(doc_version)) = (
        text_at(&json, "/document/tracking/id"),
        text_at(&json, "/document/tracking/version"),
    ) else {
        return Ok(Fetched::Skipped(format!(
            "{}：缺少 tracking id 或 version",
            c.url
        )));
    };
    Ok(Fetched::Accepted(Box::new(Document {
        url: c.url.clone(),
        title: text_at(&json, "/document/title"),
        published: text_at(&json, "/document/tracking/initial_release_date"),
        updated: text_at(&json, "/document/tracking/current_release_date"),
        tracking_id,
        version: doc_version,
        tlp,
        sha_ok,
        raw,
        json,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_file_formats() {
        assert_eq!(
            expected_hash("D3DF30  file.json\n").as_deref(),
            Some("d3df30")
        );
        assert_eq!(expected_hash("abc").as_deref(), Some("abc"));
        assert_eq!(expected_hash("<html>"), None);
        assert_eq!(expected_hash(""), None);
    }

    #[test]
    fn digest_by_suffix() {
        assert_eq!(
            hex_digest("x.sha256", b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hex_digest("x.sha512", b"").unwrap().len(), 128);
        assert!(hex_digest("x.asc", b"").is_none());
    }
}
