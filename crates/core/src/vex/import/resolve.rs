//! VEX 元件 → 本機元件：cvewatch bom-ref → purl → CPE（vendor:product + 版本）→ 名稱 + 版本（產品目錄別名）。
//!
//! - cvewatch 格式的 bom-ref 指向特定資產的元件；找不到時不改用其他方式（避免把某台設備的判斷
//!   套用到其他資產），列為未對應。bom-ref 對應包含已退役資產與已移除元件（命中保留）。
//! - 其他方式只對應使用中的元件，且必須有版本並與本機版本相同；可能對應到多個資產。

use std::collections::HashMap;

use packageurl::PackageUrl;
use rusqlite::Connection;

use super::doc;
use crate::assets::packages;
use crate::catalog::normalize;
use crate::cpe;
use crate::error::CoreError;
use crate::vex::bomref::{self, ComponentRef};

/// 元件對應方式（dry-run 明細中標示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    BomRef,
    Purl,
    Cpe,
    NameVersion,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::BomRef => "bom-ref",
            Method::Purl => "purl",
            Method::Cpe => "CPE",
            Method::NameVersion => "名稱+版本",
        }
    }
}

/// CPE 版本欄位的萬用值。
const CPE_ANY: [&str; 3] = ["*", "-", ""];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    pub id: i64,
    pub asset_tag: String,
    /// 產品 key 或套件 purl（顯示用）
    pub label: String,
    product: Option<(i64, String)>,
    purl: Option<String>,
    /// 載入時解析一次的 purl（比對時不重複解析）
    purl_key: Option<PurlKey>,
    version: String,
    active: bool,
}

/// 一個 VEX 元件的對應結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub method: Option<Method>,
    pub matches: Vec<Local>,
}

pub struct Resolver {
    locals: Vec<Local>,
    /// (cpe vendor, cpe product)（小寫）→ 產品 id
    cpe_products: HashMap<(String, String), Vec<i64>>,
    /// 正規化別名 → 產品 id
    aliases: HashMap<String, i64>,
}

fn same_version(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}

impl Resolver {
    pub fn load(conn: &Connection) -> Result<Self, CoreError> {
        Ok(Resolver {
            locals: load_locals(conn)?,
            cpe_products: load_cpe_products(conn)?,
            aliases: load_aliases(conn)?,
        })
    }

    /// 依序嘗試各對應方式，取第一個有結果者。
    pub fn resolve(&self, c: &doc::Component) -> Resolution {
        if let Some(r) = c.bom_ref.as_deref().and_then(bomref::parse) {
            return Resolution {
                method: Some(Method::BomRef),
                matches: self.by_bom_ref(&r),
            };
        }
        // 依序嘗試，前一種有結果就不再計算後面的
        let found = c
            .purl
            .as_deref()
            .map(|p| (Method::Purl, self.by_purl(p)))
            .filter(|(_, m)| !m.is_empty())
            .or_else(|| {
                c.cpe
                    .as_deref()
                    .map(|p| (Method::Cpe, self.by_cpe(p, c.version.as_deref())))
                    .filter(|(_, m)| !m.is_empty())
            })
            .or_else(|| {
                Some((
                    Method::NameVersion,
                    self.by_name(c.name.as_deref(), c.version.as_deref()),
                ))
                .filter(|(_, m)| !m.is_empty())
            });
        match found {
            Some((method, matches)) => Resolution {
                method: Some(method),
                matches,
            },
            None => Resolution {
                method: None,
                matches: Vec::new(),
            },
        }
    }

    /// 資產編號完全相同者優先；沒有時才不分大小寫，且只在恰好對應一個資產時採用
    /// （資料庫的資產編號區分大小寫，不可把一台設備的判斷套到另一台）。
    fn by_bom_ref(&self, r: &ComponentRef) -> Vec<Local> {
        let exact = self.with_component(r, |tag| tag == r.asset_tag());
        if !exact.is_empty() {
            return exact;
        }
        let loose = self.with_component(r, |tag| tag.eq_ignore_ascii_case(r.asset_tag()));
        let first = loose.first().map(|l| l.asset_tag.clone());
        if loose.iter().all(|l| Some(&l.asset_tag) == first.as_ref()) {
            loose
        } else {
            Vec::new()
        }
    }

