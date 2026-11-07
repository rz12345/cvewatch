//! cvewatch 元件的 bom-ref：`cvewatch:<asset_tag>:<catalog key 或 purl>`。
//!
//! 套件使用不含版本的識別 purl（與資料庫 `component.purl` 相同），元件升級後仍對應到同一元件。
//! 資產編號可能含 `:`，因此解析時：含 `:pkg:` 者以第一個 `:pkg:` 分開，否則以最後一個 `:` 分開
//! （產品目錄 key 不含 `:`）。

pub const PREFIX: &str = "cvewatch:";
const PURL_MARK: &str = ":pkg:";

/// bom-ref 指向的本機元件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRef {
    Product { asset_tag: String, key: String },
    Package { asset_tag: String, purl: String },
}

impl ComponentRef {
    pub fn asset_tag(&self) -> &str {
        match self {
            ComponentRef::Product { asset_tag, .. } | ComponentRef::Package { asset_tag, .. } => {
                asset_tag
            }
        }
    }

    pub fn to_bom_ref(&self) -> String {
        match self {
            ComponentRef::Product { asset_tag, key } => format!("{PREFIX}{asset_tag}:{key}"),
            ComponentRef::Package { asset_tag, purl } => format!("{PREFIX}{asset_tag}:{purl}"),
        }
    }
}

/// 產品目錄 key 的寫法（小寫英數與 `-`，與目錄驗證相同）。
fn is_catalog_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 解析 cvewatch 格式的 bom-ref；其他工具的 bom-ref（UUID 等）回傳 `None`。
pub fn parse(bom_ref: &str) -> Option<ComponentRef> {
    let rest = bom_ref.trim().strip_prefix(PREFIX)?;
    // 資產編號本身可能含 `:pkg:`：取第一個其後為合法 purl 的位置
    let package = rest.match_indices(PURL_MARK).find_map(|(i, _)| {
        let (tag, purl) = (&rest[..i], &rest[i + 1..]);
        let valid = !tag.is_empty() && purl.parse::<packageurl::PackageUrl<'_>>().is_ok();
        valid.then(|| ComponentRef::Package {
            asset_tag: tag.to_string(),
            purl: purl.to_string(),
        })
    });
    if package.is_some() {
        return package;
    }
    let (tag, key) = rest.rsplit_once(':')?;
    (!tag.is_empty() && is_catalog_key(key)).then(|| ComponentRef::Product {
        asset_tag: tag.to_string(),
        key: key.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(tag: &str, key: &str) -> ComponentRef {
        ComponentRef::Product {
            asset_tag: tag.into(),
            key: key.into(),
        }
    }

    fn package(tag: &str, purl: &str) -> ComponentRef {
        ComponentRef::Package {
            asset_tag: tag.into(),
            purl: purl.into(),
        }
    }

    #[test]
    fn product_ref_round_trips() {
        let r = product("FW-01", "paloalto-panos");
        assert_eq!(r.to_bom_ref(), "cvewatch:FW-01:paloalto-panos");
        assert_eq!(parse(&r.to_bom_ref()), Some(r));
    }

    #[test]
    fn package_ref_round_trips() {
        let r = package("WEB-01", "pkg:deb/debian/openssl?distro=debian-12");
        assert_eq!(
            r.to_bom_ref(),
            "cvewatch:WEB-01:pkg:deb/debian/openssl?distro=debian-12"
        );
        assert_eq!(parse(&r.to_bom_ref()), Some(r));
    }

    #[test]
    fn asset_tag_may_contain_colons() {
        for r in [
            product("DC:R1:U12", "dell-idrac"),
            package("DC:R1", "pkg:rpm/redhat/curl"),
        ] {
            assert_eq!(parse(&r.to_bom_ref()), Some(r.clone()));
            assert!(r.asset_tag().starts_with("DC:R1"));
        }
    }

    #[test]
    fn asset_tag_containing_pkg_marker_is_not_mistaken_for_purl() {
        let r = product("LAB:pkg:01", "dell-idrac");
        assert_eq!(parse(&r.to_bom_ref()), Some(r));
        let p = package("LAB:pkg:01", "pkg:deb/debian/openssl");
        assert_eq!(parse(&p.to_bom_ref()), Some(p));
    }

    #[test]
    fn foreign_or_malformed_refs_are_not_parsed() {
        for s in [
            "6b8f6d6e-1c2a-4a7e-9c55-0b7d5d1f6a10",
            "pkg:deb/debian/openssl@3.0.11-1",
            "cvewatch:",
            "cvewatch:FW-01",
            "cvewatch::paloalto-panos",
            "cvewatch:FW-01:",
            "cvewatch::pkg:deb/debian/openssl",
        ] {
            assert_eq!(parse(s), None, "{s}");
        }
    }
}
