//! 產品名稱正規化：Unicode NFKC → 小寫 → 移除空白與 `-_./`。

use unicode_normalization::UnicodeNormalization;

const STRIPPED_SYMBOLS: [char; 4] = ['-', '_', '.', '/'];

pub fn normalize(name: &str) -> String {
    name.nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace() && !STRIPPED_SYMBOLS.contains(c))
        .collect()
}

/// 廠商名稱尾端常見的公司型態字樣（比較時移除；`networks` 等字保留）。
const COMPANY_SUFFIXES: [&str; 14] = [
    "inc",
    "incorporated",
    "ltd",
    "limited",
    "co",
    "corp",
    "corporation",
    "llc",
    "gmbh",
    "ag",
    "sa",
    "plc",
    "bv",
    "company",
];

/// 廠商名稱正規化：移除尾端的公司型態字樣與逗號後再 [`normalize`]。
///
/// 例如 `Palo Alto Networks, Inc.` 與 `Palo Alto Networks` 都得到 `paloaltonetworks`。
pub fn normalize_vendor(name: &str) -> String {
    let lowered: String = name.nfkc().flat_map(char::to_lowercase).collect();
    let mut words: Vec<&str> = lowered
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .collect();
    while words.len() > 1 {
        let last = words[words.len() - 1].trim_matches('.').replace('.', "");
        if !COMPANY_SUFFIXES.contains(&last.as_str()) {
            break;
        }
        words.pop();
    }
    normalize(&words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_case_spaces_and_symbols() {
        assert_eq!(normalize("Palo Alto PA-460"), "paloaltopa460");
        assert_eq!(normalize("PAN-OS"), "panos");
        assert_eq!(normalize(" pan_os "), "panos");
        assert_eq!(normalize("Debian GNU/Linux"), "debiangnulinux");
        assert_eq!(normalize("vCenter.Server"), "vcenterserver");
    }

    #[test]
    fn applies_nfkc_to_fullwidth_characters() {
        assert_eq!(normalize("ＰＡＮ－ＯＳ"), "panos");
        assert_eq!(normalize("ESXi\u{3000}8"), "esxi8");
    }

    #[test]
    fn vendor_suffixes_are_removed() {
        assert_eq!(
            normalize_vendor("Palo Alto Networks, Inc."),
            "paloaltonetworks"
        );
        assert_eq!(normalize_vendor("Palo Alto Networks"), "paloaltonetworks");
        assert_eq!(normalize_vendor("Synology Inc."), "synology");
        assert_eq!(normalize_vendor("ACME Co., Ltd."), "acme");
        assert_eq!(normalize_vendor("Dell"), "dell");
        // 只有單一字時不移除
        assert_eq!(normalize_vendor("Inc"), "inc");
        // 字中間的 co 不受影響
        assert_eq!(normalize_vendor("Cisco"), "cisco");
    }

    #[test]
    fn keeps_other_symbols() {
        assert_eq!(
            normalize("DiskStation Manager (DSM)"),
            "diskstationmanager(dsm)"
        );
    }
}
