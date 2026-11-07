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

/// 記憶體資料庫（未加密）並套用全部 migration；給不需測試加密的 ingest 測試使用。
pub fn memory_db() -> rusqlite::Connection {
    let mut conn = rusqlite::Connection::open_in_memory().expect("memory db");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
    cvewatch_core::db::migrate::migrate(&mut conn).expect("migrate");
    conn
}

/// `tests/fixtures/ingest/<rel>` 的完整路徑。
pub fn ingest_fixture(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ingest")
        .join(rel)
}

/// 讀取 ingest fixture 的位元組。
pub fn read_ingest(rel: &str) -> Vec<u8> {
    let path = ingest_fixture(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀取 {}：{e}", path.display()))
}
pub mod ingest;
pub mod matching;
