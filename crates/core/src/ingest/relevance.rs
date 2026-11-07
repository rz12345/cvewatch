//! 關聯性篩選：判斷一筆 CVE 紀錄是否與產品目錄或資產套件有關，並記錄原因。

use std::collections::HashSet;

use super::cve5::{Affected, CveRecord};
use crate::catalog::{normalize, normalize_vendor, Catalog};

string_enum!(
    /// 保留原因（`cve.relevance_reason`）：
    /// `cna` CNA／ADP affected 的 vendor/product 相符；`cpe` affected 或 NVD 的 CPE 相符；
    /// `purl` affected 的 packageURL 與資產套件相符；`keyword_only` 只有描述關鍵字相符（放入等待區）；
    /// `nvd` 由 NVD CPE 查詢加入；`csaf` 由原廠 CSAF 公告加入。
    Relevance {
        Cna => "cna",
        Cpe => "cpe",
        Purl => "purl",
        KeywordOnly => "keyword_only",
        Nvd => "nvd",
        Csaf => "csaf",
    }
);

impl Relevance {
    pub fn label(self) -> &'static str {
        match self {
            Relevance::Cna => "CNA 產品相符",
            Relevance::Cpe => "CPE 相符",
            Relevance::Purl => "套件相符",
            Relevance::KeywordOnly => "僅關鍵字相符",
            Relevance::Nvd => "NVD CPE 查詢",
            Relevance::Csaf => "原廠公告",
        }
    }

    /// 紀錄內容本身足以證明相關（不需外部來源）。
    fn is_content_match(self) -> bool {
        matches!(self, Relevance::Cna | Relevance::Cpe | Relevance::Purl)
    }

    /// 本機既有原因與本次篩選結果合併：內容相符者為準；否則保留外部來源（NVD/CSAF）的原因。
    pub fn merge(old: Relevance, new: Relevance) -> Relevance {
        if new.is_content_match() {
            return new;
        }
        if matches!(old, Relevance::Nvd | Relevance::Csaf) {
            return old;
        }
        new
    }
}

/// 套件識別：purl 的 type 與 name（含 namespace），小寫。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageKey {
    pub kind: String,
    pub name: String,
}

impl PackageKey {
    /// 從 purl 字串取 `type` 與 `namespace/name`；不是 purl 時為 None。
    pub fn from_purl(purl: &str) -> Option<Self> {
        let parsed: packageurl::PackageUrl<'_> = purl.trim().parse().ok()?;
        let name = match parsed.namespace() {
            Some(ns) if !ns.is_empty() => format!("{ns}/{}", parsed.name()),
            _ => parsed.name().to_string(),
        };
        Some(PackageKey {
            kind: parsed.ty().to_lowercase(),
            name: name.to_lowercase(),
        })
    }
}

/// 篩選所需的索引；由目錄與資產套件建立，sync 期間不變。
#[derive(Debug, Clone, Default)]
pub struct RelevanceIndex {
    cna: HashSet<(String, String)>,
    cpe: HashSet<(String, String)>,
    packages: HashSet<PackageKey>,
    /// 小寫關鍵字
    keywords: Vec<String>,
}

impl RelevanceIndex {
    pub fn new(catalog: &Catalog, packages: impl IntoIterator<Item = PackageKey>) -> Self {
        let mut index = RelevanceIndex {
            packages: packages.into_iter().collect(),
            ..Default::default()
        };
        for p in catalog.products() {
            index
                .cna
                .extend(p.cna.iter().map(|n| (n.vendor.clone(), n.product.clone())));
            index
                .cpe
                .extend(p.cpe.iter().map(|c| (c.vendor.clone(), c.product.clone())));
            index.keywords.extend(normalize_keywords(&p.keywords));
        }
        index.keywords.sort();
        index.keywords.dedup();
        index
    }

    /// 依設計的規則 1–4 判斷；都不符合時為 None（呼叫端再依「已在本機」決定是否保留）。
    pub fn evaluate(&self, record: &CveRecord) -> Option<Relevance> {
        let affected = || {
            record
                .cna
                .affected
                .iter()
                .chain(record.adp.iter().flat_map(|a| a.affected.iter()))
        };
        if affected().any(|a| self.cna_match(a)) {
            return Some(Relevance::Cna);
        }
        if affected()
            .flat_map(|a| a.cpes.iter())
            .any(|c| self.cpe_match(c))
        {
            return Some(Relevance::Cpe);
        }
        if affected().any(|a| self.purl_match(a)) {
            return Some(Relevance::Purl);
        }
        if record
            .description_en
            .as_deref()
            .is_some_and(|d| self.keyword_match(d))
        {
            return Some(Relevance::KeywordOnly);
        }
        None
    }

    fn cna_match(&self, a: &Affected) -> bool {
        let (Some(vendor), Some(product)) = (a.vendor.as_deref(), a.product.as_deref()) else {
            return false;
        };
        let product = normalize(product);
        [normalize_vendor(vendor), normalize(vendor)]
            .into_iter()
            .any(|v| self.cna.contains(&(v, product.clone())))
    }

    /// CPE 2.3 字串（`cpe:2.3:part:vendor:product:...`）的 vendor:product 是否在目錄中。
    pub fn cpe_match(&self, cpe: &str) -> bool {
        cpe_vendor_product(cpe).is_some_and(|vp| self.cpe.contains(&vp))
    }

