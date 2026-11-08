//! 繁體中文字型：egui 內建字型沒有中日韓字元，啟動時載入 Windows 內建字型作為後備
//! （排在內建字型之後，英數字仍用內建字型）。不打包字型。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

/// 依序嘗試的字型檔：微軟正黑體、微軟正黑體 Light、細明體。
pub const CANDIDATES: [&str; 3] = ["msjh.ttc", "msjhl.ttc", "mingliu.ttc"];

/// 字型名稱（`FontDefinitions` 中的鍵）。
const FONT_NAME: &str = "cvewatch-cjk";

/// 找不到中文字型時的警告。
pub const MISSING_WARNING: &str = "找不到中文字型（微軟正黑體或細明體），中文可能無法正確顯示";

/// 已讀取的字型檔。
pub struct CjkFont {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// `%WINDIR%\Fonts`（環境變數不存在時用 `C:\Windows\Fonts`）。
pub fn system_fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("Fonts")
}

/// 依 [`CANDIDATES`] 順序讀取第一個可讀的字型檔；都沒有時回傳警告文字。
pub fn find(dir: &Path) -> Result<CjkFont, String> {
    for name in CANDIDATES {
        let path = dir.join(name);
        match std::fs::read(&path) {
            Ok(bytes) if !bytes.is_empty() => return Ok(CjkFont { path, bytes }),
            Ok(_) => tracing::warn!(path = %path.display(), "字型檔是空的"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "無法讀取字型檔"),
        }
    }
    Err(MISSING_WARNING.to_string())
}

/// 把字型加到 Proportional 與 Monospace 字族的後備清單尾端。
pub fn definitions(font: CjkFont) -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert(
        FONT_NAME.to_string(),
        Arc::new(egui::FontData::from_owned(font.bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        defs.families
            .entry(family)
            .or_default()
            .push(FONT_NAME.to_string());
    }
    defs
}

/// 載入系統中文字型；找不到時回傳警告（照常啟動）。
pub fn install(ctx: &egui::Context) -> Option<String> {
    match find(&system_fonts_dir()) {
        Ok(font) => {
            tracing::info!(path = %font.path.display(), "載入中文字型");
            ctx.set_fonts(definitions(font));
            None
        }
        Err(warning) => {
            tracing::warn!("{warning}");
            Some(warning)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_existing_candidate_is_used() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mingliu.ttc"), b"mingliu").unwrap();
        std::fs::write(dir.path().join("msjhl.ttc"), b"light").unwrap();
        let font = find(dir.path()).unwrap();
        assert_eq!(font.path, dir.path().join("msjhl.ttc"));
        assert_eq!(font.bytes, b"light");
        std::fs::write(dir.path().join("msjh.ttc"), b"regular").unwrap();
        assert_eq!(find(dir.path()).unwrap().path, dir.path().join("msjh.ttc"));
    }

    #[test]
    fn missing_fonts_give_warning_without_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find(dir.path()).err().as_deref(), Some(MISSING_WARNING));
        assert!(find(&dir.path().join("no-such-dir")).is_err());
    }

    #[test]
    fn font_is_fallback_after_builtin() {
        let defs = definitions(CjkFont {
            path: PathBuf::from("x.ttc"),
            bytes: vec![1, 2, 3],
        });
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = &defs.families[&family];
            assert_eq!(list.last().map(String::as_str), Some(FONT_NAME));
            assert!(list.len() > 1, "內建字型仍在前面");
        }
    }
}
