//! VEX 檔案讀寫：不覆蓋既有檔案、age 口令加密（`.age`）與解密。
//!
//! 匯出檔含資產清單與處理狀態，與資料庫同等敏感：未加密時由呼叫端提示，`--encrypt` 時只寫出
//! `<檔名>.age`，不產生明文檔。

use std::path::{Path, PathBuf};

use age::secrecy::SecretString;
use rusqlite::Connection;

use super::export::{self, ExportMeta};
use crate::error::CoreError;

pub const AGE_EXTENSION: &str = "age";
/// age 檔案開頭（二進位格式）。
const AGE_MAGIC: &[u8] = b"age-encryption.org/v1";
/// ASCII armor 格式（`age -a`）的開頭。
const AGE_ARMOR: &[u8] = b"-----BEGIN AGE ENCRYPTED FILE-----";

/// 匯出結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    pub path: PathBuf,
    pub findings: usize,
    pub components: usize,
    pub encrypted: bool,
}

/// 加密輸出的路徑：已是 `.age` 時沿用，否則附加 `.age`。
pub fn encrypted_path(path: &Path) -> PathBuf {
    let is_age = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(AGE_EXTENSION));
    if is_age {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(AGE_EXTENSION);
    PathBuf::from(name)
}

/// age 加密檔（二進位或 ASCII armor）。
pub fn is_encrypted(bytes: &[u8]) -> bool {
    let trimmed = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map_or(bytes, |i| &bytes[i..]);
    trimmed.starts_with(AGE_MAGIC) || trimmed.starts_with(AGE_ARMOR)
}

fn already_exists(path: &Path) -> CoreError {
    CoreError::Invalid(format!("{} 已存在，未覆蓋；請指定其他檔名", path.display()))
}

/// 檔案已存在時回報錯誤（例如要求口令之前先檢查）；實際寫入仍以 [`write_new`] 保證不覆蓋。
pub fn ensure_absent(path: &Path) -> Result<(), CoreError> {
    crate::files::ensure_absent(path, already_exists)
}

/// 寫入新檔案；檔案已存在時不覆蓋。寫入失敗時移除不完整的檔案。
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    crate::files::write_new(path, bytes, already_exists)
}

/// 以口令加密（age scrypt）。
pub fn encrypt(plain: &[u8], passphrase: &str) -> Result<Vec<u8>, CoreError> {
    let recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    age::encrypt(&recipient, plain).map_err(|e| CoreError::Invalid(format!("加密失敗：{e}")))
}

/// 以口令解密；口令錯誤或檔案損壞時回報錯誤。
pub fn decrypt(cipher: &[u8], passphrase: &str) -> Result<Vec<u8>, CoreError> {
    let identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_string()));
    age::decrypt(&identity, cipher)
        .map_err(|e| CoreError::Invalid(format!("無法解密 VEX 檔（口令錯誤或檔案損壞）：{e}")))
}

/// 匯出命中到新檔案；`passphrase` 有值時寫出加密的 `.age` 檔（不產生明文檔）。
pub fn export_to_file(
    conn: &Connection,
    ids: &[i64],
    meta: &ExportMeta,
    path: &Path,
    passphrase: Option<&str>,
) -> Result<ExportOutcome, CoreError> {
    // 讀取期間的一致快照（排程 sync 同時寫入時，狀態與時間軸不會互相矛盾）
    let snapshot = conn.unchecked_transaction()?;
    let document = export::build(&snapshot, ids, meta)?;
    drop(snapshot);
    let json = export::to_json(&document)?;
    let (path, bytes) = match passphrase {
        Some(p) => (encrypted_path(path), encrypt(json.as_bytes(), p)?),
        None => (path.to_path_buf(), json.into_bytes()),
    };
    write_new(&path, &bytes)?;
    tracing::info!(
        path = %path.display(),
        findings = document.vulnerabilities.len(),
        encrypted = passphrase.is_some(),
        "已匯出 VEX"
    );
    Ok(ExportOutcome {
        path,
        findings: document.vulnerabilities.len(),
        components: document.components.len(),
        encrypted: passphrase.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_path_appends_age_once() {
        assert_eq!(
            encrypted_path(Path::new("all.json")),
            PathBuf::from("all.json.age")
        );
        assert_eq!(
            encrypted_path(Path::new("all.json.AGE")),
            PathBuf::from("all.json.AGE")
        );
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let cipher = encrypt(b"{\"a\":1}", "correct horse").unwrap();
        assert!(is_encrypted(&cipher));
        assert_eq!(decrypt(&cipher, "correct horse").unwrap(), b"{\"a\":1}");
        let err = decrypt(&cipher, "wrong").unwrap_err().to_string();
        assert!(err.contains("口令錯誤"), "{err}");
        assert!(!is_encrypted(b"{}"));
    }

    #[test]
    fn armored_files_are_recognised_and_decrypted() {
        let recipient = age::scrypt::Recipient::new(SecretString::from("pw".to_string()));
        let armored = age::encrypt_and_armor(&recipient, b"{}").unwrap();
        assert!(is_encrypted(armored.as_bytes()));
        assert_eq!(decrypt(armored.as_bytes(), "pw").unwrap(), b"{}");
    }

    #[test]
    fn write_new_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        write_new(&path, b"one").unwrap();
        let err = write_new(&path, b"two").unwrap_err().to_string();
        assert!(err.contains("已存在"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
    }
}