    fn with_component(&self, r: &ComponentRef, tag_matches: impl Fn(&str) -> bool) -> Vec<Local> {
        self.locals
            .iter()
            .filter(|l| tag_matches(&l.asset_tag))
            .filter(|l| match r {
                ComponentRef::Product { key, .. } => {
                    l.product.as_ref().is_some_and(|(_, k)| k == key)
                }
                ComponentRef::Package { purl, .. } => l.purl.as_deref() == Some(purl.as_str()),
            })
            .cloned()
            .collect()
    }

    fn active(&self) -> impl Iterator<Item = &Local> {
        self.locals.iter().filter(|l| l.active)
    }

    fn by_purl(&self, text: &str) -> Vec<Local> {
        let Some(vex) = PurlKey::parse(text) else {
            return Vec::new();
        };
        let Some(version) = vex.version.clone() else {
            return Vec::new();
        };
        self.active()
            .filter(|l| {
                l.purl_key.as_ref().is_some_and(|k| k.same_package(&vex))
                    && same_version(&l.version, &version)
            })
            .cloned()
            .collect()
    }

    fn by_cpe(&self, text: &str, fallback_version: Option<&str>) -> Vec<Local> {
        let Some((vendor, product, version)) = cpe_parts(text) else {
            return Vec::new();
        };
        let version = version.or_else(|| fallback_version.map(str::to_string));
        let (Some(version), Some(ids)) = (version, self.cpe_products.get(&(vendor, product)))
        else {
            return Vec::new();
        };
        self.products_with_version(ids, &version)
    }

    fn by_name(&self, name: Option<&str>, version: Option<&str>) -> Vec<Local> {
        let (Some(name), Some(version)) = (name, version) else {
            return Vec::new();
        };
        match self.aliases.get(&normalize(name)) {
            Some(id) => self.products_with_version(&[*id], version),
            None => Vec::new(),
        }
    }

    fn products_with_version(&self, ids: &[i64], version: &str) -> Vec<Local> {
        self.active()
            .filter(|l| l.product.as_ref().is_some_and(|(id, _)| ids.contains(id)))
            .filter(|l| same_version(&l.version, version))
            .cloned()
            .collect()
    }
}

