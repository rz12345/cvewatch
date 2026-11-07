//! 原始 JSON 以 zstd 壓縮後存入資料庫。

use crate::error::CoreError;

/// 壓縮等級：3 是 zstd 預設值，速度與壓縮率的平衡點。
pub const ZSTD_LEVEL: i32 = 3;

pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    zstd::encode_all(bytes, ZSTD_LEVEL)
        .map_err(|e| CoreError::Invalid(format!("zstd 壓縮失敗：{e}")))
}

pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    zstd::decode_all(bytes).map_err(|e| CoreError::Invalid(format!("zstd 解壓縮失敗：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = br#"{"cveMetadata":{"cveId":"CVE-2024-3400"}}"#.repeat(20);
        let packed = compress(&data).unwrap();
        assert!(packed.len() < data.len());
        assert_eq!(decompress(&packed).unwrap(), data);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decompress(b"not zstd").is_err());
    }
}
