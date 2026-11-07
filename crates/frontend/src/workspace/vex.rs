//! 以目前篩選（含搜尋）匯出 VEX（與 `cvewatch vex export` 相同的 core 函式；
//! 不覆蓋既有檔案、不加密）。

use std::path::{Path, PathBuf};

use cvewatch_core::vex::export::ExportMeta;
use cvewatch_core::vex::file as vex_file;

use super::Workspace;
use crate::report::Report;

/// 預設匯出檔名已存在時附加的序號上限（`-2` … `-N`）。
const MAX_NAME_SUFFIX: u32 = 99;

/// `<dir>/<stem>.json`；已存在時改用 `<stem>-2.json`、`<stem>-3.json`…（同一分鐘內多次匯出）。
/// 序號用完時回傳原檔名，由寫入時回報已存在。
fn unused_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.json"));
    std::iter::once(first.clone())
        .chain((2..=MAX_NAME_SUFFIX).map(|n| dir.join(format!("{stem}-{n}.json"))))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

impl<V> Workspace<V> {
    /// 預設輸出路徑（「文件」資料夾下的 `cvewatch-vex-<時間>.json`）與要匯出的命中數；
    /// 目前篩選沒有命中時回報原因。
    pub fn vex_export_default(&self) -> Result<(PathBuf, usize), Report> {
        let count = self.visible_ids().len();
        if count == 0 {
            return Err(Report::info("目前篩選沒有命中，未匯出"));
        }
        let stamp = self
            .services
            .tz
            .format(self.services.now().fixed_offset(), "%Y%m%d-%H%M");
        let path = unused_path(
            &self.services.documents_dir,
            &format!("cvewatch-vex-{stamp}"),
        );
        Ok((path, count))
    }

    /// 匯出目前篩選的命中到 `path`。
    pub fn export_vex(&self, path: &str) -> Report {
        if path.is_empty() {
            return Report::info("未指定檔名，未匯出");
        }
        let ids = self.visible_ids();
        let result = ExportMeta::new(self.services.now()).and_then(|meta| {
            vex_file::export_to_file(&self.conn, &ids, &meta, Path::new(path), None)
        });
        match result {
            Ok(o) => Report::info(format!(
                "已匯出 {} 筆命中至 {}（此檔未加密，含資產與處理狀態）",
                o.findings,
                o.path.display()
            )),
            Err(e) => Report::failure("匯出 VEX 失敗", &e),
        }
    }
}
