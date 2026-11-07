//! 時間軸事件 ↔ VEX `analysis.detail` / `lastUpdated`。
//!
//! - 匯入事件的備註為「<檔名>：<detail>」（`import_file` 欄位記錄檔名），匯出時去掉前綴還原 detail，
//!   使匯出 → 匯入 → 再匯出的 detail 不變。
//! - detail 取最後一筆人為判定（使用者或匯入）事件的備註；其後若系統又改變過狀態
//!   （例如列入 KEV 重新開啟），該備註已不描述目前狀態，不輸出。
//! - lastUpdated 為最後一次更新判斷的時間：最後一筆人為判定事件，或其後系統改變狀態的事件
//!   （不含系統的註記類事件）；匯入事件以來源檔中的判定時間（`decided_at`）為準，衝突判斷亦同，
//!   舊判斷不因匯入時間而變新。

use chrono::{DateTime, SecondsFormat, Utc};

use crate::matching::triage::Decider;

/// 匯出與衝突判斷所需的事件欄位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFacts {
    pub at: String,
    pub actor: String,
    pub from: Option<String>,
    pub to: String,
    pub note: String,
    pub import_file: Option<String>,
    /// 匯入事件在來源檔中的判定時間
    pub decided_at: Option<String>,
}

impl EventFacts {
    /// 使用者或匯入的人為判定。
    pub fn is_decision(&self) -> bool {
        self.actor == Decider::USER_ACTOR || self.actor == Decider::IMPORT_ACTOR
    }

    fn changes_status(&self) -> bool {
        self.from.as_deref() != Some(self.to.as_str())
    }

    /// 判定時間：匯入事件為來源檔中的時間（來源沒有時間時為未知，視為最舊），其他為事件時間。
    fn effective_at(&self) -> Option<DateTime<Utc>> {
        if self.actor == Decider::IMPORT_ACTOR {
            return self.decided_at.as_deref().and_then(parse_time);
        }
        parse_time(&self.at)
    }
}

/// 匯入事件的備註。
pub fn import_note(file: &str, detail: Option<&str>) -> String {
    match detail.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("{file}：{d}"),
        None => file.to_string(),
    }
}

/// 事件備註中的使用者文字（匯入事件去掉檔名前綴）。
fn user_text(e: &EventFacts) -> String {
    let Some(file) = e.import_file.as_deref() else {
        return e.note.clone();
    };
    if e.note == file {
        return String::new();
    }
    e.note
        .strip_prefix(&format!("{file}："))
        .unwrap_or(&e.note)
        .to_string()
}

