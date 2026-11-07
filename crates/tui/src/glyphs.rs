//! 框線與標記字元。舊版 conhost 對框線字元支援不佳：不在 Windows Terminal（無 `WT_SESSION`）時改用 ASCII。

use ratatui::symbols::border;

/// Windows Terminal 會設定此環境變數。
pub const WINDOWS_TERMINAL_ENV: &str = "WT_SESSION";

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    pub border: border::Set<'static>,
    pub ok: &'static str,
    pub fail: &'static str,
    pub running: &'static str,
    /// 多選標記
    pub mark: &'static str,
    /// 目前列標記
    pub highlight: &'static str,
    pub separator: &'static str,
    pub arrow: &'static str,
    pub warn: &'static str,
    pub up_down: &'static str,
    pub left_right: &'static str,
    /// 截斷標記（顯示寬度 1）。conhost 的中文字型把 `…` 畫成 2 欄，會使欄位錯位，因此用 `~`。
    pub ellipsis: char,
}

pub const UNICODE: Glyphs = Glyphs {
    border: border::PLAIN,
    ok: "✓",
    fail: "✗",
    running: "…",
    mark: "●",
    highlight: "▶ ",
    separator: "─",
    arrow: "→",
    warn: "⚠",
    up_down: "↑↓",
    left_right: "←→",
    ellipsis: '…',
};

pub const ASCII: Glyphs = Glyphs {
    border: ASCII_BORDER,
    ok: "OK",
    fail: "NG",
    running: "..",
    mark: "*",
    highlight: "> ",
    separator: "-",
    arrow: "->",
    warn: "!",
    up_down: "Up/Down",
    left_right: "Left/Right",
    ellipsis: '~',
};

/// 依是否在 Windows Terminal 中選擇字元。
pub fn for_terminal(windows_terminal: bool) -> Glyphs {
    if windows_terminal {
        UNICODE
    } else {
        ASCII
    }
}

/// 由環境變數偵測。
pub fn detect() -> Glyphs {
    for_terminal(std::env::var_os(WINDOWS_TERMINAL_ENV).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conhost_uses_ascii_borders() {
        let g = for_terminal(false);
        assert_eq!(g, ASCII);
        let all = [
            g.border.top_left,
            g.ok,
            g.fail,
            g.running,
            g.mark,
            g.highlight,
            g.separator,
            g.arrow,
            g.warn,
            g.up_down,
            g.left_right,
        ];
        assert!(all.iter().all(|s| s.is_ascii()));
        assert!(g.ellipsis.is_ascii());
    }

    #[test]
    fn windows_terminal_uses_box_drawing() {
        assert_eq!(for_terminal(true).border.top_left, "┌");
    }
}
