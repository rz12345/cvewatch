//! `cvewatch db export-key | import-key | rekey | backup | restore`

use std::path::Path;

use cvewatch_core::db::backup::{self, BackupCipher, RestoreKey};
use cvewatch_core::db::{keyops, DbKey};
use cvewatch_core::{CoreError, ExitCode};

use crate::app::App;
use crate::prompt;

const EXPORT_CONFIRM: &str = "YES";
const PLAIN_CONFIRM: &str = "PLAIN";
const PLAIN_NAME_MARKER: &str = ".plain";

fn cancelled() -> CoreError {
    CoreError::Invalid("已取消，未變更任何資料".to_string())
}

pub fn export_key(app: &App) -> Result<ExitCode, CoreError> {
    println!("復原金鑰可以解開整個資料庫，請只存入密碼管理器，不要存成檔案或截圖。");
    if !prompt::confirm(
        &format!("輸入 {EXPORT_CONFIRM} 以顯示復原金鑰："),
        EXPORT_CONFIRM,
    )? {
        return Err(cancelled());
    }
    let recovery = keyops::export_recovery_key(&app.paths, &app.store, chrono::Utc::now())?;
    println!();
    println!("{}", recovery.as_str());
    println!();
    println!("已記錄匯出時間。Windows 重灌後可用 `cvewatch db import-key` 輸入此金鑰。");
    Ok(ExitCode::Success)
}

pub fn import_key(app: &App) -> Result<ExitCode, CoreError> {
    let recovery = prompt::secret("輸入復原金鑰（不會顯示）：")?;
    keyops::import_recovery_key(&app.paths, &app.store, &recovery)?;
    println!("復原金鑰正確，已寫入 Windows Credential Manager。");
    Ok(ExitCode::Success)
}

pub fn rekey(app: &App) -> Result<ExitCode, CoreError> {
    keyops::rekey(&app.paths, &app.store)?;
    println!("已以新金鑰重新加密資料庫並更新 Windows Credential Manager，舊的復原金鑰已失效。");
    println!("注意：先前以資料庫金鑰加密的備份，之後須以 `db restore <file> --recovery-key`");
    println!("      輸入舊復原金鑰才能還原；口令備份不受影響。建議現在另做一份新備份。");
    eprintln!("{}", super::EXPORT_REMINDER);
    Ok(ExitCode::Success)
}

pub fn backup(
    app: &App,
    dest: &Path,
    passphrase: bool,
    plain: bool,
) -> Result<ExitCode, CoreError> {
    let cipher = if plain {
        confirm_plain(dest)?;
        BackupCipher::Plain
    } else if passphrase {
        BackupCipher::Passphrase(prompt::new_passphrase()?)
    } else {
        BackupCipher::DatabaseKey
    };
    backup::backup(&app.paths, &app.store, dest, &cipher)?;
    let how = match cipher {
        BackupCipher::DatabaseKey => "以資料庫金鑰加密",
        BackupCipher::Passphrase(_) => "以口令加密",
        BackupCipher::Plain => "未加密",
    };
    println!("備份完成（{how}）：{}", dest.display());
    Ok(ExitCode::Success)
}

fn confirm_plain(dest: &Path) -> Result<(), CoreError> {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !name.contains(PLAIN_NAME_MARKER) {
        eprintln!("警告：明文備份的檔名建議包含 `{PLAIN_NAME_MARKER}`，避免與加密備份混淆。");
    }
    eprintln!("警告：明文備份含所有資產與處理紀錄，任何人取得檔案都能讀取。");
    if !prompt::confirm(
        &format!("輸入 {PLAIN_CONFIRM} 以產生明文備份："),
        PLAIN_CONFIRM,
    )? {
        return Err(CoreError::Invalid("已取消，未產生任何檔案".to_string()));
    }
    Ok(())
}

pub fn restore(
    app: &App,
    src: &Path,
    passphrase: bool,
    recovery_key: bool,
) -> Result<ExitCode, CoreError> {
    let outcome = if passphrase {
        let phrase = prompt::secret("輸入備份口令（不會顯示）：")?;
        backup::restore(
            &app.paths,
            &app.store,
            src,
            &RestoreKey::Passphrase(&phrase),
        )?
    } else if recovery_key {
        let input = prompt::secret("輸入建立備份時的復原金鑰（不會顯示）：")?;
        let key = DbKey::from_recovery_string(&input)?;
        backup::restore(&app.paths, &app.store, src, &RestoreKey::RecoveryKey(&key))?
    } else {
        backup::restore(&app.paths, &app.store, src, &RestoreKey::LocalKey)?
    };
    println!(
        "已還原並以本機資料庫金鑰重新加密：{}",
        app.paths.db_path.display()
    );
    if let Some(saved) = outcome.previous_saved_as {
        println!("原資料庫已保留為：{}", saved.display());
    }
    Ok(ExitCode::Success)
}
