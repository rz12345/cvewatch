//! CSAF 公告探索：`provider-metadata.json`（ROLIE feed 或目錄式 `changes.csv`）與 RSS。

use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use serde::Deserialize;
use serde_json::Value;

use crate::config::{CsafDiscovery, CsafSourceConfig, CSAF_ID_PLACEHOLDER};
use crate::error::CoreError;
use crate::ingest::source::SyncCtx;

/// 允許的 TLP 標示（未標示也接受）。
pub const ALLOWED_TLP: [&str; 2] = ["WHITE", "CLEAR"];
const CHANGES_CSV: &str = "changes.csv";

/// 一份待下載的 CSAF 文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub url: String,
    pub updated: DateTime<Utc>,
    /// ROLIE entry 明確列出的雜湊檔；空時改試 `<url>.sha512` / `<url>.sha256`
    pub hash_urls: Vec<String>,
}

pub fn tlp_allowed(label: Option<&str>) -> bool {
    label.is_none_or(|l| ALLOWED_TLP.iter().any(|a| a.eq_ignore_ascii_case(l.trim())))
}

/// RFC 3339 或 RFC 2822。
pub fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    let t = text.trim();
    DateTime::parse_from_rfc3339(t)
        .or_else(|_| DateTime::parse_from_rfc2822(t))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

async fn get_bytes(ctx: &SyncCtx<'_>, url: &str) -> Result<Vec<u8>, CoreError> {
    let resp = ctx.http.get(url).await?;
    Ok(ctx.http.bytes(resp).await?)
}

