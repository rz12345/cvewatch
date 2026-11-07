//! 06 任務 5.4：Task Scheduler XML 以 golden file 驗證（spec sync-scheduling）。
//!
//! 更新 golden：`CVEWATCH_UPDATE_GOLDEN=1 cargo test -p cvewatch-core --test schedule_xml`

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use cvewatch_core::schedule::xml::{encode_utf16, plan, task_xml, InstallContext, Trigger};
use cvewatch_core::schedule::ScheduleConfig;

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/schedule")
        .join(name);
    if std::env::var_os("CVEWATCH_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(actual, expected, "golden {name} 不符");
}

fn ctx(config: Option<&str>) -> InstallContext {
    InstallContext {
        user_id: r"LAB\rz12345".into(),
        wrapper: PathBuf::from(r"C:\Tools\cvewatch\cvewatchw.exe"),
        config: config.map(PathBuf::from),
        today: NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
    }
}

#[test]
fn default_schedule_matches_golden_files() {
    let specs = plan(&ScheduleConfig::default(), &ctx(None));
    assert_eq!(specs.len(), 2);
    golden("sync.xml", &task_xml(&specs[0]));
    golden("digest.xml", &task_xml(&specs[1]));
}

#[test]
fn required_settings_are_present() {
    let specs = plan(&ScheduleConfig::default(), &ctx(None));
    for spec in &specs {
        let xml = task_xml(spec);
        for needle in [
            "<LogonType>InteractiveToken</LogonType>",
            "<StartWhenAvailable>true</StartWhenAvailable>",
            "<RunOnlyIfNetworkAvailable>true</RunOnlyIfNetworkAvailable>",
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<ExecutionTimeLimit>PT1H</ExecutionTimeLimit>",
            r"<Command>C:\Tools\cvewatch\cvewatchw.exe</Command>",
            r"<UserId>LAB\rz12345</UserId>",
        ] {
            assert!(xml.contains(needle), "{} 缺少 {needle}", spec.name);
        }
    }
    assert!(task_xml(&specs[0]).contains("<Arguments>sync</Arguments>"));
    assert!(task_xml(&specs[1]).contains("<Arguments>notify digest</Arguments>"));
}

#[test]
fn reinstall_with_new_interval_and_time_updates_triggers() {
    let cfg = ScheduleConfig {
        sync_every_hours: 2,
        digest_at: "07:45".into(),
    };
    let specs = plan(&cfg, &ctx(None));
    assert!(matches!(
        specs[0].trigger,
        Trigger::EveryHours { hours: 2, .. }
    ));
    let sync = task_xml(&specs[0]);
    assert!(sync.contains("<Interval>PT2H</Interval>"));
    let digest = task_xml(&specs[1]);
    assert!(digest.contains("<StartBoundary>2026-11-07T07:45:00</StartBoundary>"));
    assert!(digest.contains("<DaysInterval>1</DaysInterval>"));
}

#[test]
fn explicit_config_is_passed_and_quoted() {
    let specs = plan(
        &ScheduleConfig::default(),
        &ctx(Some(r"C:\Users\rz\My Docs\cvewatch.toml")),
    );
    assert_eq!(
        specs[0].arguments,
        r#"--config "C:\Users\rz\My Docs\cvewatch.toml" sync"#
    );
    assert!(task_xml(&specs[1])
        .contains(r#"<Arguments>--config &quot;C:\Users\rz\My Docs\cvewatch.toml&quot; notify digest</Arguments>"#));
}

#[test]
fn file_is_utf16_with_bom() {
    let specs = plan(&ScheduleConfig::default(), &ctx(None));
    let bytes = encode_utf16(&task_xml(&specs[0]));
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    let units: Vec<u16> = bytes[2..]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let text = String::from_utf16(&units).unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n"));
}
