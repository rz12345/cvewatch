//! Windows 檔案對話框（另存 VEX、範本與清冊、選擇清冊、元件檔、IP 檔與 ISMS 清冊）。以 trait 抽象，測試以替身取代。

use std::path::{Path, PathBuf};

pub trait FileDialogs {
    /// 另存新檔：預設資料夾與檔名。取消時為 None。
    fn save_vex(&self, dir: &Path, file_name: &str) -> Option<PathBuf>;
    /// 另存 Excel（.xlsx）：對話框標題、預設資料夾與檔名。取消時為 None。
    fn save_xlsx(&self, title: &str, dir: &Path, file_name: &str) -> Option<PathBuf>;
    /// 開啟資產清冊（.xlsx／.csv）。
    fn open_inventory(&self) -> Option<PathBuf>;
    /// 開啟 CSV 清冊的元件檔。
    fn open_components(&self) -> Option<PathBuf>;
    /// 開啟 CSV 清冊的 IP 檔（20 design D11）。
    fn open_ips(&self) -> Option<PathBuf>;
    /// 開啟 ISMS 資訊資產清冊（.xlsx）。
    fn open_isms_workbook(&self) -> Option<PathBuf>;
}

/// 原生對話框（`rfd`，會阻塞 UI 執行緒直到使用者關閉對話框）。
pub struct NativeDialogs;

impl FileDialogs for NativeDialogs {
    fn save_vex(&self, dir: &Path, file_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("匯出 VEX")
            .set_directory(dir)
            .set_file_name(file_name)
            .add_filter("CycloneDX VEX（JSON）", &["json"])
            .save_file()
    }

    fn save_xlsx(&self, title: &str, dir: &Path, file_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title(title)
            .set_directory(dir)
            .set_file_name(file_name)
            .add_filter("Excel 活頁簿（.xlsx）", &["xlsx"])
            .save_file()
    }

    fn open_inventory(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("匯入資產清冊")
            .add_filter("資產清冊（.xlsx、.csv）", &["xlsx", "csv"])
            .pick_file()
    }

    fn open_components(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("選擇元件檔")
            .add_filter("元件檔（.csv）", &["csv"])
            .pick_file()
    }

    fn open_ips(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("選擇 IP 檔")
            .add_filter("IP 檔（.csv）", &["csv"])
            .pick_file()
    }

    fn open_isms_workbook(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("選擇 ISMS 資訊資產清冊")
            .add_filter("ISMS 資訊資產清冊（.xlsx）", &["xlsx"])
            .pick_file()
    }
}

/// 清冊檔的種類（由副檔名判斷，不分大小寫）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryKind {
    Xlsx,
    Csv,
}

pub fn inventory_kind(path: &Path) -> Option<InventoryKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "xlsx" => Some(InventoryKind::Xlsx),
        "csv" => Some(InventoryKind::Csv),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_extensions() {
        assert_eq!(
            inventory_kind(Path::new("A.XLSX")),
            Some(InventoryKind::Xlsx)
        );
        assert_eq!(
            inventory_kind(Path::new(r"C:\x\assets.csv")),
            Some(InventoryKind::Csv)
        );
        assert_eq!(inventory_kind(Path::new("report.pdf")), None);
        assert_eq!(inventory_kind(Path::new("assets")), None);
        assert_eq!(inventory_kind(Path::new("assets.xls")), None);
    }
}
