//! 各來源對單一（元件 × CVE）的判定。
//!
//! 每個來源只在「提到這個產品」時產生證據；沒提到時回傳 None（與判定為未知不同）。

pub mod cna;
pub mod csaf;
pub mod nvd;
pub mod osv;
pub mod range;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::catalog::Product;

string_enum!(
    /// 證據來源；`PRECEDENCE` 為產品元件的優先序（OSV 只用於套件元件）。
    #[derive(PartialOrd, Ord)]
    Source {
        Vendor => "vendor",
        Cna => "cna",
        Nvd => "nvd",
        Adp => "adp",
        Osv => "osv",
    }
);

impl Source {
    pub const PRECEDENCE: [Source; 4] = [Source::Vendor, Source::Cna, Source::Nvd, Source::Adp];

    pub fn label(self) -> &'static str {
        match self {
            Source::Vendor => "原廠公告",
            Source::Cna => "CNA",
            Source::Nvd => "NVD",
            Source::Adp => "ADP",
            Source::Osv => "OSV",
        }
    }

    fn rank(self) -> usize {
        Self::PRECEDENCE
            .iter()
            .position(|s| *s == self)
            .unwrap_or(Self::PRECEDENCE.len())
    }
}

string_enum!(
    Verdict {
        Affected => "affected",
        NotAffected => "not_affected",
        Unknown => "unknown",
    }
);

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Affected => "受影響",
            Verdict::NotAffected => "不受影響",
            Verdict::Unknown => "未知",
        }
    }

    /// CVE 5.x `status`（affected / unaffected / unknown）。
    pub fn from_cve5(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "affected" => Verdict::Affected,
            "unaffected" => Verdict::NotAffected,
            _ => Verdict::Unknown,
        }
    }
}

/// 一個來源的判定與可追溯的依據。
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub source: Source,
    pub verdict: Verdict,
    /// 依據（哪個區間、哪份公告）
    pub summary: String,
    /// 來源是否提供了任何版本資訊（等待區判斷「缺少版本資料」）
    pub has_version_data: bool,
    /// 原廠標示調查中（under_investigation）
    pub investigating: bool,
    /// 原廠或 OSV 提供的修正版本
    pub fixed_versions: Vec<String>,
    /// 補充說明，例如「NVD 設定含平台條件，已忽略」
    pub notes: Vec<String>,
}

/// `finding_evidence.detail_json` 的內容（寫入與 `finding show` 讀取共用）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub summary: String,
    #[serde(default)]
    pub has_version_data: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub investigating: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Evidence {
    pub fn new(source: Source, verdict: Verdict, summary: impl Into<String>) -> Self {
        Evidence {
            source,
            verdict,
            summary: summary.into(),
            has_version_data: true,
            investigating: false,
            fixed_versions: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// `finding_evidence.detail_json`。
    pub fn detail_json(&self) -> Result<String, crate::error::CoreError> {
        let detail = Detail {
            summary: self.summary.clone(),
            has_version_data: self.has_version_data,
            investigating: self.investigating,
            fixed_versions: self.fixed_versions.clone(),
            notes: self.notes.clone(),
        };
        serde_json::to_string(&detail)
            .map_err(|e| crate::error::CoreError::Invalid(format!("證據內容序列化失敗：{e}")))
    }
}

/// 依優先序排列（Vendor → CNA → NVD → ADP → OSV）。
pub fn sort_by_precedence(evidence: &mut [Evidence]) {
    evidence.sort_by_key(|e| (e.source.rank(), e.source));
}

/// 目錄產品的識別（判斷各來源的條目是否屬於此產品）。
#[derive(Debug, Clone)]
pub struct ProductIds {
    pub key: String,
    pub scheme: String,
    /// CVE 5.x affected 的 vendor → product（皆正規化）
    pub cna: HashMap<String, HashSet<String>>,
    /// CPE 的 (vendor, product)，小寫
    pub cpe: HashSet<(String, String)>,
}

impl ProductIds {
    pub fn from_product(p: &Product) -> Self {
        ProductIds {
            key: p.key.clone(),
            scheme: p.version_scheme.clone(),
            cna: p.cna.iter().fold(HashMap::new(), |mut acc, n| {
                acc.entry(n.vendor.clone())
                    .or_insert_with(HashSet::new)
                    .insert(n.product.clone());
                acc
            }),
            cpe: p
                .cpe
                .iter()
                .map(|c| (c.vendor.clone(), c.product.clone()))
                .collect(),
        }
    }

    /// 已解析的 CPE (vendor, product) 是否屬於此產品。
    pub fn has_cpe(&self, vendor_product: &(String, String)) -> bool {
        self.cpe.contains(vendor_product)
    }
}

/// 元件版本：每次比對每個元件只解析一次，交給各來源的判定共用。
pub struct ComponentVersion<'a> {
    pub scheme: &'a dyn crate::matching::version::VersionScheme,
    pub raw: &'a str,
    /// 無法以 scheme 解析時為 None（各來源判定為未知）
    pub parsed: Option<crate::matching::version::Version>,
}

impl<'a> ComponentVersion<'a> {
    pub fn new(scheme: &'a dyn crate::matching::version::VersionScheme, raw: &'a str) -> Self {
        ComponentVersion {
            scheme,
            raw,
            parsed: scheme.parse(raw),
        }
    }

    /// 無法解析時的未知證據。
    pub fn unparseable(&self, source: Source) -> Evidence {
        Evidence::new(
            source,
            Verdict::Unknown,
            unparseable_component(self.raw, self.scheme.name()),
        )
    }
}

/// 元件版本無法解析時的說明。
pub fn unparseable_component(version: &str, scheme: &str) -> String {
    format!("元件版本「{version}」無法以 {scheme} 格式解析")
}
