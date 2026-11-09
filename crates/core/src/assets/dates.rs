//! 生命週期日期的解析與格式（22 design D8）。
//!
//! 清冊與畫面接受 `YYYY-MM-DD`、`YYYY/MM/DD`（月日可為一位數）；資料庫、匯出與目錄一律為
//! `YYYY-MM-DD` 文字。

use chrono::NaiveDate;

/// 日期欄位錯誤的說明（接在欄名與原文之後）。
pub const INVALID_DATE_HINT: &str = "不是有效日期（請用 YYYY-MM-DD）";

/// 儲存與顯示格式。
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// 解析清冊或畫面輸入的日期：空白為 `Ok(None)`，格式不符或日期不存在為 `Err(())`。
#[allow(clippy::result_unit_err)]
pub fn parse_date(text: &str) -> Result<Option<NaiveDate>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let separator = if text.contains('-') { '-' } else { '/' };
    let parts: Vec<&str> = text.split(separator).collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(());
    };
    let digits = |s: &str, min: usize, max: usize| {
        (min..=max).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())
    };
    if !digits(year, 4, 4) || !digits(month, 1, 2) || !digits(day, 1, 2) {
        return Err(());
    }
    let (Ok(y), Ok(m), Ok(d)) = (year.parse(), month.parse(), day.parse()) else {
        return Err(());
    };
    NaiveDate::from_ymd_opt(y, m, d).map(Some).ok_or(())
}

/// 嚴格的 `YYYY-MM-DD`（目錄與資料庫的值）。
pub fn parse_iso_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    let shaped = text.len() == 10
        && text.char_indices().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        });
    if !shaped {
        return None;
    }
    NaiveDate::parse_from_str(text, DATE_FORMAT).ok()
}

/// 儲存格式的文字。
pub fn format_date(date: NaiveDate) -> String {
    date.format(DATE_FORMAT).to_string()
}

/// 選填日期的文字：未填為空字串（匯出、表單）。
pub fn format_opt(date: Option<NaiveDate>) -> String {
    date.map(format_date).unwrap_or_default()
}

/// UTC 時間在指定位移（None 為本機時區）下的日曆日：EoL 與到期日以使用者的日曆日判斷，
/// 摘要（[`crate::notify::content::Zone`]）與畫面（frontend 的顯示時區）共用。
pub fn calendar_date(
    t: chrono::DateTime<chrono::Utc>,
    offset: Option<chrono::FixedOffset>,
) -> NaiveDate {
    match offset {
        Some(o) => t.with_timezone(&o).date_naive(),
        None => t.with_timezone(&chrono::Local).date_naive(),
    }
}

/// 欄位錯誤訊息：「`support_end` 的「2027-02-30」不是有效日期（請用 YYYY-MM-DD）」。
pub fn invalid_date_message(field: &str, raw: &str) -> String {
    format!("{field} 的「{}」{INVALID_DATE_HINT}", raw.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, d)
    }

    #[test]
    fn accepts_dash_and_slash_with_short_month_and_day() {
        assert_eq!(parse_date("2027/12/31"), Ok(ymd(2027, 12, 31)));
        assert_eq!(parse_date("2027-1-5"), Ok(ymd(2027, 1, 5)));
        assert_eq!(parse_date(" 2027-06-30 "), Ok(ymd(2027, 6, 30)));
    }

    #[test]
    fn rejects_nonexistent_and_other_formats() {
        assert_eq!(parse_date("2027-02-30"), Err(()));
        assert_eq!(parse_date("27/1/1"), Err(()));
        assert_eq!(parse_date("2027-13-01"), Err(()));
        assert_eq!(parse_date("116/01/01"), Err(()));
        assert_eq!(parse_date("2027/01-01"), Err(()));
        assert_eq!(parse_date("2027-01-01 00:00:00"), Err(()));
    }

    #[test]
    fn blank_is_none() {
        assert_eq!(parse_date(""), Ok(None));
        assert_eq!(parse_date("   "), Ok(None));
    }

    #[test]
    fn iso_is_strict() {
        assert_eq!(parse_iso_date("2026-10-31"), ymd(2026, 10, 31));
        assert_eq!(parse_iso_date("2026-1-31"), None);
        assert_eq!(parse_iso_date("2026/10/31"), None);
        assert_eq!(parse_iso_date("2026-02-30"), None);
        assert_eq!(format_date(ymd(2027, 1, 5).unwrap()), "2027-01-05");
    }

    #[test]
    fn message_names_field_and_raw_text() {
        assert_eq!(
            invalid_date_message("support_end", "2027-02-30"),
            "support_end 的「2027-02-30」不是有效日期（請用 YYYY-MM-DD）"
        );
    }
}
