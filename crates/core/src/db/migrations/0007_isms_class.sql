-- 0007_isms_class：資產類別改為 ISMS 子類別代碼、以 C/I/A 取代重要性、新增 ISMS 資產編號
-- （13 design D7）。不重建 asset 表：component、asset_unresolved 以 ON DELETE CASCADE 參照 asset，
-- 在交易內 DROP TABLE 會連帶刪除元件。criticality 欄保留（NOT NULL DEFAULT 2），程式不再讀寫。

ALTER TABLE asset ADD COLUMN confidentiality INTEGER NOT NULL DEFAULT 2
    CHECK (confidentiality BETWEEN 1 AND 4);
ALTER TABLE asset ADD COLUMN integrity INTEGER NOT NULL DEFAULT 2
    CHECK (integrity BETWEEN 1 AND 4);
ALTER TABLE asset ADD COLUMN availability INTEGER NOT NULL DEFAULT 2
    CHECK (availability BETWEEN 1 AND 4);
ALTER TABLE asset ADD COLUMN isms_id TEXT;

-- 依舊值設定 C/I/A：轉換後子類別（HW01、HW02、HW03、HW05）的內建範本值
UPDATE asset SET (confidentiality, integrity, availability) = (
    CASE category WHEN 'network' THEN 2 WHEN 'security' THEN 2 WHEN 'virtualization' THEN 2
                  WHEN 'server' THEN 2 WHEN 'storage' THEN 2 WHEN 'host' THEN 2
                  ELSE confidentiality END,
    CASE category WHEN 'network' THEN 3 WHEN 'security' THEN 3 WHEN 'virtualization' THEN 3
                  WHEN 'server' THEN 3 WHEN 'storage' THEN 3 WHEN 'host' THEN 3
                  ELSE integrity END,
    CASE category WHEN 'network' THEN 4 WHEN 'security' THEN 4 WHEN 'virtualization' THEN 4
                  WHEN 'server' THEN 4 WHEN 'storage' THEN 4 WHEN 'host' THEN 4
                  ELSE availability END
);

-- 附記舊重要性，供使用者比對轉換前後的價值
UPDATE asset SET notes = CASE
    WHEN notes IS NULL OR trim(notes) = '' THEN '舊重要性：' || criticality
    ELSE notes || '；舊重要性：' || criticality
END;

-- 轉換類別（不在對照表中的值維持原樣）
UPDATE asset SET category = CASE category
    WHEN 'network' THEN 'HW02'
    WHEN 'security' THEN 'HW03'
    WHEN 'virtualization' THEN 'HW01'
    WHEN 'server' THEN 'HW01'
    WHEN 'storage' THEN 'HW05'
    WHEN 'host' THEN 'HW01'
    ELSE category
END;

-- ISMS 編號不分大小寫唯一（core 寫入前先查詢並回報使用者，此 index 為最後防線）
CREATE UNIQUE INDEX idx_asset_isms_id ON asset (isms_id COLLATE NOCASE) WHERE isms_id IS NOT NULL;

-- 以資產價值取代重要性
DROP VIEW active_component;
CREATE VIEW active_component AS
    SELECT c.id, c.asset_id, c.kind, c.product_id, c.purl, c.purl_full, c.version_raw,
           c.version_norm, c.source, c.updated_at, a.asset_tag, a.exposure,
           max(a.confidentiality, a.integrity, a.availability) AS asset_value
    FROM component c JOIN asset a ON a.id = c.asset_id
    WHERE a.status = 'active' AND c.removed_at IS NULL;
