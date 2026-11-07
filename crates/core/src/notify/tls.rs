//! 傳輸安全：機構自簽 CA（`notify.ca_file`）載入，與明文傳輸的檢查（SMTP 與 Redmine 共用）。
//!
//! 憑證一律驗證，沒有略過驗證的選項；`ca_file` 的憑證與系統根憑證並用。

use std::path::Path;

/// PEM 檔中的每張憑證（`-----BEGIN CERTIFICATE-----` 區塊，含頭尾）。
pub fn pem_blocks(pem: &str) -> Vec<String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(BEGIN) {
        let Some(len) = rest[start..].find(END) else {
            break;
        };
        let end = start + len + END.len();
        out.push(format!("{}\n", &rest[start..end]));
        rest = &rest[end..];
    }
    out
}

/// 讀取 CA 檔，回傳 PEM 憑證區塊；檔案不存在、無法讀取或沒有憑證時回傳說明。
pub fn load_ca_file(path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("無法讀取 CA 檔 {}：{e}", path.display()))?;
    let blocks = pem_blocks(&text);
    if blocks.is_empty() {
        return Err(format!(
            "CA 檔 {} 中沒有 PEM 憑證（-----BEGIN CERTIFICATE-----）",
            path.display()
        ));
    }
    Ok(blocks)
}

/// 經明文連線送出秘密時的錯誤訊息。
pub const PLAINTEXT_SECRET_REFUSED_SMTP: &str =
    "明文連線（security = \"none\"）不得傳送 SMTP 密碼；請改用 STARTTLS / TLS，或改為不驗證的內部轉送（清空帳號並刪除 smtp_password）";
pub const PLAINTEXT_SECRET_REFUSED_REDMINE: &str =
    "Redmine 網址為 http（明文），拒絕送出帶 API key 的請求；請改用 https";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_bundle() {
        let pem = "junk\n-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n\
                   -----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";
        let blocks = pem_blocks(pem);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[1].contains("BBB"));
        assert!(pem_blocks("-----BEGIN CERTIFICATE-----\nAAA").is_empty());
    }

    #[test]
    fn missing_or_empty_file_is_explained() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.pem");
        assert!(load_ca_file(&missing).unwrap_err().contains("無法讀取"));
        let empty = dir.path().join("empty.pem");
        std::fs::write(&empty, "nothing").unwrap();
        assert!(load_ca_file(&empty).unwrap_err().contains("沒有 PEM"));
    }
}
