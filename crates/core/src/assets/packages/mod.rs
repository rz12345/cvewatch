//! OS 套件清單匯入：CycloneDX / SPDX JSON（取 purl）、`dpkg-query -W` 與 `rpm -qa` 文字輸出。

pub mod import;
mod plain;
mod sbom;

use std::collections::HashSet;

use packageurl::PackageUrl;

use super::model::ComponentSource;
use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    CycloneDx,
    Spdx,
    Dpkg,
    Rpm,
}

impl PackageFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageFormat::CycloneDx => "cyclonedx",
            PackageFormat::Spdx => "spdx",
            PackageFormat::Dpkg => "dpkg",
            PackageFormat::Rpm => "rpm",
        }
    }

    pub fn source(self) -> ComponentSource {
        match self {
            PackageFormat::CycloneDx | PackageFormat::Spdx => ComponentSource::Sbom,
            PackageFormat::Dpkg => ComponentSource::Dpkg,
            PackageFormat::Rpm => ComponentSource::Rpm,
        }
    }
}

pub const SUPPORTED_FORMATS: &str =
    "CycloneDX JSON、SPDX JSON、dpkg-query -W 輸出（name<TAB>version）、rpm -qa 輸出（name<TAB>epoch:version-release）";

/// 識別套件時忽略的 purl qualifier（會隨版本變動）。
const VERSIONED_QUALIFIERS: [&str; 2] = ["epoch", "upstream"];

/// 發行版，例如 `debian-12`、`ubuntu-22.04`、`rhel-9`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    pub name: String,
    pub release: String,
}

impl Distro {
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        let v = value.trim().to_lowercase();
        let invalid = || {
            CoreError::Invalid(format!(
                "--distro「{value}」格式應為 <發行版>-<版本>，例如 debian-12、ubuntu-22.04、rhel-9"
            ))
        };
        let (name, release) = v.split_once('-').ok_or_else(invalid)?;
        let ok = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_lowercase())
            && !release.is_empty()
            && release
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.');
        if !ok {
            return Err(invalid());
        }
        Ok(Distro {
            name: name.to_string(),
            release: release.to_string(),
        })
    }

    /// purl 的 `distro` qualifier。
    pub fn qualifier(&self) -> String {
        format!("{}-{}", self.name, self.release)
    }
}

/// 一個套件：`purl` 為不含版本的識別，`purl_full` 為完整 purl。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    pub purl: String,
    pub purl_full: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPackages {
    pub format: PackageFormat,
    pub entries: Vec<PackageEntry>,
    pub skipped_without_purl: usize,
    pub skipped_without_version: usize,
    pub duplicates: usize,
}

/// 解析套件清單；`format` 為 `None` 時自動判斷。dpkg / rpm 需要 `distro`。
pub fn parse_packages(
    text: &str,
    format: Option<PackageFormat>,
    distro: Option<&Distro>,
) -> Result<ParsedPackages, CoreError> {
    let text = text.trim_start_matches('\u{feff}');
    let format = match format {
        Some(f) => f,
        None => detect(text)?,
    };
    let raw = match format {
        PackageFormat::CycloneDx => sbom::cyclonedx(text)?,
        PackageFormat::Spdx => sbom::spdx(text)?,
        PackageFormat::Dpkg | PackageFormat::Rpm => {
            let distro = distro.ok_or_else(|| {
                CoreError::Invalid(format!(
                    "{} 清單需要 --distro，例如 --distro debian-12",
                    format.as_str()
                ))
            })?;
            plain::parse(text, format, distro)?
        }
    };
    Ok(collect(format, raw))
}

/// JSON 依 `bomFormat` / `spdxVersion`；文字依每行 tab 分隔的兩欄，rpm 的第二欄帶 `epoch:` 前綴。
pub fn detect(text: &str) -> Result<PackageFormat, CoreError> {
    let unknown = || {
        CoreError::Invalid(format!(
            "無法判斷檔案格式；支援：{SUPPORTED_FORMATS}（可用 --format 指定）"
        ))
    };
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let json: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| unknown())?;
        if json.get("bomFormat").and_then(|v| v.as_str()) == Some("CycloneDX") {
            return Ok(PackageFormat::CycloneDx);
        }
        if json.get("spdxVersion").is_some() {
            return Ok(PackageFormat::Spdx);
        }
        return Err(unknown());
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let columns: Option<Vec<(&str, &str)>> = lines.iter().map(|l| l.split_once('\t')).collect();
    let columns = columns.filter(|c| !c.is_empty()).ok_or_else(unknown)?;
    if columns.iter().any(|(_, rest)| rest.contains('\t')) {
        return Err(unknown());
    }
    let rpm_like = columns.iter().all(|(_, v)| plain::has_rpm_epoch_prefix(v));
    Ok(if rpm_like {
        PackageFormat::Rpm
    } else {
        PackageFormat::Dpkg
    })
}

/// 未經整理的解析結果：每個元素為完整 purl（含版本）或略過原因。
pub(crate) enum RawEntry {
    Purl(String),
    NoPurl,
}

fn collect(format: PackageFormat, raw: Vec<RawEntry>) -> ParsedPackages {
    let mut parsed = ParsedPackages {
        format,
        entries: Vec::new(),
        skipped_without_purl: 0,
        skipped_without_version: 0,
        duplicates: 0,
    };
    let mut seen = HashSet::new();
    for item in raw {
        let RawEntry::Purl(text) = item else {
            parsed.skipped_without_purl += 1;
            continue;
        };
        let Some(entry) = entry_from_purl(&text) else {
            parsed.skipped_without_version += 1;
            continue;
        };
        if seen.insert(entry.purl.clone()) {
            parsed.entries.push(entry);
        } else {
            parsed.duplicates += 1;
        }
    }
    parsed
}

/// purl 的版本；epoch 不為 0 時併入為 `<epoch>:<version>`（與元件 `version_raw` 相同）。
pub(crate) fn purl_version(p: &PackageUrl<'_>) -> Option<String> {
    let version = p.version()?.to_string();
    Some(match p.qualifiers().get("epoch") {
        Some(e) if e.as_ref() != "0" => format!("{e}:{version}"),
        _ => version,
    })
}

/// 識別套件用的 qualifier（去掉隨版本變動者）。
pub(crate) fn identity_qualifiers(p: &PackageUrl<'_>) -> Vec<(String, String)> {
    p.qualifiers()
        .iter()
        .filter(|(k, _)| !VERSIONED_QUALIFIERS.contains(&k.as_ref()))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 完整 purl → 識別（去掉版本與隨版本變動的 qualifier）。沒有版本或無法解析時回傳 `None`。
fn entry_from_purl(text: &str) -> Option<PackageEntry> {
    let full: PackageUrl<'static> = text.parse().ok()?;
    let version = purl_version(&full)?;
    let mut identity = full.clone();
    identity.without_version();
    let kept = identity_qualifiers(&full);
    identity.clear_qualifiers();
    for (k, v) in kept {
        identity.add_qualifier(k, v).ok()?;
    }
    Some(PackageEntry {
        purl: identity.to_string(),
        purl_full: full.to_string(),
        version,
    })
}
