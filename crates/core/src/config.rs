//! 設定檔 `cvewatch.toml` 的結構、預設值、載入與驗證。
//!
//! 秘密不在設定檔中（見 [`crate::secrets`]）。未知欄位只產生警告；型別或範圍錯誤
//! 回傳 [`ConfigError::Invalid`]，對應結束碼 2。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use crate::matching::pending::DEFAULT_STALE_DAYS;
use crate::matching::scoring::{
    ScoringConfig, DEFAULT_CVSS_HIGH, DEFAULT_CVSS_MEDIUM, DEFAULT_EPSS_HIGH, DEFAULT_EPSS_MEDIUM,
    DEFAULT_EXPOSURE_BUMP,
};
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
    pub import: ImportConfig,
    /// 風險等級門檻
    pub scoring: ScoringConfig,
    /// 等待區
    pub pending: PendingConfig,
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
            import: ImportConfig::default(),
            scoring: ScoringConfig::default(),
            pending: PendingConfig::default(),
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
    /// 內建 Palo Alto CSAF 來源（`id = "paloalto"`）
    pub paloalto_enabled: bool,
    /// 原廠 CSAF 來源（`[[sources.csaf]]`）
    #[serde(deserialize_with = "csaf_sources")]
    pub csaf: Vec<CsafSourceConfig>,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        SourcesConfig {
            nvd_enabled: true,
            osv_enabled: true,
            paloalto_enabled: true,
            csaf: Vec::new(),
        }
    }
}

