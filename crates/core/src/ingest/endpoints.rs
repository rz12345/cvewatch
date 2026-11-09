//! 各來源的端點（見 design.md 的端點表）。測試以 [`Endpoints::with_base`] 改指向 wiremock。

/// cvelistV5 GitHub repo。
pub const CVELIST_REPO: &str = "CVEProject/cvelistV5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// GitHub REST API，例如 `https://api.github.com`
    pub github_api: String,
    /// release 下載：`<base>/<tag>/<asset>`
    pub github_download: String,
    /// 單筆紀錄：`<base>/cves/<year>/<Nxxx>/<id>.json`
    pub cvelist_raw: String,
    pub nvd: String,
    pub kev: String,
    pub epss: String,
    /// OSV API base，例如 `https://api.osv.dev/v1`
    pub osv: String,
    /// endoflife.date API v1 base（22 design D1）
    pub endoflife: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            github_api: "https://api.github.com".into(),
            github_download: format!("https://github.com/{CVELIST_REPO}/releases/download"),
            cvelist_raw: format!("https://raw.githubusercontent.com/{CVELIST_REPO}/main"),
            nvd: "https://services.nvd.nist.gov/rest/json/cves/2.0".into(),
            kev: "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json"
                .into(),
            epss: "https://api.first.org/data/v1/epss".into(),
            osv: "https://api.osv.dev/v1".into(),
            endoflife: "https://endoflife.date/api/v1".into(),
        }
    }
}

impl Endpoints {
    /// 全部指向同一個 base（測試用），路徑前綴區分來源。
    pub fn with_base(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Endpoints {
            github_api: format!("{base}/gh-api"),
            github_download: format!("{base}/gh-dl"),
            cvelist_raw: format!("{base}/raw"),
            nvd: format!("{base}/nvd"),
            kev: format!("{base}/kev.json"),
            epss: format!("{base}/epss"),
            osv: format!("{base}/osv"),
            endoflife: format!("{base}/eol"),
        }
    }

    pub fn releases_url(&self, per_page: u32) -> String {
        format!(
            "{}/repos/{CVELIST_REPO}/releases?per_page={per_page}",
            self.github_api
        )
    }

    /// `GET /products/{slug}`（代稱已在目錄載入時驗證為 `[a-z0-9-]+`，不需要跳脫）。
    pub fn endoflife_product_url(&self, slug: &str) -> String {
        format!("{}/products/{slug}", self.endoflife)
    }

    pub fn download_url(&self, tag: &str, asset: &str) -> String {
        format!("{}/{tag}/{asset}", self.github_download)
    }

    /// `CVE-2024-3400` → `<raw>/cves/2024/3xxx/CVE-2024-3400.json`；格式不符時為 None。
    pub fn record_url(&self, cve_id: &str) -> Option<String> {
        let mut parts = cve_id.split('-');
        let (Some("CVE"), Some(year), Some(number), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        let valid = year.len() == 4
            && year.chars().all(|c| c.is_ascii_digit())
            && number.len() >= 4
            && number.chars().all(|c| c.is_ascii_digit());
        if !valid {
            return None;
        }
        let bucket = format!("{}xxx", &number[..number.len() - 3]);
        Some(format!(
            "{}/cves/{year}/{bucket}/{cve_id}.json",
            self.cvelist_raw
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_url_buckets() {
        let e = Endpoints::default();
        assert_eq!(
            e.record_url("CVE-2024-3400").unwrap(),
            "https://raw.githubusercontent.com/CVEProject/cvelistV5/main/cves/2024/3xxx/CVE-2024-3400.json"
        );
        assert!(e
            .record_url("CVE-2026-104080")
            .unwrap()
            .ends_with("/cves/2026/104xxx/CVE-2026-104080.json"));
        assert!(e.record_url("CVE-2024-12").is_none());
        assert!(e.record_url("GHSA-xxxx").is_none());
        assert!(e.record_url("CVE-2024-3400/../x").is_none());
    }

    #[test]
    fn test_base_prefixes() {
        let e = Endpoints::with_base("http://127.0.0.1:9/");
        assert_eq!(
            e.download_url("t", "a.zip"),
            "http://127.0.0.1:9/gh-dl/t/a.zip"
        );
        assert!(e
            .releases_url(30)
            .starts_with("http://127.0.0.1:9/gh-api/repos/"));
        assert_eq!(
            e.endoflife_product_url("panos"),
            "http://127.0.0.1:9/eol/products/panos"
        );
        assert_eq!(
            Endpoints::default().endoflife_product_url("panos"),
            "https://endoflife.date/api/v1/products/panos"
        );
    }
}
