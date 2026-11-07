//! 資料庫金鑰：32 bytes，由作業系統安全亂數產生，記憶體中以 [`Zeroizing`] 保存。

use std::fmt;

use data_encoding::{BASE32_NOPAD, HEXUPPER};
use rand::rngs::SysRng;
use rand::TryRng;
use zeroize::Zeroizing;

use crate::error::CoreError;

pub const DB_KEY_LEN: usize = 32;
/// 復原金鑰 Base32 顯示時每組的字元數。
pub const RECOVERY_GROUP_LEN: usize = 4;
const RECOVERY_SEPARATOR: char = '-';

pub struct DbKey(Zeroizing<[u8; DB_KEY_LEN]>);

impl DbKey {
    /// 以作業系統 CSPRNG 產生新金鑰。
    pub fn generate() -> Result<Self, CoreError> {
        let mut bytes = Zeroizing::new([0u8; DB_KEY_LEN]);
        SysRng
            .try_fill_bytes(bytes.as_mut())
            .map_err(|e| CoreError::Invalid(format!("無法取得系統亂數：{e}")))?;
        Ok(DbKey(bytes))
    }

    fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; DB_KEY_LEN] = bytes.try_into().ok()?;
        Some(DbKey(Zeroizing::new(array)))
    }

    /// 憑證庫中保存的格式（64 個十六進位字元）。
    pub fn from_hex(hex: &str) -> Result<Self, CoreError> {
        let decoded = Zeroizing::new(
            HEXUPPER
                .decode(hex.trim().to_ascii_uppercase().as_bytes())
                .map_err(|_| CoreError::DbKeyInvalid)?,
        );
        Self::from_slice(&decoded).ok_or(CoreError::DbKeyInvalid)
    }

    pub fn to_hex(&self) -> Zeroizing<String> {
        Zeroizing::new(HEXUPPER.encode(self.0.as_ref()))
    }

    /// SQLCipher raw key 格式：`x'<64 hex>'`（不經 KDF）。
    pub fn pragma_value(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("x'{}'", self.to_hex().as_str()))
    }

    /// 給人抄寫的復原金鑰：Base32、每 4 字元以 `-` 分組。
    pub fn to_recovery_string(&self) -> Zeroizing<String> {
        let encoded = Zeroizing::new(BASE32_NOPAD.encode(self.0.as_ref()));
        let groups: Vec<&str> = encoded
            .as_bytes()
            .chunks(RECOVERY_GROUP_LEN)
            .map(|c| std::str::from_utf8(c).unwrap_or_default())
            .collect();
        Zeroizing::new(groups.join(&RECOVERY_SEPARATOR.to_string()))
    }

    /// 解析復原金鑰；忽略分隔符號、空白與大小寫。
    pub fn from_recovery_string(input: &str) -> Result<Self, CoreError> {
        let normalized: Zeroizing<String> = Zeroizing::new(
            input
                .chars()
                .filter(|c| !c.is_whitespace() && *c != RECOVERY_SEPARATOR)
                .map(|c| c.to_ascii_uppercase())
                .collect(),
        );
        let decoded = Zeroizing::new(
            BASE32_NOPAD
                .decode(normalized.as_bytes())
                .map_err(|_| invalid_recovery())?,
        );
        Self::from_slice(&decoded).ok_or_else(invalid_recovery)
    }
}

fn invalid_recovery() -> CoreError {
    CoreError::Invalid("復原金鑰格式不正確（應為 Base32 分組字串）".to_string())
}

impl fmt::Debug for DbKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DbKey(***)")
    }
}

impl PartialEq for DbKey {
    fn eq(&self, other: &Self) -> bool {
        self.0[..] == other.0[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_differ() {
        let a = DbKey::generate().unwrap();
        let b = DbKey::generate().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn hex_roundtrip() {
        let key = DbKey::generate().unwrap();
        let hex = key.to_hex();
        assert_eq!(hex.len(), DB_KEY_LEN * 2);
        assert_eq!(DbKey::from_hex(&hex).unwrap(), key);
        assert_eq!(DbKey::from_hex(&hex.to_lowercase()).unwrap(), key);
    }

    #[test]
    fn invalid_hex_is_rejected() {
        assert!(DbKey::from_hex("zz").is_err());
        assert!(DbKey::from_hex("ABCD").is_err());
    }

    #[test]
    fn pragma_value_format() {
        let key = DbKey::from_hex(&"AB".repeat(DB_KEY_LEN)).unwrap();
        assert_eq!(
            key.pragma_value().as_str(),
            format!("x'{}'", "AB".repeat(32))
        );
    }

    #[test]
    fn recovery_string_is_grouped_and_roundtrips() {
        let key = DbKey::generate().unwrap();
        let rec = key.to_recovery_string();
        let groups: Vec<&str> = rec.split('-').collect();
        assert!(groups
            .iter()
            .all(|g| g.len() <= RECOVERY_GROUP_LEN && !g.is_empty()));
        assert_eq!(groups.concat().len(), 52);
        assert_eq!(DbKey::from_recovery_string(&rec).unwrap(), key);
        let sloppy = format!("  {}  ", rec.to_lowercase().replace('-', " "));
        assert_eq!(DbKey::from_recovery_string(&sloppy).unwrap(), key);
    }

    #[test]
    fn invalid_recovery_string_is_rejected() {
        assert!(DbKey::from_recovery_string("ABCD-EFGH").is_err());
        assert!(DbKey::from_recovery_string("!!!!").is_err());
    }

    #[test]
    fn debug_does_not_reveal_key() {
        let key = DbKey::generate().unwrap();
        assert_eq!(format!("{key:?}"), "DbKey(***)");
    }
}
