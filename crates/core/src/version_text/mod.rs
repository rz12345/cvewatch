//! 版本字串的共用文法（不依賴產品目錄或比對模組）。

pub mod esxi;
pub mod vendor;

/// 修剪、合併空白、小寫，`_` 視為空白（NVD CPE 的 `update_3c`）。
pub fn canonical(raw: &str) -> String {
    raw.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
