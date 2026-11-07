//! 可開啟的連結：CVE.org、NVD、原廠公告與 CVE 紀錄的參考連結。

use cvewatch_core::ingest::show::CveDetail;

const CVE_ORG: &str = "https://www.cve.org/CVERecord?id=";
const NVD: &str = "https://nvd.nist.gov/vuln/detail/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// 只開啟 http(s) 連結：參考連結來自外部資料，不得讓它啟動其他協定或程式。
pub fn is_safe_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// CVE.org、NVD → 原廠修正公告 → 參考連結；去除重複與不安全的網址。
pub fn for_cve(cve_id: &str, detail: Option<&CveDetail>) -> Vec<Link> {
    let mut links = vec![
        Link {
            label: "CVE.org".into(),
            url: format!("{CVE_ORG}{cve_id}"),
        },
        Link {
            label: "NVD".into(),
            url: format!("{NVD}{cve_id}"),
        },
    ];
    if let Some(d) = detail {
        let vendor = d.fixes.iter().filter_map(|f| {
            f.url.as_ref().map(|url| Link {
                label: format!("原廠公告 {} {}", f.provider, f.product),
                url: url.clone(),
            })
        });
        let refs = d.references.iter().enumerate().map(|(i, url)| Link {
            label: format!("參考 {}", i + 1),
            url: url.clone(),
        });
        links.extend(vendor.chain(refs));
    }
    let mut seen = std::collections::HashSet::new();
    links
        .into_iter()
        .filter(|l| is_safe_url(&l.url) && seen.insert(l.url.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_urls() {
        assert!(is_safe_url(
            "https://security.paloaltonetworks.com/CVE-2024-3400"
        ));
        assert!(is_safe_url("HTTP://example.com"));
        assert!(!is_safe_url("file:///C:/Windows/system32/calc.exe"));
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("https://a.com/ x"));
    }

    #[test]
    fn without_detail_has_cve_org_and_nvd() {
        let links = for_cve("CVE-2024-3400", None);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].url,
            "https://www.cve.org/CVERecord?id=CVE-2024-3400"
        );
        assert_eq!(
            links[1].url,
            "https://nvd.nist.gov/vuln/detail/CVE-2024-3400"
        );
    }
}