/// 列出時間晚於 `since` 的文件（依時間由舊到新）。
pub async fn discover(
    ctx: &SyncCtx<'_>,
    cfg: &CsafSourceConfig,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<Candidate>, CoreError> {
    let mut found = match cfg.discovery {
        CsafDiscovery::Rss => rss(ctx, cfg).await?,
        CsafDiscovery::ProviderMetadata => provider_metadata(ctx, &cfg.url).await?,
    };
    found.retain(|c| since.is_none_or(|s| c.updated > s));
    found.sort_by(|a, b| a.updated.cmp(&b.updated).then_with(|| a.url.cmp(&b.url)));
    // 同一文件出現多次（例如多個 feed）時保留最新的一筆
    let mut dedup: Vec<Candidate> = Vec::new();
    for c in found.into_iter().rev() {
        if !dedup.iter().any(|d| d.url == c.url) {
            dedup.push(c);
        }
    }
    dedup.reverse();
    Ok(dedup)
}

// ── RSS ─────────────────────────────────────────────────────

async fn rss(ctx: &SyncCtx<'_>, cfg: &CsafSourceConfig) -> Result<Vec<Candidate>, CoreError> {
    let template = cfg.csaf_url_template.as_deref().unwrap_or_default();
    let body = get_bytes(ctx, &cfg.url).await?;
    let text = String::from_utf8_lossy(&body);
    let items = parse_rss(&text)?;
    Ok(items
        .into_iter()
        .filter_map(|(link, date)| {
            let id = link.trim_end_matches('/').rsplit('/').next()?.trim();
            if id.is_empty() {
                return None;
            }
            let encoded: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
            Some(Candidate {
                url: template.replace(CSAF_ID_PLACEHOLDER, &encoded),
                updated: parse_time(&date)?,
                hash_urls: Vec::new(),
            })
        })
        .collect())
}

/// RSS 2.0 的 item（link, pubDate）。
pub fn parse_rss(xml: &str) -> Result<Vec<(String, String)>, CoreError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut items = Vec::new();
    let mut in_item = false;
    let mut field: Option<&'static str> = None;
    let mut buf = String::new();
    let (mut link, mut date) = (String::new(), String::new());
    loop {
        let event = reader
            .read_event()
            .map_err(|e| CoreError::Invalid(format!("RSS 格式錯誤：{e}")))?;
        match event {
            Event::Start(e) => match e.name().as_ref() {
                "item" => {
                    in_item = true;
                    link.clear();
                    date.clear();
                }
                "link" if in_item => (field, buf) = (Some("link"), String::new()),
                "pubDate" if in_item => (field, buf) = (Some("date"), String::new()),
                _ => {}
            },
            Event::Text(t) if field.is_some() => buf.push_str(&t.xml10_content()),
            Event::CData(t) if field.is_some() => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    buf.push(c);
                } else {
                    buf.push_str(match r.into_inner().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "link" | "pubDate" if field.is_some() => {
                    let value = std::mem::take(&mut buf).trim().to_string();
                    match field.take() {
                        Some("link") => link = value,
                        _ => date = value,
                    }
                }
                "item" => {
                    in_item = false;
                    if !link.is_empty() {
                        items.push((link.clone(), date.clone()));
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(items)
}

// ── provider-metadata ───────────────────────────────────────

#[derive(Deserialize)]
struct ProviderMetadata {
    #[serde(default)]
    distributions: Vec<Distribution>,
}

#[derive(Deserialize)]
struct Distribution {
    directory_url: Option<String>,
    rolie: Option<Rolie>,
}

#[derive(Deserialize)]
struct Rolie {
    #[serde(default)]
    feeds: Vec<Feed>,
}

#[derive(Deserialize)]
struct Feed {
    tlp_label: Option<String>,
    url: String,
}

async fn provider_metadata(ctx: &SyncCtx<'_>, url: &str) -> Result<Vec<Candidate>, CoreError> {
    let meta: ProviderMetadata = serde_json::from_slice(&get_bytes(ctx, url).await?)
        .map_err(|e| CoreError::Invalid(format!("provider-metadata 格式錯誤：{e}")))?;
    let mut out = Vec::new();
    for d in &meta.distributions {
        if let Some(rolie) = &d.rolie {
            for feed in rolie
                .feeds
                .iter()
                .filter(|f| tlp_allowed(f.tlp_label.as_deref()))
            {
                out.extend(rolie_feed(ctx, &feed.url).await?);
            }
        } else if let Some(dir) = &d.directory_url {
            out.extend(changes_csv(ctx, dir).await?);
        }
    }
    Ok(out)
}

async fn rolie_feed(ctx: &SyncCtx<'_>, url: &str) -> Result<Vec<Candidate>, CoreError> {
    let feed: Value = serde_json::from_slice(&get_bytes(ctx, url).await?)
        .map_err(|e| CoreError::Invalid(format!("ROLIE feed 格式錯誤：{e}")))?;
    let entries = feed
        .pointer("/feed/entry")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(entries.iter().filter_map(rolie_entry).collect())
}

fn rolie_entry(entry: &Value) -> Option<Candidate> {
    let links = entry.get("link").and_then(Value::as_array);
    let href_of = |rel: &str| -> Vec<String> {
        links
            .into_iter()
            .flatten()
            .filter(|l| l.get("rel").and_then(Value::as_str) == Some(rel))
            .filter_map(|l| l.get("href").and_then(Value::as_str).map(str::to_string))
            .collect()
    };
    let url = entry
        .pointer("/content/src")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| href_of("self").into_iter().next())?;
    let updated = entry
        .get("updated")
        .or_else(|| entry.get("published"))
        .and_then(Value::as_str)
        .and_then(parse_time)?;
    Some(Candidate {
        url,
        updated,
        hash_urls: href_of("hash"),
    })
}

async fn changes_csv(ctx: &SyncCtx<'_>, directory: &str) -> Result<Vec<Candidate>, CoreError> {
    let base = if directory.ends_with('/') {
        directory.to_string()
    } else {
        format!("{directory}/")
    };
    let body = get_bytes(ctx, &format!("{base}{CHANGES_CSV}")).await?;
    parse_changes_csv(&body, &base)
}

/// `"相對路徑","時間"`，最新在前。
pub fn parse_changes_csv(bytes: &[u8], base: &str) -> Result<Vec<Candidate>, CoreError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(bytes);
    let mut out = Vec::new();
    for row in reader.records() {
        let row = row.map_err(|e| CoreError::Invalid(format!("changes.csv 格式錯誤：{e}")))?;
        let (Some(path), Some(time)) = (row.get(0), row.get(1)) else {
            continue;
        };
        let path = path.trim().trim_start_matches('/');
        if path.is_empty() || path.contains("..") {
            continue;
        }
        if let Some(updated) = parse_time(time) {
            out.push(Candidate {
                url: format!("{base}{path}"),
                updated,
                hash_urls: Vec::new(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tlp_filter() {
        assert!(tlp_allowed(None));
        assert!(tlp_allowed(Some("white")));
        assert!(tlp_allowed(Some("CLEAR")));
        assert!(!tlp_allowed(Some("AMBER")));
        assert!(!tlp_allowed(Some("GREEN")));
    }

    #[test]
    fn times() {
        assert!(parse_time("2026-10-31T03:15:00.000Z").is_some());
        assert!(parse_time("Sat, 31 Oct 2026 03:15:00 GMT").is_some());
        assert!(parse_time("yesterday").is_none());
    }

    #[test]
    fn rss_items_with_entities() {
        let xml = r#"<rss><channel><link>https://x/rss.xml</link>
            <item><title>A &amp; B</title><link>https://x/PAN-SA-1?a=1&amp;b=2</link>
            <pubDate>2026-10-31T03:15:00.000Z</pubDate></item>
            <item><link><![CDATA[https://x/CVE-2026-1]]></link><pubDate>x</pubDate></item>
            </channel></rss>"#;
        let items = parse_rss(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "https://x/PAN-SA-1?a=1&b=2");
        assert_eq!(items[1].0, "https://x/CVE-2026-1");
    }

    #[test]
    fn changes_csv_rows() {
        let csv = b"\"2026/a.json\",\"2026-11-05T08:00:00+00:00\"\n\"../etc\",\"2026-11-05T08:00:00+00:00\"\n";
        let rows = parse_changes_csv(csv, "https://v/csaf/").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "https://v/csaf/2026/a.json");
    }
}
