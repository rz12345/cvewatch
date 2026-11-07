//! CVE 指派單位（CNA）標註。TWCERT/CC 指派的 CVE 標為國內通報來源。

/// TWCERT/CC 在 cvelistV5 的 `assignerShortName`。
pub const TWCERT_SHORT_NAME: &str = "twcert";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignerInfo {
    pub label: String,
    /// 國內通報來源（TWCERT/CC）
    pub domestic: bool,
}

/// 常見指派單位的顯示名稱；未列出者直接顯示 shortName。
const KNOWN: [(&str, &str); 12] = [
    (TWCERT_SHORT_NAME, "TWCERT/CC"),
    ("palo_alto", "Palo Alto Networks"),
    ("vmware", "VMware by Broadcom"),
    ("synology", "Synology"),
    ("dell", "Dell"),
    ("redhat", "Red Hat"),
    ("microsoft", "Microsoft"),
    ("mitre", "MITRE"),
    ("GitHub_M", "GitHub"),
    ("Linux", "Linux kernel"),
    ("cisa-cg", "CISA"),
    ("VulnCheck", "VulnCheck"),
];

pub fn describe(short_name: Option<&str>) -> AssignerInfo {
    let Some(short) = short_name.map(str::trim).filter(|s| !s.is_empty()) else {
        return AssignerInfo {
            label: "（未知）".to_string(),
            domestic: false,
        };
    };
    let label = KNOWN
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(short))
        .map_or_else(|| short.to_string(), |(_, v)| (*v).to_string());
    AssignerInfo {
        domestic: short.eq_ignore_ascii_case(TWCERT_SHORT_NAME),
        label,
    }
}

/// `cve show` 的「指派單位」欄位文字。
pub fn display(short_name: Option<&str>) -> String {
    let info = describe(short_name);
    if info.domestic {
        format!("{}（國內通報來源）", info.label)
    } else {
        info.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twcert_is_domestic() {
        let info = describe(Some("twcert"));
        assert_eq!(info.label, "TWCERT/CC");
        assert!(info.domestic);
        assert_eq!(display(Some("TWCERT")), "TWCERT/CC（國內通報來源）");
    }

    #[test]
    fn others_are_labelled() {
        assert_eq!(display(Some("palo_alto")), "Palo Alto Networks");
        assert_eq!(display(Some("some_cna")), "some_cna");
        assert_eq!(display(None), "（未知）");
        assert!(!describe(Some("mitre")).domestic);
    }
}
