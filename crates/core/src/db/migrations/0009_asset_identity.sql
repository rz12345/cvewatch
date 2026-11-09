-- 0009_asset_identity：資產識別欄位與 IP（20 design D1）。
-- 以 ALTER TABLE 加欄，不重建 asset 表（component、asset_unresolved 以 ON DELETE CASCADE 參照 asset）。
-- 既有資產的 6 欄為 NULL、沒有 IP，不需要轉換資料。

ALTER TABLE asset ADD COLUMN brand TEXT;
ALTER TABLE asset ADD COLUMN model TEXT;
ALTER TABLE asset ADD COLUMN serial_number TEXT;
ALTER TABLE asset ADD COLUMN property_number TEXT;
ALTER TABLE asset ADD COLUMN hostname TEXT;
ALTER TABLE asset ADD COLUMN management_url TEXT;

-- 財產編號不分大小寫唯一（含 retired），與 ISMS 編號相同的作法
CREATE UNIQUE INDEX idx_asset_property_number
    ON asset (property_number COLLATE NOCASE) WHERE property_number IS NOT NULL;

-- 資產的 IP（一筆一個位址）；順序以 id 遞增表示，address 為標準寫法（IpAddr 的 Display）
CREATE TABLE asset_ip (
    id       INTEGER PRIMARY KEY,
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    address  TEXT    NOT NULL,
    purpose  TEXT,
    UNIQUE (asset_id, address)
);