/// 目前狀態的 detail；沒有或已不適用時為 `None`。
pub fn current_detail(events: &[EventFacts]) -> Option<String> {
    let last = events.iter().rposition(EventFacts::is_decision)?;
    let superseded = events[last + 1..]
        .iter()
        .any(|e| !e.is_decision() && e.changes_status());
    if superseded {
        return None;
    }
    let text = user_text(&events[last]);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 本機判斷的時間（匯入衝突判斷用）：依事件順序，自最後一筆人為判定起，取最後一筆判定或
/// 系統狀態變化（例如列入 KEV 重新開啟）的時間。沒有人為判定，或該判定為時間未知的匯入時為 `None`。
pub fn last_decision_at(events: &[EventFacts]) -> Option<DateTime<Utc>> {
    let last = events.iter().rposition(EventFacts::is_decision)?;
    events[last..]
        .iter()
        .rev()
        .find(|e| e.is_decision() || e.changes_status())
        .and_then(EventFacts::effective_at)
}

/// 最後一次更新判斷的時間（依事件順序取最後一筆人為判定或狀態變化）；沒有時為 `fallback`。
pub fn last_updated(events: &[EventFacts], fallback: &str) -> String {
    let latest = events
        .iter()
        .rev()
        .find(|e| e.is_decision() || e.changes_status())
        .and_then(EventFacts::effective_at);
    latest
        .map(format_time)
        .unwrap_or_else(|| normalize_time(fallback))
}

pub fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// RFC 3339 UTC（保留小數秒，衝突判斷比較時不失精度）。
pub fn format_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// 資料庫中的時間字串轉為輸出格式；無法解析時原樣輸出。
pub fn normalize_time(text: &str) -> String {
    parse_time(text).map_or_else(|| text.to_string(), format_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(at: &str, actor: &str, from: Option<&str>, to: &str, note: &str) -> EventFacts {
        EventFacts {
            at: at.into(),
            actor: actor.into(),
            from: from.map(str::to_string),
            to: to.into(),
            note: note.into(),
            import_file: (actor == "import").then(|| "team.json".to_string()),
            decided_at: None,
        }
    }

    const T1: &str = "2026-11-01T00:00:00+00:00";
    const T2: &str = "2026-11-02T00:00:00+00:00";
    const T3: &str = "2026-11-03T00:00:00+00:00";

    #[test]
    fn detail_comes_from_last_user_event() {
        let events = [
            ev(T1, "system", None, "open", ""),
            ev(
                T2,
                "user",
                Some("open"),
                "ignored",
                " 未啟用 GlobalProtect ",
            ),
        ];
        assert_eq!(
            current_detail(&events).as_deref(),
            Some("未啟用 GlobalProtect")
        );
    }

    #[test]
    fn import_prefix_is_stripped() {
        let note = import_note("team.json", Some("未啟用 GP"));
        assert_eq!(note, "team.json：未啟用 GP");
        let events = [ev(T2, "import", Some("open"), "ignored", &note)];
        assert_eq!(current_detail(&events).as_deref(), Some("未啟用 GP"));
        let bare = [ev(T2, "import", Some("open"), "ignored", "team.json")];
        assert_eq!(current_detail(&bare), None);
        assert_eq!(import_note("team.json", Some("  ")), "team.json");
    }

    #[test]
    fn system_status_change_after_decision_drops_detail() {
        let events = [
            ev(T1, "user", Some("open"), "ignored", "接受風險"),
            ev(T2, "system", Some("ignored"), "open", "列入 KEV"),
        ];
        assert_eq!(current_detail(&events), None);
        let same_status = [
            ev(T1, "user", Some("open"), "resolved", "已升級"),
            ev(T2, "system", Some("resolved"), "resolved", "註記"),
        ];
        assert_eq!(current_detail(&same_status).as_deref(), Some("已升級"));
    }

    #[test]
    fn no_decision_means_no_detail() {
        assert_eq!(current_detail(&[ev(T1, "system", None, "open", "x")]), None);
        assert_eq!(current_detail(&[]), None);
    }

    #[test]
    fn last_decision_ignores_system_events() {
        let mut imported = ev(T3, "import", Some("open"), "resolved", "team.json");
        imported.decided_at = Some(T2.into());
        let events = [
            ev(T1, "user", Some("open"), "ignored", ""),
            ev(T3, "system", Some("ignored"), "open", ""),
            imported,
        ];
        assert_eq!(
            last_decision_at(&events),
            parse_time(T2),
            "依事件順序，匯入以來源時間"
        );
        assert_eq!(
            last_updated(&events, T1),
            "2026-11-02T00:00:00Z",
            "依事件順序取最後一筆"
        );
        let reopened = [
            ev(T1, "user", Some("open"), "ignored", ""),
            ev(T3, "system", Some("ignored"), "open", ""),
        ];
        assert_eq!(last_updated(&reopened, T1), "2026-11-03T00:00:00Z");
        assert_eq!(last_updated(&[], T1), "2026-11-01T00:00:00Z");
    }

    #[test]
    fn annotation_events_do_not_advance_last_updated() {
        let events = [
            ev(T1, "user", Some("open"), "ignored", ""),
            ev(T3, "system", Some("ignored"), "ignored", "註記"),
        ];
        assert_eq!(last_updated(&events, T1), "2026-11-01T00:00:00Z");
    }

    #[test]
    fn imported_decision_uses_source_time() {
        let mut imported = ev(T3, "import", Some("open"), "ignored", "team.json");
        imported.decided_at = Some(T1.into());
        let events = [ev(T1, "system", None, "open", ""), imported];
        assert_eq!(last_decision_at(&events), parse_time(T1));
        assert_eq!(last_updated(&events, T1), "2026-11-01T00:00:00Z");
    }

    #[test]
    fn system_reopen_after_decision_counts_as_local_change() {
        let events = [
            ev(T1, "user", Some("open"), "ignored", ""),
            ev(T3, "system", Some("ignored"), "open", "列入 KEV"),
        ];
        assert_eq!(last_decision_at(&events), parse_time(T3));
        let no_decision = [ev(T3, "system", None, "open", "")];
        assert_eq!(last_decision_at(&no_decision), None, "沒有人為判定");
    }

    #[test]
    fn forced_import_replaces_newer_user_time() {
        let mut forced = ev(T3, "import", Some("resolved"), "ignored", "team.json");
        forced.decided_at = Some(T2.into());
        let events = [ev(T3, "user", Some("open"), "resolved", ""), forced];
        assert_eq!(last_decision_at(&events), parse_time(T2));
    }

    #[test]
    fn undated_import_is_unknown_time() {
        let undated = ev(T3, "import", Some("open"), "ignored", "team.json");
        let events = [ev(T1, "system", None, "open", ""), undated];
        assert_eq!(last_decision_at(&events), None);
        assert_eq!(last_updated(&events, T1), "2026-11-01T00:00:00Z");
    }

    #[test]
    fn fractional_seconds_survive_formatting() {
        let t = "2026-11-07T01:02:03.123456789+00:00";
        assert_eq!(parse_time(&normalize_time(t)), parse_time(t));
        assert_eq!(normalize_time("not a time"), "not a time");
    }
}
