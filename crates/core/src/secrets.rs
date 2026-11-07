//! 秘密存放：Windows Credential Manager（`keyring`），service 名稱固定 `cvewatch`。
//!
//! 秘密只存在憑證庫與記憶體中（以 [`Zeroizing`] 包裝），不寫入設定檔、資料庫或日誌。

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use zeroize::Zeroizing;

pub const KEYRING_SERVICE: &str = "cvewatch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKey {
    NvdApiKey,
    SmtpPassword,
    RedmineApiKey,
    /// 資料庫金鑰；只能經由 `db` 子命令管理，不能用 `config set-secret` 設定。
    DbKey,
    /// `db rekey` 進行中的新金鑰；更換完成後刪除，中斷時用來恢復。
    DbKeyPending,
}

impl SecretKey {
    /// 可用 `config set-secret` 設定的秘密。
    pub const USER_SETTABLE: [SecretKey; 3] = [
        SecretKey::NvdApiKey,
        SecretKey::SmtpPassword,
        SecretKey::RedmineApiKey,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SecretKey::NvdApiKey => "nvd_api_key",
            SecretKey::SmtpPassword => "smtp_password",
            SecretKey::RedmineApiKey => "redmine_api_key",
            SecretKey::DbKey => "db_key",
            SecretKey::DbKeyPending => "db_key_pending",
        }
    }

    /// 解析 `config set-secret <key>` 的參數。
    pub fn parse_user_settable(name: &str) -> Result<SecretKey, SecretError> {
        SecretKey::USER_SETTABLE
            .into_iter()
            .find(|k| k.as_str() == name)
            .ok_or_else(|| SecretError::UnknownKey(name.to_string()))
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("不支援的秘密鍵 `{0}`；可用：nvd_api_key、smtp_password、redmine_api_key")]
    UnknownKey(String),

    #[error("秘密內容不可為空")]
    Empty,

    #[error("無法存取 Windows Credential Manager（{key}）：{reason}")]
    Backend { key: SecretKey, reason: String },
}

/// 秘密存放介面；正式環境為 [`KeyringStore`]，測試使用 [`MemoryStore`]。
pub trait SecretStore {
    fn get(&self, key: SecretKey) -> Result<Option<Zeroizing<String>>, SecretError>;
    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError>;
    /// 刪除秘密；不存在時視為成功。
    fn delete(&self, key: SecretKey) -> Result<(), SecretError>;

    fn is_set(&self, key: SecretKey) -> Result<bool, SecretError> {
        Ok(self.get(key)?.is_some())
    }
}

/// Windows Credential Manager。
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
}

impl Default for KeyringStore {
    fn default() -> Self {
        KeyringStore {
            service: KEYRING_SERVICE.to_string(),
        }
    }
}

impl KeyringStore {
    fn entry(&self, key: SecretKey) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, key.as_str()).map_err(|e| backend(key, &e))
    }
}

/// 只輸出錯誤類型，不帶出任何可能包含秘密位元組的內容。
fn backend(key: SecretKey, err: &keyring::Error) -> SecretError {
    let reason = match err {
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            "憑證內容格式錯誤".to_string()
        }
        keyring::Error::NoStorageAccess(_) => "無法存取憑證庫".to_string(),
        keyring::Error::NoDefaultStore => "此平台沒有可用的憑證庫".to_string(),
        other => format!("{other}"),
    };
    SecretError::Backend { key, reason }
}

impl SecretStore for KeyringStore {
    fn get(&self, key: SecretKey) -> Result<Option<Zeroizing<String>>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(backend(key, &e)),
        }
    }

    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
        if value.is_empty() {
            return Err(SecretError::Empty);
        }
        self.entry(key)?
            .set_password(value)
            .map_err(|e| backend(key, &e))
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(backend(key, &e)),
        }
    }
}

/// 記憶體內的替身，供測試使用。
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<SecretKey, Zeroizing<String>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SecretKey, Zeroizing<String>>> {
        // 持有鎖時不會 panic，故中毒時直接取回內容。
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: SecretKey) -> Result<Option<Zeroizing<String>>, SecretError> {
        Ok(self.lock().get(&key).cloned())
    }

    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
        if value.is_empty() {
            return Err(SecretError::Empty);
        }
        self.lock().insert(key, Zeroizing::new(value.to_string()));
        Ok(())
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        self.lock().remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_settable_accepts_known_keys() {
        for key in SecretKey::USER_SETTABLE {
            assert_eq!(SecretKey::parse_user_settable(key.as_str()).unwrap(), key);
        }
    }

    #[test]
    fn db_key_is_not_user_settable() {
        assert!(matches!(
            SecretKey::parse_user_settable("db_key"),
            Err(SecretError::UnknownKey(_))
        ));
        assert!(SecretKey::parse_user_settable("bogus").is_err());
    }

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryStore::new();
        assert!(!store.is_set(SecretKey::NvdApiKey).unwrap());
        store.set(SecretKey::NvdApiKey, "abc").unwrap();
        assert_eq!(
            store
                .get(SecretKey::NvdApiKey)
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some("abc")
        );
        assert!(store.is_set(SecretKey::NvdApiKey).unwrap());
        store.delete(SecretKey::NvdApiKey).unwrap();
        store.delete(SecretKey::NvdApiKey).unwrap();
        assert!(store.get(SecretKey::NvdApiKey).unwrap().is_none());
    }

    #[test]
    fn empty_secret_is_rejected() {
        let store = MemoryStore::new();
        assert!(matches!(
            store.set(SecretKey::SmtpPassword, ""),
            Err(SecretError::Empty)
        ));
    }

    #[test]
    fn trait_object_injection() {
        fn configured(store: &dyn SecretStore) -> Vec<bool> {
            SecretKey::USER_SETTABLE
                .iter()
                .map(|k| store.is_set(*k).unwrap())
                .collect()
        }
        let store = MemoryStore::new();
        store.set(SecretKey::RedmineApiKey, "k").unwrap();
        assert_eq!(configured(&store), vec![false, false, true]);
    }
}
