//! 設定檔 `cvewatch.toml` 的結構、預設值、載入與驗證。
//!
//! 秘密不在設定檔中（見 [`crate::secrets`]）。未知欄位只產生警告；型別或範圍錯誤
//! 回傳 [`ConfigError::Invalid`]，對應結束碼 2。

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use crate::paths::ConfigLocation;

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_LOG_RETENTION_DAYS: u32 = 30;
pub const DEFAULT_USER_AGENT: &str =
    concat!("cvewatch/", env!("CARGO_PKG_VERSION"), " (+internal tool)");

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("指定的設定檔不存在：{0}")]
    NotFound(PathBuf),

    #[error("無法讀取設定檔 {path}：{source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("設定檔錯誤：{path}{location}：{message}")]
    Invalid {
        path: PathBuf,
        /// 例如「第 3 行，欄位 `http.timeout_secs`」
        location: String,
        message: String,
    },

    #[error("無法取得使用者目錄（%APPDATA% / %LOCALAPPDATA%）")]
    NoHomeDir,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub data_dir: Option<PathBuf>,
    pub http: HttpConfig,
    pub sources: SourcesConfig,
    pub log: LogConfig,
    /// 由 change 04 定義；目前接受任意內容。
    pub scoring: toml::Table,
    /// 由 change 06 定義；目前接受任意內容。
    pub notify: toml::Table,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            data_dir: None,
            http: HttpConfig::default(),
            sources: SourcesConfig::default(),
            log: LogConfig::default(),
            scoring: toml::Table::new(),
            notify: toml::Table::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    #[serde(deserialize_with = "bounded::<_, u64, 1, 600>")]
    pub timeout_secs: u64,
    #[serde(deserialize_with = "bounded::<_, u32, 0, 10>")]
    pub max_retries: u32,
    #[serde(deserialize_with = "non_empty_string")]
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_retries: DEFAULT_MAX_RETRIES,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SourcesConfig {
    pub nvd_enabled: bool,
    pub osv_enabled: bool,
    pub paloalto_enabled: bool,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        SourcesConfig {
            nvd_enabled: true,
            osv_enabled: true,
            paloalto_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    #[serde(deserialize_with = "bounded::<_, u32, 1, 3650>")]
    pub retention_days: u32,
    pub level: LogLevel,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            retention_days: DEFAULT_LOG_RETENTION_DAYS,
            level: LogLevel::default(),
        }
    }
}

/// 載入結果：設定、未知欄位警告、是否使用內建預設值。
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub unknown_keys: Vec<String>,
    pub used_defaults: bool,
    pub location: ConfigLocation,
}

/// 依位置載入設定。預設位置不存在時使用內建預設值；明確指定的路徑不存在則為錯誤。
pub fn load(location: &ConfigLocation) -> Result<LoadedConfig, ConfigError> {
    if !location.path.exists() {
        if location.is_explicit() {
            return Err(ConfigError::NotFound(location.path.clone()));
        }
        return Ok(LoadedConfig {
            config: Config::default(),
            unknown_keys: Vec::new(),
            used_defaults: true,
            location: location.clone(),
        });
    }
    let src = std::fs::read_to_string(&location.path).map_err(|source| ConfigError::Read {
        path: location.path.clone(),
        source,
    })?;
    let (config, unknown_keys) = parse(&src, &location.path)?;
    Ok(LoadedConfig {
        config,
        unknown_keys,
        used_defaults: false,
        location: location.clone(),
    })
}

/// 解析設定內容，回傳設定與未知欄位路徑（例如 `foo`、`http.bar`）。
pub fn parse(src: &str, path: &Path) -> Result<(Config, Vec<String>), ConfigError> {
    let de = toml::Deserializer::parse(src).map_err(|e| invalid(path, src, None, &e))?;
    let mut unknown = Vec::new();
    let result = {
        let mut on_unknown = |p: serde_ignored::Path<'_>| unknown.push(p.to_string());
        let ignored = serde_ignored::Deserializer::new(de, &mut on_unknown);
        serde_path_to_error::deserialize::<_, Config>(ignored)
    };
    let config = result.map_err(|e| {
        let field = e.path().to_string();
        let field = (field != ".").then_some(field);
        invalid(path, src, field, &e.into_inner())
    })?;
    Ok((config, unknown))
}

fn invalid(path: &Path, src: &str, field: Option<String>, err: &toml::de::Error) -> ConfigError {
    let line = err.span().map(|span| line_of(src, span.start));
    let mut parts = Vec::new();
    if let Some(line) = line {
        parts.push(format!("第 {line} 行"));
    }
    if let Some(field) = field {
        parts.push(format!("欄位 `{field}`"));
    }
    let location = if parts.is_empty() {
        String::new()
    } else {
        format!("（{}）", parts.join("，"))
    };
    ConfigError::Invalid {
        path: path.to_path_buf(),
        location,
        message: err.message().trim().to_string(),
    }
}

