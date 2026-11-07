//! 通知與排程欄位的驗證規則：設定檔載入（CLI）與 TUI 設定畫面共用。
//!
//! 每個函式回傳使用者看得懂的中文錯誤訊息（不含欄位名稱，由呼叫端決定如何標示）。

use std::str::FromStr;

use lettre::message::Mailbox;
use lettre::Address;

/// 緊急門檻與 CVSS 的上限。
pub const CVSS_MAX: f64 = 10.0;
/// 同步間隔（小時）的允許範圍。
pub const SYNC_HOURS_MIN: u32 = 1;
pub const SYNC_HOURS_MAX: u32 = 24;

/// 連接埠 1–65535。
pub fn port(value: i64) -> Result<u16, String> {
    u16::try_from(value)
        .ok()
        .filter(|p| *p >= 1)
        .ok_or_else(|| format!("連接埠 {value} 超出允許範圍 1–65535"))
}

/// 以文字輸入的連接埠。
pub fn port_text(text: &str) -> Result<u16, String> {
    let n: i64 = text
        .trim()
        .parse()
        .map_err(|_| format!("連接埠「{}」不是數字", text.trim()))?;
    port(n)
}

/// 單一 Email 位址（`user@example.edu.tw`）。
pub fn email_address(text: &str) -> Result<Address, String> {
    Address::from_str(text.trim()).map_err(|_| format!("「{}」不是有效的 Email 位址", text.trim()))
}

/// 寄件者或收件者，可含顯示名稱（`cvewatch <user@example.edu.tw>`）。
pub fn mailbox(text: &str) -> Result<Mailbox, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Email 位址不可為空".into());
    }
    Mailbox::from_str(text).map_err(|_| format!("「{text}」不是有效的 Email 位址"))
}

/// 緊急門檻（CVSS）0–10。
pub fn threshold(value: f64) -> Result<f64, String> {
    if value.is_finite() && (0.0..=CVSS_MAX).contains(&value) {
        Ok(value)
    } else {
        Err(format!("門檻 {value} 超出允許範圍 0–10"))
    }
}

pub fn threshold_text(text: &str) -> Result<f64, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("門檻「{}」不是數字", text.trim()))?;
    threshold(value)
}

/// 每日時間 `HH:MM`（24 小時制），回傳 (時, 分)。
pub fn hhmm(text: &str) -> Result<(u32, u32), String> {
    let err = || format!("時間「{}」格式應為 HH:MM（00:00–23:59）", text.trim());
    let (h, m) = text.trim().split_once(':').ok_or_else(err)?;
    if h.len() != 2 || m.len() != 2 {
        return Err(err());
    }
    let h: u32 = h.parse().map_err(|_| err())?;
    let m: u32 = m.parse().map_err(|_| err())?;
    if h > 23 || m > 59 {
        return Err(err());
    }
    Ok((h, m))
}

/// 同步間隔（小時）1–24。
pub fn sync_hours(value: i64) -> Result<u32, String> {
    u32::try_from(value)
        .ok()
        .filter(|h| (SYNC_HOURS_MIN..=SYNC_HOURS_MAX).contains(h))
        .ok_or_else(|| {
            format!("同步間隔 {value} 小時超出允許範圍 {SYNC_HOURS_MIN}–{SYNC_HOURS_MAX}")
        })
}

pub fn sync_hours_text(text: &str) -> Result<u32, String> {
    let n: i64 = text
        .trim()
        .parse()
        .map_err(|_| format!("同步間隔「{}」不是整數", text.trim()))?;
    sync_hours(n)
}

/// Redmine 網址：必須為 https；只有 `allow_insecure` 時接受 http。不得含帳密、查詢字串。
pub fn redmine_url(text: &str, allow_insecure: bool) -> Result<url::Url, String> {
    let text = text.trim();
    let url = url::Url::parse(text).map_err(|_| format!("「{text}」不是有效的網址"))?;
    match url.scheme() {
        "https" => {}
        "http" if allow_insecure => {}
        "http" => return Err(
            "Redmine 網址必須使用 https（明文 http 需在設定檔手動啟用 allow_insecure_transport）"
                .into(),
        ),
        other => return Err(format!("Redmine 網址必須使用 https，不支援 {other}")),
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(format!("「{text}」缺少主機名稱"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Redmine 網址不可包含帳號密碼；API key 請存入 Credential Manager".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Redmine 網址不可包含查詢字串".into());
    }
    Ok(url)
}

/// 非負整數 ID（0 表示未設定）。
pub fn id_text(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("「{}」不是有效的 ID（非負整數）", text.trim()))
}

/// Redmine 專案：數字 ID 或識別碼（小寫英數字、-、_）。
pub fn project_ref(text: &str) -> Result<String, String> {
    let t = text.trim();
    let ok = !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if ok {
        Ok(t.to_string())
    } else {
        Err(format!(
            "專案「{t}」應為數字 ID 或 Redmine 識別碼（小寫英數字、-、_）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports() {
        assert_eq!(port(587), Ok(587));
        assert!(port(0).is_err());
        assert!(port(70000).unwrap_err().contains("1–65535"));
        assert!(port_text("abc").is_err());
        assert_eq!(port_text(" 465 "), Ok(465));
    }

    #[test]
    fn emails() {
        assert!(email_address("rz@example.edu.tw").is_ok());
        assert!(email_address("not-an-email").is_err());
        assert!(email_address("a b@example.com").is_err());
        assert!(mailbox("cvewatch <rz@example.edu.tw>").is_ok());
        assert!(mailbox("rz@example.edu.tw").is_ok());
        assert!(mailbox("").is_err());
        assert!(mailbox("cvewatch <oops>").is_err());
    }

    #[test]
    fn thresholds() {
        assert_eq!(threshold(9.0), Ok(9.0));
        assert!(threshold(10.5).is_err());
        assert!(threshold(-1.0).is_err());
        assert!(threshold(f64::NAN).is_err());
        assert!(threshold_text("x").is_err());
    }

    #[test]
    fn times() {
        assert_eq!(hhmm("08:30"), Ok((8, 30)));
        assert_eq!(hhmm("23:59"), Ok((23, 59)));
        for bad in ["8:30", "24:00", "12:60", "0830", "ab:cd", ""] {
            assert!(hhmm(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn hours() {
        assert_eq!(sync_hours(4), Ok(4));
        assert!(sync_hours(0).is_err());
        assert!(sync_hours(25).is_err());
        assert!(sync_hours_text("2.5").is_err());
    }

    #[test]
    fn redmine_urls() {
        assert!(redmine_url("https://redmine.example.edu.tw", false).is_ok());
        let err = redmine_url("http://redmine.local", false).unwrap_err();
        assert!(err.contains("https"), "{err}");
        assert!(redmine_url("http://redmine.local", true).is_ok());
        assert!(redmine_url("ftp://x", true).is_err());
        assert!(redmine_url("https://u:p@x.example", false).is_err());
        assert!(redmine_url("https://x.example/?key=1", false).is_err());
        assert!(redmine_url("not a url", false).is_err());
    }

    #[test]
    fn ids_and_projects() {
        assert_eq!(id_text("12"), Ok(12));
        assert!(id_text("-1").is_err());
        assert_eq!(project_ref("infra-sec"), Ok("infra-sec".into()));
        assert_eq!(project_ref("12"), Ok("12".into()));
        assert!(project_ref("Infra Sec").is_err());
    }
}
