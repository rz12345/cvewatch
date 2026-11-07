-- 0004_findings：比對命中、證據、處理時間軸、來源矛盾與等待區

-- 元件改為軟刪除：從清冊或套件清單消失時標記 removed_at，不再參與比對，
-- 其命中與處理紀錄保留；同一產品／套件再次出現時清除 removed_at 沿用原元件與命中。
ALTER TABLE component ADD COLUMN removed_at TEXT;

-- 使用中的元件（資產使用中、元件未移除）：比對、同步對象與查詢一律經由此 view，不各自重複條件
CREATE VIEW active_component AS
    SELECT c.id, c.asset_id, c.kind, c.product_id, c.purl, c.purl_full, c.version_raw,
           c.version_norm, c.source, c.updated_at, a.asset_tag, a.exposure, a.criticality
    FROM component c JOIN asset a ON a.id = c.asset_id
    WHERE a.status = 'active' AND c.removed_at IS NULL;

-- 每個（元件 × CVE）一筆。cve_id 不參照 cve 表：OSV 命中的 CVE 可能不在本機。
-- in_range：最近一次比對仍判定受影響（含 probable）。「已處理但仍受影響」= status = resolved AND in_range（查詢時計算）。
-- kev_listed：最近一次比對（或使用者變更狀態）時 CVE 是否已在 KEV，用來判斷「新列入」。
CREATE TABLE finding (
    id               INTEGER PRIMARY KEY,
    component_id     INTEGER NOT NULL REFERENCES component (id) ON DELETE CASCADE,
    cve_id           TEXT    NOT NULL,
    confidence       TEXT    NOT NULL CHECK (confidence IN ('confirmed', 'probable')),
    decided_by       TEXT    NOT NULL
                     CHECK (decided_by IN ('vendor', 'cna', 'nvd', 'adp', 'osv', 'manual')),
    priority         TEXT    NOT NULL CHECK (priority IN ('P1', 'P2', 'P3', 'P4')),
    status           TEXT    NOT NULL DEFAULT 'open'
                     CHECK (status IN ('open', 'resolved', 'ignored')),
    ignore_reason    TEXT    CHECK (ignore_reason IN ('not_affected', 'risk_accepted', 'false_positive')),
    -- ignore_reason = not_affected 時必填，值同 CycloneDX VEX justification
    justification    TEXT    CHECK (justification IN (
                         'code_not_present', 'code_not_reachable', 'requires_configuration',
                         'requires_dependency', 'requires_environment', 'protected_by_compiler',
                         'protected_at_runtime', 'protected_at_perimeter',
                         'protected_by_mitigating_control')),
    first_seen       TEXT    NOT NULL,
    last_evaluated   TEXT    NOT NULL,
    in_range         INTEGER NOT NULL DEFAULT 1,
    kev_listed       INTEGER NOT NULL DEFAULT 0,
    -- 使用者人工判定套用（等待區 --apply）；之後來源資料改寫 decided_by 時仍保留，
    -- 來源不再提到此產品時命中不會被系統自動轉為已處理
    manual           INTEGER NOT NULL DEFAULT 0,
    -- 因等待區人工判定不相關（--dismiss）而忽略；資料之後確定受影響時自動重新開啟，
    -- 使用者手動變更狀態後清除
    dismissed        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (component_id, cve_id),
    CHECK ((status = 'ignored') = (ignore_reason IS NOT NULL)),
    CHECK ((ignore_reason IS 'not_affected') = (justification IS NOT NULL))
);

CREATE INDEX idx_finding_cve ON finding (cve_id);

-- 最近一次比對時各來源的判定（每次重算整批取代）
CREATE TABLE finding_evidence (
    finding_id  INTEGER NOT NULL REFERENCES finding (id) ON DELETE CASCADE,
    source      TEXT    NOT NULL CHECK (source IN ('vendor', 'cna', 'nvd', 'adp', 'osv')),
    verdict     TEXT    NOT NULL CHECK (verdict IN ('affected', 'not_affected', 'unknown')),
    detail_json TEXT    NOT NULL
);

CREATE INDEX idx_finding_evidence_finding ON finding_evidence (finding_id);

-- 狀態時間軸；from_status 為 NULL 表示新命中
CREATE TABLE finding_event (
    id          INTEGER PRIMARY KEY,
    finding_id  INTEGER NOT NULL REFERENCES finding (id) ON DELETE CASCADE,
    at          TEXT    NOT NULL,
    actor       TEXT    NOT NULL CHECK (actor IN ('user', 'system')),
    from_status TEXT    CHECK (from_status IN ('open', 'resolved', 'ignored')),
    to_status   TEXT    NOT NULL CHECK (to_status IN ('open', 'resolved', 'ignored')),
    note        TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_finding_event_finding ON finding_event (finding_id);

-- 最終判定為不受影響、但有其他來源判定受影響（每次重算整批取代 active 元件的列）
CREATE TABLE match_conflict (
    component_id  INTEGER NOT NULL REFERENCES component (id) ON DELETE CASCADE,
    cve_id        TEXT    NOT NULL,
    final_source  TEXT    NOT NULL,
    final_verdict TEXT    NOT NULL,
    other_source  TEXT    NOT NULL,
    other_verdict TEXT    NOT NULL,
    at            TEXT    NOT NULL,
    PRIMARY KEY (component_id, cve_id, other_source)
);

-- 資料不足以判定的 CVE（CVE 層級）。逾期（entered_at 超過 [pending] stale_days）於查詢時計算。
-- resolved_at 非 NULL 表示已移出；
-- 使用者人工移出（manual_apply / manual_dismiss）後不再自動放回。
CREATE TABLE pending (
    cve_id          TEXT    PRIMARY KEY,
    reason          TEXT    NOT NULL
                    CHECK (reason IN ('keyword_only', 'no_version_data', 'vendor_investigating')),
    entered_at      TEXT    NOT NULL,
    last_checked    TEXT    NOT NULL,
    -- 相關的產品 key（JSON 陣列）：產品暫時不在目錄中時，據此延後判斷而不誤判為無關
    products_json   TEXT    NOT NULL DEFAULT '[]',
    -- 人工判定不相關時此 CVE 是否已在本機 KEV（之後才列入者依「新列入 KEV」規則重新開啟）
    kev_at_dismissal INTEGER NOT NULL DEFAULT 0,
    resolved_at     TEXT,
    resolved_reason TEXT    CHECK (resolved_reason IN (
                        'data_affected', 'data_not_affected', 'data_inconclusive', 'no_longer_relevant',
                        'manual_apply', 'manual_dismiss'))
);
