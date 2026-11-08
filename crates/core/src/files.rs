//! 匯出檔案的共用寫入：不覆蓋既有檔案（`create_new`，沒有「先檢查再寫入」的競態）。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::error::CoreError;

/// 目的檔已存在時回傳 `already_exists(path)`（例如要求口令或開啟資料庫之前先檢查）；
/// 實際寫入仍以 [`write_new`] 保證不覆蓋。
pub fn ensure_absent(path: &Path, already_exists: fn(&Path) -> CoreError) -> Result<(), CoreError> {
    if path.exists() {
        return Err(already_exists(path));
    }
    Ok(())
}

/// 寫入新檔案；檔案已存在時回傳 `already_exists(path)`，不覆蓋。寫入失敗時移除不完整的檔案。
pub fn write_new(
    path: &Path,
    bytes: &[u8],
    already_exists: fn(&Path) -> CoreError,
) -> Result<(), CoreError> {
    let file_error =
        |e: std::io::Error| CoreError::Invalid(format!("無法寫入 {}：{e}", path.display()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => already_exists(path),
            _ => file_error(e),
        })?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(e) = written {
        drop(file);
        if let Err(rm) = std::fs::remove_file(path) {
            tracing::warn!(path = %path.display(), error = %rm, "無法移除寫入失敗的檔案");
        }
        return Err(file_error(e));
    }
    Ok(())
}