fn line_of(src: &str, offset: usize) -> usize {
    let end = offset.min(src.len());
    src.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

fn bounded<'de, D, T, const MIN: u64, const MAX: u64>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Into<u64> + Copy,
{
    let value = T::deserialize(d)?;
    let n: u64 = value.into();
    if !(MIN..=MAX).contains(&n) {
        return Err(D::Error::custom(format!(
            "數值 {n} 超出允許範圍 {MIN}–{MAX}"
        )));
    }
    Ok(value)
}

fn non_empty_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s.trim().is_empty() {
        return Err(D::Error::custom("不可為空字串"));
    }
    Ok(s)
}

/// `cvewatch init` 寫出的範例設定檔。
pub fn example_config() -> String {
    format!(
        r#"# cvewatch 設定檔
# 秘密（NVD API key、SMTP 密碼、Redmine API key）不放在這裡，
# 請使用 `cvewatch config set-secret <key>` 存入 Windows Credential Manager。

# 資料目錄（資料庫、日誌、快取）。預設為 %LOCALAPPDATA%\cvewatch。
# 不建議放在 OneDrive 等雲端同步資料夾內（SQLite WAL 檔不應被同步）。
# data_dir = 'D:\cvewatch-data'

[http]
timeout_secs = {DEFAULT_TIMEOUT_SECS}
max_retries = {DEFAULT_MAX_RETRIES}
user_agent = "{DEFAULT_USER_AGENT}"

[sources]
nvd_enabled = true
osv_enabled = true
paloalto_enabled = true

[log]
retention_days = {DEFAULT_LOG_RETENTION_DAYS}
level = "info"

[scoring]   # 風險排序參數（後續版本使用）

[notify]    # Email / Redmine 通知（後續版本使用）
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> &'static Path {
        Path::new("test.toml")
    }

    #[test]
    fn example_config_parses_to_defaults_without_warnings() {
        let (cfg, unknown) = parse(&example_config(), p()).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn empty_file_gives_defaults() {
        let (cfg, unknown) = parse("", p()).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(unknown.is_empty());
    }

    #[test]
    fn type_error_reports_field_and_line() {
        let src = "data_dir = 'D:/x'\n[http]\ntimeout_secs = \"abc\"\n";
        let err = parse(src, p()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("http.timeout_secs"), "{msg}");
        assert!(msg.contains("第 3 行"), "{msg}");
    }

    #[test]
    fn range_error_reports_field_and_line() {
        let src = "[http]\nmax_retries = 99\n";
        let msg = parse(src, p()).unwrap_err().to_string();
        assert!(msg.contains("http.max_retries"), "{msg}");
        assert!(msg.contains("第 2 行"), "{msg}");
        assert!(msg.contains("0–10"), "{msg}");
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let msg = parse("[http]\ntimeout_secs = 0\n", p())
            .unwrap_err()
            .to_string();
        assert!(msg.contains("http.timeout_secs"), "{msg}");
    }

    #[test]
    fn empty_user_agent_is_rejected() {
        let msg = parse("[http]\nuser_agent = \" \"\n", p())
            .unwrap_err()
            .to_string();
        assert!(msg.contains("http.user_agent"), "{msg}");
    }

    #[test]
    fn invalid_log_level_is_rejected() {
        let msg = parse("[log]\nlevel = \"loud\"\n", p())
            .unwrap_err()
            .to_string();
        assert!(msg.contains("log.level"), "{msg}");
    }

    #[test]
    fn syntax_error_reports_line() {
        let msg = parse("[http\n", p()).unwrap_err().to_string();
        assert!(msg.contains("第 1 行"), "{msg}");
    }

    #[test]
    fn unknown_keys_are_collected() {
        let src = "foo = 1\n[http]\nbar = true\ntimeout_secs = 10\n";
        let (cfg, unknown) = parse(src, p()).unwrap();
        assert_eq!(cfg.http.timeout_secs, 10);
        assert_eq!(unknown, vec!["foo".to_string(), "http.bar".to_string()]);
    }

    #[test]
    fn placeholder_sections_accept_anything() {
        let src = "[scoring]\nweight = 3\n[notify]\nsmtp_host = \"mail\"\n";
        let (cfg, unknown) = parse(src, p()).unwrap();
        assert!(unknown.is_empty());
        assert_eq!(
            cfg.scoring.get("weight").and_then(|v| v.as_integer()),
            Some(3)
        );
    }

    #[test]
    fn line_of_counts_newlines() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 100), 3);
    }
}
