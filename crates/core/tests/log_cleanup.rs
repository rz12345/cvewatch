//! 5.3：日誌清理。

use chrono::{Duration, NaiveDate};
use cvewatch_core::logging::cleanup_old_logs;

fn name(date: NaiveDate) -> String {
    format!("cvewatch.{}.log", date.format("%Y-%m-%d"))
}

#[test]
fn deletes_only_logs_older_than_retention() {
    let dir = tempfile::tempdir().unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
    let keep_30 = name(today - Duration::days(30));
    let drop_31 = name(today - Duration::days(31));
    let keep_today = name(today);
    for f in [&keep_30, &drop_31, &keep_today, &"notes.txt".to_string()] {
        std::fs::write(dir.path().join(f), b"x").unwrap();
    }

    let removed = cleanup_old_logs(dir.path(), 30, today).unwrap();

    assert_eq!(removed, vec![dir.path().join(&drop_31)]);
    assert!(!dir.path().join(&drop_31).exists());
    for f in [&keep_30, &keep_today, &"notes.txt".to_string()] {
        assert!(dir.path().join(f).exists(), "{f} 應保留");
    }
}

#[test]
fn missing_log_dir_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
    assert!(cleanup_old_logs(&dir.path().join("none"), 30, today)
        .unwrap()
        .is_empty());
}
