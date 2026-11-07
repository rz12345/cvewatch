-- 0001_init：schema 版本、應用程式中繼資料、sync 鎖與 sync 執行紀錄

CREATE TABLE schema_version (
    version    INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    applied_at TEXT    NOT NULL
);

-- 鍵值中繼資料，例如 db_key_exported_at
CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 同時間只允許一個 sync；id 固定為 1
CREATE TABLE sync_lock (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    pid         INTEGER NOT NULL,
    acquired_at TEXT    NOT NULL
);

CREATE TABLE sync_run (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at  TEXT    NOT NULL,
    finished_at TEXT,
    source      TEXT    NOT NULL,
    status      TEXT    NOT NULL CHECK (status IN ('running', 'success', 'failed')),
    item_count  INTEGER NOT NULL DEFAULT 0,
    error       TEXT
);

CREATE INDEX idx_sync_run_source_started ON sync_run (source, started_at);
