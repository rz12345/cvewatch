-- 0010_lifecycle：資產生命週期日期、EoL 覆寫與 endoflife.date 資料（22 design D6）。
-- 以 ALTER TABLE 加欄，不重建 asset、component 表；既有資產與元件的新欄位為 NULL。
-- 日期為 YYYY-MM-DD 文字；覆寫的日期與來源成對（由 core 的寫入函式保證，不以 CHECK 驗證）。

ALTER TABLE asset ADD COLUMN warranty_end TEXT;
ALTER TABLE asset ADD COLUMN support_end TEXT;
ALTER TABLE asset ADD COLUMN license_end TEXT;
ALTER TABLE asset ADD COLUMN hw_eol_override TEXT;
ALTER TABLE asset ADD COLUMN hw_eol_source TEXT;
ALTER TABLE component ADD COLUMN eol_override TEXT;
ALTER TABLE component ADD COLUMN eol_override_source TEXT;

-- endoflife.date 的產品（一個代稱一列）
CREATE TABLE eol_product (
    slug          TEXT PRIMARY KEY,
    label         TEXT,
    html_link     TEXT,
    status        TEXT NOT NULL CHECK (status IN ('ok', 'not_found')),
    last_modified TEXT,
    fetched_at    TEXT NOT NULL
);

-- endoflife.date 的版本分支（position 為 API 回應中的順序，新到舊）
CREATE TABLE eol_release (
    slug         TEXT    NOT NULL REFERENCES eol_product (slug) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    label        TEXT,
    release_date TEXT,
    is_eol       INTEGER NOT NULL CHECK (is_eol IN (0, 1)),
    eol_from     TEXT,
    eoas_from    TEXT,
    eoes_from    TEXT,
    latest_name  TEXT,
    position     INTEGER NOT NULL,
    PRIMARY KEY (slug, name)
);
