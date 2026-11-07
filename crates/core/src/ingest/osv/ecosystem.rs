//! purl（含 `distro` qualifier）→ OSV 的 ecosystem 與套件名稱。

use packageurl::PackageUrl;

/// OSV 查詢用的套件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsvPackage {
    pub ecosystem: String,
    pub name: String,
}

/// Debian 代號 → 版本號。
const DEBIAN_CODENAMES: [(&str, &str); 6] = [
    ("buster", "10"),
    ("bullseye", "11"),
    ("bookworm", "12"),
    ("trixie", "13"),
    ("forky", "14"),
    ("sid", "unstable"),
];

/// 語言套件 purl type → OSV ecosystem。
const LANGUAGE: [(&str, &str); 8] = [
    ("npm", "npm"),
    ("pypi", "PyPI"),
    ("maven", "Maven"),
    ("golang", "Go"),
    ("cargo", "crates.io"),
    ("nuget", "NuGet"),
    ("gem", "RubyGems"),
    ("composer", "Packagist"),
];

/// `debian-12` → (`debian`, `12`)
fn split_distro(distro: &str) -> Option<(String, String)> {
    let (name, release) = distro
        .trim()
        .to_lowercase()
        .split_once('-')
        .map(|(a, b)| (a.to_string(), b.to_string()))?;
    (!name.is_empty() && !release.is_empty()).then_some((name, release))
}

fn distro_ecosystem(name: &str, release: &str) -> Option<String> {
    match name {
        "debian" => {
            let release = DEBIAN_CODENAMES
                .iter()
                .find(|(code, _)| *code == release)
                .map_or(release, |(_, n)| *n);
            Some(format!("Debian:{release}"))
        }
        "ubuntu" => {
            // OSV 以 `Ubuntu:22.04:LTS` 表示 LTS（偶數年 .04）
            let lts = release
                .split_once('.')
                .and_then(|(y, m)| Some(y.parse::<u32>().ok()? % 2 == 0 && m == "04"))
                .unwrap_or(false);
            Some(if lts {
                format!("Ubuntu:{release}:LTS")
            } else {
                format!("Ubuntu:{release}")
            })
        }
        "alpine" => Some(format!("Alpine:v{}", release.trim_start_matches('v'))),
        "rocky" => Some(format!("Rocky Linux:{release}")),
        "almalinux" | "alma" => Some(format!("AlmaLinux:{release}")),
        _ => None,
    }
}

/// 無法對應（例如 RHEL 需要 repository 資訊）時為 None。
pub fn from_purl(purl: &str) -> Option<OsvPackage> {
    let p: PackageUrl<'_> = purl.trim().parse().ok()?;
    let ty = p.ty().to_lowercase();
    let namespace = p.namespace().filter(|n| !n.is_empty());
    if let Some((_, eco)) = LANGUAGE.iter().find(|(t, _)| *t == ty) {
        let name = match (ty.as_str(), namespace) {
            ("maven", Some(ns)) => format!("{ns}:{}", p.name()),
            (_, Some(ns)) => format!("{ns}/{}", p.name()),
            (_, None) => p.name().to_string(),
        };
        return Some(OsvPackage {
            ecosystem: (*eco).to_string(),
            name,
        });
    }
    if !matches!(ty.as_str(), "deb" | "rpm" | "apk") {
        return None;
    }
    let distro = p.qualifiers().get("distro").map(|d| d.to_string());
    let (name, release) = distro.as_deref().and_then(split_distro)?;
    Some(OsvPackage {
        ecosystem: distro_ecosystem(&name, &release)?,
        name: p.name().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eco(purl: &str) -> Option<(String, String)> {
        from_purl(purl).map(|p| (p.ecosystem, p.name))
    }

    #[test]
    fn distro_packages() {
        assert_eq!(
            eco("pkg:deb/debian/openssl?distro=debian-12"),
            Some(("Debian:12".into(), "openssl".into()))
        );
        assert_eq!(
            eco("pkg:deb/debian/openssl?distro=debian-bookworm")
                .unwrap()
                .0,
            "Debian:12"
        );
        assert_eq!(
            eco("pkg:deb/ubuntu/curl?distro=ubuntu-22.04").unwrap().0,
            "Ubuntu:22.04:LTS"
        );
        assert_eq!(
            eco("pkg:deb/ubuntu/curl?distro=ubuntu-23.10").unwrap().0,
            "Ubuntu:23.10"
        );
        assert_eq!(
            eco("pkg:apk/alpine/musl?distro=alpine-3.19").unwrap().0,
            "Alpine:v3.19"
        );
        assert_eq!(eco("pkg:rpm/redhat/bash?distro=rhel-9"), None);
        assert_eq!(eco("pkg:deb/debian/openssl"), None);
    }

    #[test]
    fn language_packages() {
        assert_eq!(
            eco("pkg:maven/org.apache.activemq/activemq-broker"),
            Some(("Maven".into(), "org.apache.activemq:activemq-broker".into()))
        );
        assert_eq!(
            eco("pkg:npm/%40babel/core"),
            Some(("npm".into(), "@babel/core".into()))
        );
        assert_eq!(
            eco("pkg:pypi/django"),
            Some(("PyPI".into(), "django".into()))
        );
        assert_eq!(eco("pkg:generic/thing"), None);
    }
}
