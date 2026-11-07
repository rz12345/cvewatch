-- 0002_assets：資產、元件、版本歷史與產品目錄

CREATE TABLE catalog_product (
    id              INTEGER PRIMARY KEY,
    key             TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL,
    vendor_display  TEXT NOT NULL,
    version_scheme  TEXT NOT NULL,
    advisory_source TEXT
);

-- 正規化後（NFKC、小寫、去空白與 -_./）的別名，全域唯一
CREATE TABLE catalog_alias (
    product_id INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    alias_norm TEXT    NOT NULL UNIQUE
);

CREATE TABLE catalog_cpe (
    product_id  INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    cpe_part    TEXT    NOT NULL CHECK (cpe_part IN ('a', 'o', 'h')),
    cpe_vendor  TEXT    NOT NULL,
    cpe_product TEXT    NOT NULL
);

-- CVE 5.x affected[].vendor / product（正規化後）
CREATE TABLE catalog_cna (
    product_id       INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    cna_vendor_norm  TEXT    NOT NULL,
    cna_product_norm TEXT    NOT NULL
);

-- CSAF product_tree 的 vendor / product_name（正規化後）
CREATE TABLE catalog_csaf (
    product_id        INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    csaf_vendor_norm  TEXT    NOT NULL,
    csaf_product_norm TEXT    NOT NULL
);

CREATE TABLE asset (
    id          INTEGER PRIMARY KEY,
    asset_tag   TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    category    TEXT    NOT NULL,
    location    TEXT,
    owner       TEXT,
    exposure    TEXT    NOT NULL DEFAULT 'internal'
                CHECK (exposure IN ('internet', 'dmz', 'internal', 'mgmt')),
    criticality INTEGER NOT NULL DEFAULT 2 CHECK (criticality IN (1, 2, 3)),
    notes       TEXT,
    status      TEXT    NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    first_seen  TEXT    NOT NULL,
    last_seen   TEXT    NOT NULL,
    retired_at  TEXT
);

-- kind=product：product_id 必填；kind=package：purl（不含版本的識別）必填
CREATE TABLE component (
    id           INTEGER PRIMARY KEY,
    asset_id     INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL CHECK (kind IN ('product', 'package')),
    product_id   INTEGER REFERENCES catalog_product (id),
    purl         TEXT,
    purl_full    TEXT,
    version_raw  TEXT    NOT NULL,
    version_norm TEXT,
    source       TEXT    NOT NULL CHECK (source IN ('excel', 'sbom', 'dpkg', 'rpm')),
    updated_at   TEXT    NOT NULL,
    CHECK ((kind = 'product' AND product_id IS NOT NULL AND purl IS NULL)
        OR (kind = 'package' AND purl IS NOT NULL AND product_id IS NULL)),
    UNIQUE (asset_id, product_id),
    UNIQUE (asset_id, purl)
);

CREATE INDEX idx_component_product ON component (product_id);

-- 版本變更前的舊版本
CREATE TABLE component_history (
    id           INTEGER PRIMARY KEY,
    component_id INTEGER NOT NULL REFERENCES component (id) ON DELETE CASCADE,
    version_raw  TEXT    NOT NULL,
    changed_at   TEXT    NOT NULL
);

CREATE INDEX idx_component_history_component ON component_history (component_id);

-- 清冊中無法辨識的產品寫法；每次匯入該資產時整批取代
CREATE TABLE asset_unresolved (
    id          INTEGER PRIMARY KEY,
    asset_id    INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    product_raw TEXT    NOT NULL,
    version_raw TEXT    NOT NULL,
    seen_at     TEXT    NOT NULL
);
