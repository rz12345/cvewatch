-- 0005_notify：通知去重（notification_log）、送出佇列（outbox）與 Redmine 議題對應

-- 已排入送出的通知事件；同一管道、同一事件種類、同一命中只有一筆（去重）。
-- 排入 outbox 時即寫入（避免下次 sync 重複排入），送出成功才填 sent_at。
-- kev：通知當時 CVE 是否已在 KEV（之後才列入者產生 BECAME_KEV）。
CREATE TABLE notification_log (
    id          INTEGER PRIMARY KEY,
    channel     TEXT    NOT NULL CHECK (channel IN ('email', 'redmine')),
    event_kind  TEXT    NOT NULL CHECK (event_kind IN ('NEW_URGENT', 'BECAME_KEV')),
    cve_id      TEXT    NOT NULL,
    finding_id  INTEGER REFERENCES finding (id) ON DELETE CASCADE,
    kev         INTEGER NOT NULL DEFAULT 0,
    queued_at   TEXT    NOT NULL,
    sent_at     TEXT,
    outbox_id   INTEGER,
    UNIQUE (channel, event_kind, finding_id)
);

CREATE INDEX idx_notification_log_finding ON notification_log (finding_id);
CREATE INDEX idx_notification_log_outbox ON notification_log (outbox_id);

-- 送出佇列：送出成功即刪除；失敗依退避排程重送，超過上限標記 dead_at（不再重送）。
-- kind：email 為 urgent / digest；redmine 為 create_issue / add_note。
-- claimed_until：某個程序正在送出（認領期限），其他程序在期限前不處理也不刪除。
CREATE TABLE outbox (
    id              INTEGER PRIMARY KEY,
    channel         TEXT    NOT NULL CHECK (channel IN ('email', 'redmine')),
    kind            TEXT    NOT NULL,
    payload_json    TEXT    NOT NULL,
    created_at      TEXT    NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    next_attempt_at TEXT    NOT NULL,
    dead_at         TEXT,
    claimed_until   TEXT
);

-- 一個 CVE 一張 Redmine 議題
CREATE TABLE redmine_issue (
    cve_id     TEXT    PRIMARY KEY,
    issue_id   INTEGER NOT NULL,
    created_at TEXT    NOT NULL
);
