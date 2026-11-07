#![allow(dead_code)]

use std::path::Path;

use cvewatch_core::db::keyops::ensure_database;
use cvewatch_core::paths::DataPaths;
use cvewatch_core::secrets::MemoryStore;
use tempfile::TempDir;

pub struct Env {
    pub dir: TempDir,
    pub paths: DataPaths,
    pub store: MemoryStore,
}

/// 臨時資料目錄 + 記憶體憑證庫，並已建立加密資料庫。
pub fn initialized() -> Env {
    let env = empty();
    ensure_database(&env.paths, &env.store).expect("建立資料庫");
    env
}

pub fn empty() -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = DataPaths::new(dir.path().join("data"));
    std::fs::create_dir_all(&paths.data_dir).expect("data dir");
    Env {
        dir,
        paths,
        store: MemoryStore::new(),
    }
}

/// 檔案內容（含 WAL）是否出現指定字串。
pub fn files_contain(db_path: &Path, needle: &str) -> bool {
    ["", "-wal", "-shm"].iter().any(|suffix| {
        let mut p = db_path.as_os_str().to_owned();
        p.push(suffix);
        std::fs::read(Path::new(&p))
            .map(|bytes| bytes.windows(needle.len()).any(|w| w == needle.as_bytes()))
            .unwrap_or(false)
    })
}
