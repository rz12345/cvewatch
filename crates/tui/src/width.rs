//! 依顯示寬度截斷文字（中文全形字佔 2 欄），超出時加上省略符號。

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const ELLIPSIS: char = '…';

/// 顯示寬度（欄數）。
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// 控制字元（換行、tab 等）換成空白，避免表格儲存格換行破版（與 GUI 共用）。
pub use cvewatch_frontend::display::one_line as sanitize;

/// 截斷到顯示寬度 `max` 以內；被截斷時最後一欄為 `…`。全形字放不下時整個略過，不切半個字。
pub fn truncate(s: &str, max: usize) -> String {
    truncate_with(s, max, ELLIPSIS)
}

/// 同 [`truncate`]，以 `ellipsis`（顯示寬度 1 的字元）標示截斷；conhost 用 ASCII 的 `~`。
pub fn truncate_with(s: &str, max: usize, ellipsis: char) -> String {
    let clean = sanitize(s);
    if display_width(&clean) <= max {
        return clean;
    }
    if max == 0 {
        return String::new();
    }
    let budget = max.saturating_sub(ellipsis.width().unwrap_or(1));
    let mut out = String::new();
    let mut used = 0;
    for c in clean.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push(ellipsis);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(truncate("PA-460", 10), "PA-460");
        assert_eq!(truncate("防火牆", 6), "防火牆");
    }

    #[test]
    fn long_chinese_name_is_cut_by_display_width() {
        let name = "資訊所機房核心防火牆主機";
        assert_eq!(display_width(name), 24);
        let cut = truncate(name, 10);
        assert_eq!(cut, "資訊所機…");
        assert!(display_width(&cut) <= 10);
    }

    #[test]
    fn wide_char_that_does_not_fit_is_dropped_whole() {
        // 預算 4 欄（含省略符號 1 欄）：「a資」=3，下一個「訊」放不下
        let cut = truncate("a資訊所", 4);
        assert_eq!(cut, "a資…");
        assert_eq!(display_width(&cut), 4);
    }

    #[test]
    fn mixed_ascii_and_chinese() {
        let cut = truncate("PVE-01 虛擬化主機 Proxmox", 12);
        assert_eq!(cut, "PVE-01 虛擬…");
        assert!(display_width(&cut) <= 12);
        for max in 0..30 {
            assert!(display_width(&truncate("PVE-01 虛擬化主機 Proxmox", max)) <= max);
        }
    }

    #[test]
    fn edge_widths() {
        assert_eq!(truncate("abc", 0), "");
        assert_eq!(truncate("abc", 1), "…");
        assert_eq!(truncate("資訊", 1), "…");
        assert_eq!(truncate("資訊", 2), "…");
    }

    #[test]
    fn ascii_ellipsis_for_conhost() {
        let cut = truncate_with("資訊所機房核心防火牆主機", 10, '~');
        assert_eq!(cut, "資訊所機~");
        assert!(cut.chars().last().is_some_and(|c| c.is_ascii()));
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(truncate("a\nb\tc", 10), "a b c");
    }
}
