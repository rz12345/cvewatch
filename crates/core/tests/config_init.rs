//! 2.4：解析順序、型別錯誤、未知欄位警告、init 冪等。

mod common;

use std::ffi::OsString;

use cvewatch_core::config::{load, ConfigError};
use cvewatch_core::init::{ensure_config_file, run};
use cvewatch_core::paths::{resolve_config_location, ConfigLocation, ConfigSource, DataPaths};
use cvewatch_core::{CoreError, ExitCode};

fn location(path: std::path::PathBuf, source: ConfigSource) -> ConfigLocation {
    ConfigLocation { path, source }
}

#[test]
fn explicit_path_wins_and_is_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.toml");
    let env = dir.path().join("env.toml");
    std::fs::write(&a, "[http]\ntimeout_secs = 11\n").unwrap();
    std::fs::write(&env, "[http]\ntimeout_secs = 22\n").unwrap();

    let loc = resolve_config_location(Some(a.clone()), Some(OsString::from(&env)), None).unwrap();
    let loaded = load(&loc).unwrap();
    assert_eq!(loaded.config.http.timeout_secs, 11);
    assert!(!loaded.used_defaults);
}

#[test]
fn missing_default_file_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loc = location(dir.path().join("cvewatch.toml"), ConfigSource::Default);
    let loaded = load(&loc).unwrap();
    assert!(loaded.used_defaults);
    assert_eq!(loaded.config, Default::default());
}

#[test]
fn missing_explicit_file_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let loc = location(dir.path().join("nope.toml"), ConfigSource::CommandLine);
    let err = CoreError::from(load(&loc).unwrap_err());
    assert_eq!(err.exit_code(), ExitCode::ConfigError);
}

#[test]
fn type_error_maps_to_exit_code_2_with_field_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "[http]\ntimeout_secs = \"abc\"\n").unwrap();
    let err = load(&location(path, ConfigSource::CommandLine)).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
    let msg = err.to_string();
    assert!(
        msg.contains("http.timeout_secs") && msg.contains("第 2 行"),
        "{msg}"
    );
    assert_eq!(CoreError::from(err).exit_code(), ExitCode::ConfigError);
}

#[test]
fn unknown_key_produces_warning_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.toml");
    std::fs::write(&path, "foo = 1\n").unwrap();
    let loaded = load(&location(path, ConfigSource::Environment)).unwrap();
    assert_eq!(loaded.unknown_keys, vec!["foo".to_string()]);
}

#[test]
fn init_creates_everything_and_is_idempotent() {
    let env = common::empty();
    let loc = location(
        env.dir.path().join("cfg").join("cvewatch.toml"),
        ConfigSource::Default,
    );
    let paths = DataPaths::new(env.dir.path().join("fresh"));

    assert!(ensure_config_file(&loc).unwrap());
    let report = run(&loc, &paths, &env.store, true).unwrap();
    assert!(report.config_created && report.db_created && report.key_created);
    assert!(!report.recovery_key_exported);
    assert!(paths.db_path.exists() && paths.log_dir.is_dir() && paths.cvelist_cache_dir.is_dir());

    // 使用者修改設定後重跑 init：設定不被覆蓋，資料庫與金鑰沿用。
    std::fs::write(&loc.path, "# mine\n[http]\ntimeout_secs = 99\n").unwrap();
    assert!(!ensure_config_file(&loc).unwrap());
    let again = run(&loc, &paths, &env.store, false).unwrap();
    assert!(!again.db_created && !again.key_created);
    assert_eq!(
        std::fs::read_to_string(&loc.path).unwrap(),
        "# mine\n[http]\ntimeout_secs = 99\n"
    );
}

#[test]
fn init_warns_for_onedrive_data_dir() {
    let env = common::empty();
    let loc = location(env.dir.path().join("cvewatch.toml"), ConfigSource::Default);
    let paths = DataPaths::new(env.dir.path().join("OneDrive - Org").join("cvewatch"));
    ensure_config_file(&loc).unwrap();
    assert!(
        run(&loc, &paths, &env.store, true)
            .unwrap()
            .cloud_sync_warning
    );
}

#[test]
fn secrets_never_written_to_data_dir() {
    use cvewatch_core::secrets::{SecretKey, SecretStore};
    let env = common::empty();
    let loc = location(env.dir.path().join("cvewatch.toml"), ConfigSource::Default);
    ensure_config_file(&loc).unwrap();
    run(&loc, &env.paths, &env.store, true).unwrap();
    env.store
        .set(SecretKey::NvdApiKey, "NVD-SECRET-VALUE-123")
        .unwrap();
    let hex = env.store.get(SecretKey::DbKey).unwrap().unwrap();

    let mut files = Vec::new();
    collect(env.dir.path(), &mut files);
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        for needle in ["NVD-SECRET-VALUE-123", hex.as_str()] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                "{} 含秘密",
                f.display()
            );
        }
    }
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn relative_ca_file_is_resolved_against_config_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cvewatch.toml");
    std::fs::write(
        &path,
        "[notify]
ca_file = 'certs/ca.pem'
",
    )
    .unwrap();
    let location = cvewatch_core::paths::ConfigLocation {
        path: path.clone(),
        source: cvewatch_core::paths::ConfigSource::CommandLine,
    };
    let loaded = cvewatch_core::config::load(&location).unwrap();
    assert_eq!(
        loaded.config.notify.ca_file(),
        Some(dir.path().join("certs").join("ca.pem").as_path())
    );
}
