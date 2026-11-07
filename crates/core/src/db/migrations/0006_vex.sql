-- 0006_vex：時間軸事件新增操作者 import（CycloneDX VEX 匯入）與來源檔名
--
-- SQLite 無法修改 CHECK 條件，以重建資料表的方式放寬 actor；沒有其他表參照 finding_event。
-- import_file：匯入事件的來源檔名（note 為「<檔名>：<detail>」，匯出時據此還原 detail）。
-- decided_at：匯入事件在來源檔中的判定時間（VEX lastUpdated，缺少時為 metadata.timestamp）；
--   衝突判斷與再匯出的 lastUpdated 以此為準，避免舊判斷因匯入時間而被當成新判斷。

CREATE TABLE finding_event_new (
    id          INTEGER PRIMARY KEY,
    finding_id  INTEGER NOT NULL REFERENCES finding (id) ON DELETE CASCADE,
    at          TEXT    NOT NULL,
    actor       TEXT    NOT NULL CHECK (actor IN ('user', 'system', 'import')),
    from_status TEXT    CHECK (from_status IN ('open', 'resolved', 'ignored')),
    to_status   TEXT    NOT NULL CHECK (to_status IN ('open', 'resolved', 'ignored')),
    note        TEXT    NOT NULL DEFAULT '',
    import_file TEXT,
    decided_at  TEXT,
    CHECK ((actor = 'import') = (import_file IS NOT NULL)),
    CHECK (decided_at IS NULL OR actor = 'import')
);

INSERT INTO finding_event_new (id, finding_id, at, actor, from_status, to_status, note)
    SELECT id, finding_id, at, actor, from_status, to_status, note FROM finding_event;

DROP TABLE finding_event;
ALTER TABLE finding_event_new RENAME TO finding_event;

CREATE INDEX idx_finding_event_finding ON finding_event (finding_id);