/// 內建 Palo Alto 來源的 id。
pub const PALOALTO_SOURCE_ID: &str = "paloalto";
pub const PALOALTO_RSS_URL: &str = "https://security.paloaltonetworks.com/rss.xml";
pub const PALOALTO_CSAF_TEMPLATE: &str = "https://security.paloaltonetworks.com/csaf/{id}";
/// `csaf_url_template` 中代表 RSS item 識別的佔位字。
pub const CSAF_ID_PLACEHOLDER: &str = "{id}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CsafDiscovery {
    /// RSS item link → `csaf_url_template`
    Rss,
    /// `provider-metadata.json`（ROLIE feed 或 `changes.csv`）
    ProviderMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsafSourceConfig {
    pub id: String,
    pub discovery: CsafDiscovery,
    pub url: String,
    /// rss 模式必填，例如 `https://security.paloaltonetworks.com/csaf/{id}`
    pub csaf_url_template: Option<String>,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

impl CsafSourceConfig {
    pub fn paloalto() -> Self {
        CsafSourceConfig {
            id: PALOALTO_SOURCE_ID.to_string(),
            discovery: CsafDiscovery::Rss,
            url: PALOALTO_RSS_URL.to_string(),
            csaf_url_template: Some(PALOALTO_CSAF_TEMPLATE.to_string()),
            enabled: true,
        }
    }

    fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err(format!("CSAF 來源 id `{}` 只能包含小寫英數字與 -", self.id));
        }
        let http = |u: &str| {
            url::Url::parse(u)
                .ok()
                .is_some_and(|u| matches!(u.scheme(), "https" | "http"))
        };
        if !http(&self.url) {
            return Err(format!("CSAF 來源 `{}` 的 url 不是有效網址", self.id));
        }
        if self.discovery == CsafDiscovery::Rss {
            let template = self.csaf_url_template.as_deref().unwrap_or_default();
            if !template.contains(CSAF_ID_PLACEHOLDER) {
                return Err(format!(
                    "CSAF 來源 `{}` 為 rss 模式，csaf_url_template 必須包含 {CSAF_ID_PLACEHOLDER}",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

impl SourcesConfig {
    /// 啟用中的 CSAF 來源：設定檔的來源 + 內建 Palo Alto（使用者以相同 id 定義時以設定檔為準）。
    /// `paloalto_enabled = false` 停用 id 為 `paloalto` 的來源。
    pub fn csaf_sources(&self) -> Vec<CsafSourceConfig> {
        let mut out: Vec<CsafSourceConfig> =
            self.csaf.iter().filter(|c| c.enabled).cloned().collect();
        let user_defined = self.csaf.iter().any(|c| c.id == PALOALTO_SOURCE_ID);
        if !user_defined {
            out.insert(0, CsafSourceConfig::paloalto());
        }
        if !self.paloalto_enabled {
            out.retain(|c| c.id != PALOALTO_SOURCE_ID);
        }
        out
    }
}

impl SourcesConfig {
    /// 所有已定義的 CSAF 來源（含停用者），供 `sync --source csaf:<id>` 明確指定。
    pub fn csaf_sources_all(&self) -> Vec<CsafSourceConfig> {
        let mut out = self.csaf.clone();
        if !out.iter().any(|c| c.id == PALOALTO_SOURCE_ID) {
            out.insert(0, CsafSourceConfig::paloalto());
        }
        out
    }
}

fn csaf_sources<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<CsafSourceConfig>, D::Error> {
    let list = Vec::<CsafSourceConfig>::deserialize(d)?;
    let mut seen = std::collections::HashSet::new();
    for c in &list {
        c.validate().map_err(D::Error::custom)?;
        if !seen.insert(c.id.clone()) {
            return Err(D::Error::custom(format!("CSAF 來源 id `{}` 重複", c.id)));
        }
    }
    Ok(list)
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

/// 等待區設定。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PendingConfig {
    /// 在等待區超過此天數標為逾期
    #[serde(deserialize_with = "bounded::<_, u32, 1, 3650>")]
    pub stale_days: u32,
}

impl Default for PendingConfig {
    fn default() -> Self {
        PendingConfig {
            stale_days: DEFAULT_STALE_DAYS,
        }
    }
}

/// 清冊匯入設定。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    /// 自訂欄名 → 標準欄位，例如 `財產編號 = "asset_tag"`
    #[serde(deserialize_with = "column_targets")]
    pub columns: BTreeMap<String, String>,
}

fn column_targets<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    use crate::assets::columns::StandardField;
    let map = BTreeMap::<String, String>::deserialize(d)?;
    if let Some((header, target)) = map.iter().find(|(_, t)| StandardField::parse(t).is_none()) {
        let allowed: Vec<&str> = StandardField::ALL.iter().map(|f| f.as_str()).collect();
        return Err(D::Error::custom(format!(
            "「{header}」對應的 `{target}` 不是標準欄位；可用：{}",
            allowed.join("、")
        )));
    }
    Ok(map)
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
    let exposure_bump = format!(
        "[{}]",
        DEFAULT_EXPOSURE_BUMP
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
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
paloalto_enabled = true     # 內建 Palo Alto CSAF 公告（RSS）

# 新增提供 CSAF 2.0 公告的原廠（不需更新程式）：
# [[sources.csaf]]
# id = "vendor-x"
# discovery = "provider_metadata"     # 或 rss（需 csaf_url_template，{{id}} 為 RSS item link 最後一段）
# url = "https://vendor-x.example/.well-known/csaf/provider-metadata.json"

[log]
retention_days = {DEFAULT_LOG_RETENTION_DAYS}
level = "info"

[import.columns]
# 現有清冊使用自訂欄名時，在此對應到標準欄位
# （asset_tag、name、category、product、version、exposure、criticality、location、owner、notes）
# "財產編號" = "asset_tag"
# "設備名稱" = "name"

[scoring]
# 風險等級：P1 KEV；P2 EPSS ≥ epss_high 或 CVSS ≥ cvss_high；P3 EPSS ≥ epss_medium 或 CVSS ≥ cvss_medium；
# 其他為 P4。曝露度在 exposure_bump 內的資產上調一級。修改後下次比對（cvewatch match）重算全部等級。
epss_high = {DEFAULT_EPSS_HIGH:.2}
epss_medium = {DEFAULT_EPSS_MEDIUM:.2}
cvss_high = {DEFAULT_CVSS_HIGH:.1}
cvss_medium = {DEFAULT_CVSS_MEDIUM:.1}
exposure_bump = {exposure_bump}

[pending]
stale_days = {DEFAULT_STALE_DAYS}    # 在等待區超過此天數標為逾期

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
    fn notify_placeholder_accepts_anything() {
        let src = "[notify]\nsmtp_host = \"mail\"\n";
        let (cfg, unknown) = parse(src, p()).unwrap();
        assert!(unknown.is_empty());
        assert!(cfg.notify.contains_key("smtp_host"));
    }

    #[test]
    fn scoring_and_pending_are_parsed() {
        let src = "[scoring]\nepss_high = 0.05\nexposure_bump = [\"internet\"]\n\
                   [pending]\nstale_days = 30\n";
        let (cfg, unknown) = parse(src, p()).unwrap();
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(cfg.scoring.epss_high, 0.05);
        assert_eq!(cfg.scoring.epss_medium, DEFAULT_EPSS_MEDIUM);
        assert_eq!(cfg.scoring.exposure_bump.len(), 1);
        assert_eq!(cfg.pending.stale_days, 30);
        let (_, unknown) = parse("[scoring]\nweight = 3\n", p()).unwrap();
        assert_eq!(unknown, vec!["scoring.weight".to_string()]);
    }

    #[test]
    fn invalid_scoring_is_rejected() {
        for src in [
            "[scoring]\nepss_high = 1.5\n",
            "[scoring]\nepss_high = 0.01\nepss_medium = 0.1\n",
            "[scoring]\ncvss_medium = 9.5\n",
            "[scoring]\nexposure_bump = [\"public\"]\n",
            "[pending]\nstale_days = 0\n",
        ] {
            let msg = parse(src, p()).unwrap_err().to_string();
            assert!(
                msg.contains("scoring") || msg.contains("pending"),
                "{src}：{msg}"
            );
        }
    }

    #[test]
    fn csaf_sources_default_to_paloalto() {
        let (cfg, _) = parse("", p()).unwrap();
        let ids: Vec<String> = cfg
            .sources
            .csaf_sources()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["paloalto"]);
        let (off, _) = parse(
            "[sources]
paloalto_enabled = false
",
            p(),
        )
        .unwrap();
        assert!(off.sources.csaf_sources().is_empty());
    }

    #[test]
    fn csaf_provider_is_added_by_configuration() {
        let src = "[[sources.csaf]]
id = \"vendor-x\"
discovery = \"provider_metadata\"
                   url = \"https://vendor-x.example/.well-known/csaf/provider-metadata.json\"
";
        let (cfg, unknown) = parse(src, p()).unwrap();
        assert!(unknown.is_empty());
        let sources = cfg.sources.csaf_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[1].discovery, CsafDiscovery::ProviderMetadata);
    }

    #[test]
    fn csaf_rss_requires_template() {
        let src = "[[sources.csaf]]
id = \"v\"
discovery = \"rss\"
url = \"https://v.example/rss.xml\"
";
        let msg = parse(src, p()).unwrap_err().to_string();
        assert!(
            msg.contains("csaf_url_template") && msg.contains("sources.csaf"),
            "{msg}"
        );
        let dup = "[[sources.csaf]]
id = \"a\"
discovery = \"provider_metadata\"
url = \"https://a.example/p.json\"
                   [[sources.csaf]]
id = \"a\"
discovery = \"provider_metadata\"
url = \"https://a.example/p.json\"
";
        assert!(parse(dup, p()).unwrap_err().to_string().contains("重複"));
        let bad_url = "[[sources.csaf]]
id = \"a\"
discovery = \"provider_metadata\"
url = \"ftp://x\"
";
        assert!(parse(bad_url, p()).is_err());
    }

    #[test]
    fn import_columns_are_validated() {
        let src = "[import.columns]
\"財產編號\" = \"asset_tag\"
";
        let (cfg, _) = parse(src, p()).unwrap();
        assert_eq!(cfg.import.columns["財產編號"], "asset_tag");
        let bad = "[import.columns]
\"序號\" = \"serial\"
";
        let msg = parse(bad, p()).unwrap_err().to_string();
        assert!(
            msg.contains("import.columns") && msg.contains("serial"),
            "{msg}"
        );
    }

    #[test]
    fn line_of_counts_newlines() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 100), 3);
    }
}
