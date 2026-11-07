//! `dpkg-query -W -f='${Package}\t${Version}\n'` 與
//! `rpm -qa --qf '%{NAME}\t%{EPOCH}:%{VERSION}-%{RELEASE}\n'` 的文字輸出。

use packageurl::PackageUrl;

use super::{Distro, PackageFormat, RawEntry};
use crate::error::CoreError;

const RPM_NO_EPOCH: &str = "(none)";

/// rpm 發行版名稱 → purl namespace（未列出者沿用發行版名稱）。
const RPM_NAMESPACES: [(&str, &str); 7] = [
    ("rhel", "redhat"),
    ("centos", "centos"),
    ("rocky", "rocky"),
    ("alma", "almalinux"),
    ("almalinux", "almalinux"),
    ("ol", "oracle"),
    ("amzn", "amazon"),
];

/// `(none):…` 或 `<數字>:…`。
pub fn has_rpm_epoch_prefix(version: &str) -> bool {
    match version.split_once(':') {
        Some((epoch, _)) => epoch == RPM_NO_EPOCH || epoch.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

pub fn parse(
    text: &str,
    format: PackageFormat,
    distro: &Distro,
) -> Result<Vec<RawEntry>, CoreError> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (name, version) = line.split_once('\t').ok_or_else(|| {
            CoreError::Invalid(format!("第 {} 行不是 name<TAB>version 格式", i + 1))
        })?;
        let (name, version) = (name.trim(), version.trim());
        if version.is_empty() {
            // dpkg 列出已移除（只剩設定檔）的套件時版本為空
            continue;
        }
        let purl = match format {
            PackageFormat::Rpm => rpm_purl(name, version, distro),
            _ => deb_purl(name, version, distro),
        }
        .map_err(|e| CoreError::Invalid(format!("第 {} 行無法產生 purl：{e}", i + 1)))?;
        out.push(RawEntry::Purl(purl));
    }
    Ok(out)
}

/// `pkg:deb/<distro 名稱>/<name>@<version>?distro=<distro>`；`name:arch` 轉為 `arch` qualifier。
fn deb_purl(name: &str, version: &str, distro: &Distro) -> Result<String, packageurl::Error> {
    let (name, arch) = match name.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (name, None),
    };
    let mut purl = PackageUrl::new("deb", name)?;
    purl.with_namespace(distro.name.clone())?
        .with_version(version)?
        .add_qualifier("distro", distro.qualifier())?;
    if let Some(arch) = arch {
        purl.add_qualifier("arch", arch)?;
    }
    Ok(purl.to_string())
}

/// `pkg:rpm/<namespace>/<name>@<version>-<release>?distro=<distro>[&epoch=N]`
fn rpm_purl(name: &str, evr: &str, distro: &Distro) -> Result<String, packageurl::Error> {
    let (epoch, vr) = match evr.split_once(':') {
        Some((e, rest)) if has_rpm_epoch_prefix(evr) => (Some(e), rest),
        _ => (None, evr),
    };
    let namespace = RPM_NAMESPACES
        .iter()
        .find(|(d, _)| *d == distro.name)
        .map_or(distro.name.as_str(), |(_, ns)| ns);
    let mut purl = PackageUrl::new("rpm", name)?;
    purl.with_namespace(namespace.to_string())?
        .with_version(vr)?
        .add_qualifier("distro", distro.qualifier())?;
    if let Some(e) = epoch.filter(|e| *e != RPM_NO_EPOCH && *e != "0") {
        purl.add_qualifier("epoch", e)?;
    }
    Ok(purl.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debian() -> Distro {
        Distro::parse("debian-12").unwrap()
    }

    #[test]
    fn deb_purl_matches_spec_example() {
        assert_eq!(
            deb_purl("openssl", "3.0.15-1~deb12u1", &debian()).unwrap(),
            "pkg:deb/debian/openssl@3.0.15-1~deb12u1?distro=debian-12"
        );
        assert_eq!(
            deb_purl("libc6:amd64", "2.36-9", &debian()).unwrap(),
            "pkg:deb/debian/libc6@2.36-9?arch=amd64&distro=debian-12"
        );
    }

    #[test]
    fn rpm_purl_handles_epoch() {
        let rhel = Distro::parse("rhel-9").unwrap();
        assert_eq!(
            rpm_purl("openssl", "1:3.0.7-27.el9", &rhel).unwrap(),
            "pkg:rpm/redhat/openssl@3.0.7-27.el9?distro=rhel-9&epoch=1"
        );
        assert_eq!(
            rpm_purl("bash", "(none):5.1.8-9.el9", &rhel).unwrap(),
            "pkg:rpm/redhat/bash@5.1.8-9.el9?distro=rhel-9"
        );
    }

    #[test]
    fn epoch_prefix_detection() {
        assert!(has_rpm_epoch_prefix("(none):1.2-3"));
        assert!(has_rpm_epoch_prefix("2:1.2-3"));
        assert!(!has_rpm_epoch_prefix("3.0.15-1~deb12u1"));
        assert!(!has_rpm_epoch_prefix("1.2:3"));
    }
}
