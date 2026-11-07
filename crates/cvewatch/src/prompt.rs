//! 互動輸入：不回顯的秘密、確認字串。

use std::io::{BufRead, Write};

use cvewatch_core::CoreError;
use zeroize::Zeroizing;

fn input_error(e: std::io::Error) -> CoreError {
    CoreError::Invalid(format!("無法讀取輸入：{e}"))
}

/// 不回顯地讀取一行；空白輸入視為錯誤。
pub fn secret(prompt: &str) -> Result<Zeroizing<String>, CoreError> {
    let value = Zeroizing::new(rpassword::prompt_password(prompt).map_err(input_error)?);
    if value.trim().is_empty() {
        return Err(CoreError::Invalid("輸入不可為空".to_string()));
    }
    Ok(value)
}

/// 輸入兩次口令並確認一致。
pub fn new_passphrase() -> Result<Zeroizing<String>, CoreError> {
    let first = secret("輸入備份口令（不會顯示）：")?;
    let second = secret("再輸入一次：")?;
    if first.as_str() != second.as_str() {
        return Err(CoreError::Invalid("兩次輸入的口令不一致".to_string()));
    }
    Ok(first)
}

/// 要求使用者輸入指定字串；完全相符才回傳 true。
pub fn confirm(prompt: &str, expected: &str) -> Result<bool, CoreError> {
    print!("{prompt}");
    std::io::stdout().flush().map_err(input_error)?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(input_error)?;
    Ok(line.trim() == expected)
}
