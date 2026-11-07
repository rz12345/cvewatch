//! 產品名稱正規化：Unicode NFKC → 小寫 → 移除空白與 `-_./`。

use unicode_normalization::UnicodeNormalization;

const STRIPPED_SYMBOLS: [char; 4] = ['-', '_', '.', '/'];

pub fn normalize(name: &str) -> String {
    name.nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace() && !STRIPPED_SYMBOLS.contains(c))
        .collect()
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
    fn keeps_other_symbols() {
        assert_eq!(
            normalize("DiskStation Manager (DSM)"),
            "diskstationmanager(dsm)"
        );
    }
}
