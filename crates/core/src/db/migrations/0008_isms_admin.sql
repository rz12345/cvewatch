-- 0008_isms_admin：資產說明（ISMS 系統／服務）與 ISMS 管理欄位（16 design D1、D6、D7）。
-- 不重建 asset 表（component、asset_unresolved 以 ON DELETE CASCADE 參照 asset）。
-- owner 欄保留在資料表、程式不再讀寫（改用 custodian），與 0007 保留 criticality 相同。

-- 資產說明：代碼建立後不可修改，故以代碼作為外鍵；代碼與名稱都不分大小寫唯一
CREATE TABLE asset_system (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_asset_system_code ON asset_system (code COLLATE NOCASE);
CREATE UNIQUE INDEX idx_asset_system_name ON asset_system (name COLLATE NOCASE);

ALTER TABLE asset ADD COLUMN system_code TEXT REFERENCES asset_system (code);
ALTER TABLE asset ADD COLUMN responsible_unit TEXT;
ALTER TABLE asset ADD COLUMN custodian TEXT;
ALTER TABLE asset ADD COLUMN user_unit TEXT;
ALTER TABLE asset ADD COLUMN compliance INTEGER CHECK (compliance BETWEEN 1 AND 4);
ALTER TABLE asset ADD COLUMN personal_data INTEGER NOT NULL DEFAULT 0
    CHECK (personal_data IN (0, 1));

CREATE INDEX idx_asset_system_code_ref ON asset (system_code);

-- owner 轉為保管人
UPDATE asset SET custodian = owner;

-- 依既有 ISMS 編號的系統代碼建立資產說明（名稱暫時等於代碼）並歸屬
INSERT OR IGNORE INTO asset_system (code, name)
    SELECT DISTINCT upper(substr(isms_id, 1, 3)), upper(substr(isms_id, 1, 3))
    FROM asset WHERE isms_id IS NOT NULL;
UPDATE asset SET system_code = upper(substr(isms_id, 1, 3)) WHERE isms_id IS NOT NULL;