    fn purl_match(&self, a: &Affected) -> bool {
        a.package_url
            .as_deref()
            .and_then(PackageKey::from_purl)
            .is_some_and(|k| self.packages.contains(&k))
    }

    /// 描述是否含任一關鍵字（不分大小寫，前後須為非英數字）。
    pub fn keyword_match(&self, description: &str) -> bool {
        mentions_any(description, &self.keywords)
    }

    /// 目錄識別與資產套件的雜湊；變更時提示 `sync --rescan`。
    pub fn fingerprint(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.cna.iter().map(|(v, p)| format!("cna:{v}:{p}")));
        parts.extend(self.cpe.iter().map(|(v, p)| format!("cpe:{v}:{p}")));
        parts.extend(
            self.packages
                .iter()
                .map(|k| format!("pkg:{}:{}", k.kind, k.name)),
        );
        parts.extend(self.keywords.iter().map(|k| format!("kw:{k}")));
        parts.sort();
        crate::hash::sha256_hex(&parts.join("\n"))
    }
}

/// 使用中資產的套件識別（`component.purl`，不含版本）。
pub fn package_keys(
    conn: &rusqlite::Connection,
) -> Result<Vec<PackageKey>, crate::error::CoreError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT purl FROM active_component WHERE kind = 'package' AND purl IS NOT NULL",
    )?;
    let purls = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(purls
        .iter()
        .filter_map(|p| PackageKey::from_purl(p))
        .collect())
}

/// `cpe:2.3:o:paloaltonetworks:pan-os:*:...` → (`paloaltonetworks`, `pan-os`)。
/// 依未跳脫的 `:` 切開；跳脫字元保留（與目錄、NVD 查詢使用的形式一致）。
pub fn cpe_vendor_product(cpe: &str) -> Option<(String, String)> {
    let parts = crate::cpe::raw_fields(cpe);
    if parts.first()? != "cpe" || parts.get(1)? != "2.3" {
        return None;
    }
    let vendor = parts.get(3)?.to_lowercase();
    let product = parts.get(4)?.to_lowercase();
    (!vendor.is_empty() && !product.is_empty()).then_some((vendor, product))
}

/// 目錄關鍵字的比對形式：修剪、小寫、去除空字串（關聯性篩選與等待區共用）。
pub fn normalize_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .collect()
}

/// 描述是否含任一關鍵字（不分大小寫、需為完整字詞）。`keywords` 須已經 [`normalize_keywords`]。
pub fn mentions_any(description: &str, keywords: &[String]) -> bool {
    mentions_any_lower(&description.to_lowercase(), keywords)
}

/// 同 [`mentions_any`]，`text` 須已轉小寫（同一描述比對多組關鍵字時只轉一次）。
pub fn mentions_any_lower(text: &str, keywords: &[String]) -> bool {
    keywords.iter().any(|k| contains_word(text, k))
}

/// `text` 是否含完整字詞 `word`（前後須為非英數字）；兩者都應已轉小寫。
fn contains_word(text: &str, word: &str) -> bool {
    let is_word_char = |c: char| c.is_alphanumeric();
    text.match_indices(word).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + word.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_content_then_external_evidence() {
        assert_eq!(
            Relevance::merge(Relevance::Csaf, Relevance::Cna),
            Relevance::Cna
        );
        assert_eq!(
            Relevance::merge(Relevance::Nvd, Relevance::KeywordOnly),
            Relevance::Nvd
        );
        assert_eq!(
            Relevance::merge(Relevance::Cna, Relevance::KeywordOnly),
            Relevance::KeywordOnly
        );
        assert_eq!(
            Relevance::merge(Relevance::Csaf, Relevance::Csaf),
            Relevance::Csaf
        );
    }

    #[test]
    fn relevance_roundtrip() {
        for r in [Relevance::Cna, Relevance::KeywordOnly, Relevance::Csaf] {
            assert_eq!(Relevance::parse(r.as_str()), Some(r));
        }
        assert_eq!(Relevance::parse("other"), None);
    }

    #[test]
    fn cpe_vendor_product_parsing() {
        assert_eq!(
            cpe_vendor_product("cpe:2.3:o:paloaltonetworks:pan-os:*:*:*:*:*:*:*:*"),
            Some(("paloaltonetworks".into(), "pan-os".into()))
        );
        assert_eq!(cpe_vendor_product("cpe:/o:vendor:product"), None);
        assert_eq!(cpe_vendor_product("cpe:2.3:o::x"), None);
    }

    #[test]
    fn word_boundaries() {
        assert!(contains_word("in pan-os before 11.1", "pan-os"));
        assert!(contains_word("pan-os.", "pan-os"));
        assert!(!contains_word("xpan-os", "pan-os"));
        assert!(!contains_word("proxmoxer", "proxmox"));
    }

    #[test]
    fn package_key_from_purl() {
        let k = PackageKey::from_purl("pkg:maven/org.apache.activemq/activemq-broker").unwrap();
        assert_eq!(k.kind, "maven");
        assert_eq!(k.name, "org.apache.activemq/activemq-broker");
        let deb = PackageKey::from_purl("pkg:deb/debian/openssl@3.0.11?distro=debian-12").unwrap();
        assert_eq!(deb.name, "debian/openssl");
        assert!(PackageKey::from_purl("not a purl").is_none());
    }
}
