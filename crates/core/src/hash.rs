//! 指紋用的雜湊（目錄、評分設定、關鍵字等是否變更）。

use sha2::{Digest, Sha256};

/// 文字的 SHA-256，小寫十六進位。
pub(crate) fn sha256_hex(text: &str) -> String {
    data_encoding::HEXLOWER.encode(&Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_empty_text() {
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