fn load_locals(conn: &Connection) -> Result<Vec<Local>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, a.asset_tag, c.product_id, p.key, c.purl, c.version_raw,
                (a.status = 'active' AND c.removed_at IS NULL)
         FROM component c JOIN asset a ON a.id = c.asset_id
         LEFT JOIN catalog_product p ON p.id = c.product_id
         ORDER BY a.asset_tag, c.id",
    )?;
    let rows = stmt.query_map([], |r| {
        let product: Option<i64> = r.get(2)?;
        let key: Option<String> = r.get(3)?;
        let purl: Option<String> = r.get(4)?;
        Ok(Local {
            id: r.get(0)?,
            asset_tag: r.get(1)?,
            label: key.clone().or_else(|| purl.clone()).unwrap_or_default(),
            product: product.zip(key),
            purl_key: purl.as_deref().and_then(PurlKey::parse),
            purl,
            version: r.get(5)?,
            active: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_cpe_products(conn: &Connection) -> Result<HashMap<(String, String), Vec<i64>>, CoreError> {
    let mut out: HashMap<(String, String), Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT cpe_vendor, cpe_product, product_id FROM catalog_cpe")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
    })?;
    for row in rows {
        let (vendor, product, id) = row?;
        out.entry((vendor.to_lowercase(), product.to_lowercase()))
            .or_default()
            .push(id);
    }
    Ok(out)
}

fn load_aliases(conn: &Connection) -> Result<HashMap<String, i64>, CoreError> {
    let mut stmt = conn.prepare("SELECT alias_norm, product_id FROM catalog_alias")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

/// CPE 2.3 formatted string 或 2.2 URI → (vendor, product, 版本)；vendor／product 小寫並保留跳脫。
/// 版本為萬用值時為 `None`。
fn cpe_parts(text: &str) -> Option<(String, String, Option<String>)> {
    let text = text.trim();
    let (vendor, product, version) = if let Some(rest) = text.strip_prefix("cpe:/") {
        let f: Vec<&str> = rest.split(':').collect();
        (
            f.get(1)?.to_string(),
            f.get(2)?.to_string(),
            f.get(3).map(|v| v.to_string()),
        )
    } else {
        let raw = cpe::raw_fields(text);
        let plain = cpe::fields(text);
        if raw.first().map(String::as_str) != Some("cpe")
            || raw.get(1).map(String::as_str) != Some("2.3")
        {
            return None;
        }
        (
            raw.get(3)?.clone(),
            raw.get(4)?.clone(),
            plain.get(5).cloned(),
        )
    };
    let version = version.filter(|v| !CPE_ANY.contains(&v.as_str()));
    Some((vendor.to_lowercase(), product.to_lowercase(), version))
}

/// purl 中用於比對套件的部分。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PurlKey {
    ty: String,
    namespace: Option<String>,
    name: String,
    version: Option<String>,
    qualifiers: Vec<(String, String)>,
}

impl PurlKey {
    /// 版本與 qualifier 的規則與套件清單匯入相同（[`packages`]）。
    fn parse(text: &str) -> Option<Self> {
        let p: PackageUrl<'_> = text.trim().parse().ok()?;
        Some(PurlKey {
            ty: p.ty().to_lowercase(),
            namespace: p.namespace().map(str::to_string),
            name: p.name().to_string(),
            version: packages::purl_version(&p),
            qualifiers: packages::identity_qualifiers(&p),
        })
    }

    /// 類型、命名空間與名稱相同，且兩邊都有的 qualifier（例如 distro）值相同。
    fn same_package(&self, other: &PurlKey) -> bool {
        let shared_agree = self.qualifiers.iter().all(|(k, v)| {
            other
                .qualifiers
                .iter()
                .find(|(ok, _)| ok == k)
                .is_none_or(|(_, ov)| ov == v)
        });
        self.ty == other.ty
            && self.namespace == other.namespace
            && self.name == other.name
            && shared_agree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpe_23_and_22_are_parsed() {
        assert_eq!(
            cpe_parts("cpe:2.3:o:paloaltonetworks:pan-os:11.1.4\\-h7:*:*:*:*:*:*:*"),
            Some((
                "paloaltonetworks".into(),
                "pan-os".into(),
                Some("11.1.4-h7".into())
            ))
        );
        assert_eq!(
            cpe_parts("cpe:/o:PaloAltoNetworks:PAN-OS:11.1.4-h7"),
            Some((
                "paloaltonetworks".into(),
                "pan-os".into(),
                Some("11.1.4-h7".into())
            ))
        );
        assert_eq!(
            cpe_parts("cpe:2.3:o:paloaltonetworks:pan-os:*:*:*:*:*:*:*:*"),
            Some(("paloaltonetworks".into(), "pan-os".into(), None))
        );
        assert_eq!(cpe_parts("cpe:2.2:x"), None);
        assert_eq!(cpe_parts("not a cpe"), None);
    }

    #[test]
    fn purl_matching_ignores_one_sided_qualifiers() {
        let local = PurlKey::parse("pkg:deb/debian/openssl?distro=debian-12").unwrap();
        let vex =
            PurlKey::parse("pkg:deb/debian/openssl@3.0.11-1?arch=amd64&distro=debian-12").unwrap();
        assert!(local.same_package(&vex));
        assert_eq!(vex.version.as_deref(), Some("3.0.11-1"));
        let other_distro =
            PurlKey::parse("pkg:deb/debian/openssl@3.0.11-1?distro=debian-11").unwrap();
        assert!(!local.same_package(&other_distro));
        let other_name = PurlKey::parse("pkg:deb/debian/libssl3@3.0.11-1").unwrap();
        assert!(!local.same_package(&other_name));
    }

    #[test]
    fn purl_epoch_is_part_of_version() {
        let p = PurlKey::parse("pkg:rpm/redhat/curl@7.76.1-26.el9?epoch=1").unwrap();
        assert_eq!(p.version.as_deref(), Some("1:7.76.1-26.el9"));
        let zero = PurlKey::parse("pkg:rpm/redhat/curl@7.76.1?epoch=0").unwrap();
        assert_eq!(zero.version.as_deref(), Some("7.76.1"));
    }

    #[test]
    fn versions_compare_trimmed_and_case_insensitive() {
        assert!(same_version(" 8.0U2 ", "8.0u2"));
        assert!(!same_version("", ""));
        assert!(!same_version("1.0", "1.0.1"));
    }
}
