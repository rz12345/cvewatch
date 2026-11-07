-- 0003_cve：本機 CVE 紀錄（只保留相關者）、各來源補充資料、原廠 CSAF 公告與同步游標

CREATE TABLE cve (
    id               TEXT PRIMARY KEY,
    state            TEXT NOT NULL CHECK (state IN ('PUBLISHED', 'REJECTED')),
    data_version     TEXT,
    assigner_short   TEXT,
    date_published   TEXT,
    date_updated     TEXT,
    title            TEXT,
    description_en   TEXT,
    -- cna：CNA/ADP affected 相符；cpe：CPE vendor:product 相符；purl：套件相符；
    -- keyword_only：僅描述關鍵字相符；nvd：由 NVD 查詢加入；csaf：由原廠公告加入
    relevance_reason TEXT NOT NULL
                     CHECK (relevance_reason IN ('cna', 'cpe', 'purl', 'keyword_only', 'nvd', 'csaf')),
    -- 完整 CVE 5.x 紀錄（zstd 壓縮）；由 NVD / CSAF 先加入、尚未取得 CVE.org 紀錄時為 NULL
    raw_cna_json     BLOB,
    -- NVD API 2.0 的 cve 物件（zstd 壓縮）
    raw_nvd_json     BLOB,
    nvd_vuln_status  TEXT,
    nvd_last_modified TEXT,
    updated_at       TEXT NOT NULL
);

CREATE INDEX idx_cve_assigner ON cve (assigner_short);

-- 各來源的 CVSS；source 為 cna / adp / nvd，provider 為 ADP 或 NVD 評分者的 shortName
CREATE TABLE cve_metric (
    cve_id     TEXT NOT NULL REFERENCES cve (id) ON DELETE CASCADE,
    source     TEXT NOT NULL CHECK (source IN ('cna', 'adp', 'nvd')),
    provider   TEXT,
    version    TEXT NOT NULL CHECK (version IN ('2.0', '3.0', '3.1', '4.0')),
    base_score REAL NOT NULL,
    vector     TEXT,
    severity   TEXT
);

CREATE INDEX idx_cve_metric_cve ON cve_metric (cve_id);

-- CNA / ADP 的 affected[]；versions_json、cpes_json 保留原始陣列，判定在 04
CREATE TABLE cve_affected (
    cve_id         TEXT NOT NULL REFERENCES cve (id) ON DELETE CASCADE,
    source         TEXT NOT NULL CHECK (source IN ('cna', 'adp')),
    provider       TEXT,
    vendor_raw     TEXT,
    product_raw    TEXT,
    vendor_norm    TEXT NOT NULL,
    product_norm   TEXT NOT NULL,
    default_status TEXT,
    versions_json  TEXT NOT NULL,
    cpes_json      TEXT NOT NULL,
    purl           TEXT
);

CREATE INDEX idx_cve_affected_cve ON cve_affected (cve_id);
CREATE INDEX idx_cve_affected_names ON cve_affected (vendor_norm, product_norm);

-- NVD configurations 的 cpeMatch；config_group 為 configurations 的索引，node_group 為 nodes 的索引
CREATE TABLE cve_nvd_match (
    cve_id         TEXT    NOT NULL REFERENCES cve (id) ON DELETE CASCADE,
    config_group   INTEGER NOT NULL,
    config_operator TEXT,
    node_group     INTEGER NOT NULL,
    node_operator  TEXT,
    node_negate    INTEGER NOT NULL DEFAULT 0,
    criteria_cpe   TEXT    NOT NULL,
    vuln           INTEGER NOT NULL,
    ver_start_incl TEXT,
    ver_start_excl TEXT,
    ver_end_incl   TEXT,
    ver_end_excl   TEXT
);

CREATE INDEX idx_cve_nvd_match_cve ON cve_nvd_match (cve_id);

-- CISA ADP（Vulnrichment）的 SSVC
CREATE TABLE cve_ssvc (
    cve_id           TEXT PRIMARY KEY REFERENCES cve (id) ON DELETE CASCADE,
    provider         TEXT,
    exploitation     TEXT,
    automatable      TEXT,
    technical_impact TEXT
);

-- 完整 KEV 清單（含本機尚無的 CVE，之後變成相關時即可使用）
CREATE TABLE kev (
    cve_id             TEXT PRIMARY KEY,
    vendor             TEXT,
    product            TEXT,
    vulnerability_name TEXT,
    date_added         TEXT NOT NULL,
    due_date           TEXT,
    known_ransomware   INTEGER NOT NULL DEFAULT 0,
    required_action    TEXT
);

CREATE TABLE epss (
    cve_id     TEXT PRIMARY KEY,
    score      REAL NOT NULL,
    percentile REAL NOT NULL,
    score_date TEXT NOT NULL,
    -- 本機查詢日期（UTC，YYYY-MM-DD）；同一天不重查
    fetched_on TEXT NOT NULL
);

CREATE TABLE osv_hit (
    component_id        INTEGER NOT NULL REFERENCES component (id) ON DELETE CASCADE,
    osv_id              TEXT    NOT NULL,
    modified            TEXT,
    cve_ids_json        TEXT    NOT NULL,
    fixed_versions_json TEXT    NOT NULL,
    queried_at          TEXT    NOT NULL,
    PRIMARY KEY (component_id, osv_id)
);

-- OSV 漏洞詳細資料快取（modified 未變時不重抓）
CREATE TABLE osv_vuln (
    osv_id    TEXT PRIMARY KEY,
    modified  TEXT,
    raw       BLOB NOT NULL,
    fetched_at TEXT NOT NULL
);

CREATE TABLE csaf_document (
    provider    TEXT NOT NULL,
    tracking_id TEXT NOT NULL,
    version     TEXT NOT NULL,
    title       TEXT,
    tlp         TEXT,
    published   TEXT,
    updated     TEXT,
    url         TEXT NOT NULL,
    sha_ok      INTEGER,
    raw         BLOB NOT NULL,
    fetched_at  TEXT NOT NULL,
    PRIMARY KEY (provider, tracking_id)
);

-- kind：exact（單一版本）或 range（vers: 字串）；version_expr 為版本或標準化後的 vers 字串
CREATE TABLE csaf_status (
    provider           TEXT    NOT NULL,
    tracking_id        TEXT    NOT NULL,
    cve_id             TEXT    NOT NULL,
    catalog_product_id INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    csaf_product_id    TEXT    NOT NULL,
    version_expr       TEXT    NOT NULL,
    kind               TEXT    NOT NULL CHECK (kind IN ('exact', 'range')),
    status             TEXT    NOT NULL
                       CHECK (status IN ('affected', 'not_affected', 'fixed', 'under_investigation')),
    justification      TEXT,
    FOREIGN KEY (provider, tracking_id) REFERENCES csaf_document (provider, tracking_id) ON DELETE CASCADE
);

CREATE INDEX idx_csaf_status_cve ON csaf_status (cve_id);

CREATE TABLE csaf_fix (
    provider           TEXT    NOT NULL,
    tracking_id        TEXT    NOT NULL,
    cve_id             TEXT    NOT NULL,
    catalog_product_id INTEGER NOT NULL REFERENCES catalog_product (id) ON DELETE CASCADE,
    fixed_version      TEXT,
    details            TEXT,
    url                TEXT,
    FOREIGN KEY (provider, tracking_id) REFERENCES csaf_document (provider, tracking_id) ON DELETE CASCADE
);

CREATE INDEX idx_csaf_fix_cve ON csaf_fix (cve_id);

-- 每個來源的游標（JSON），例如 cvelist、nvd、kev、csaf:paloalto
CREATE TABLE sync_cursor (
    source      TEXT PRIMARY KEY,
    cursor_json TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- sync_run 補上新增與更新筆數（item_count 保留為兩者之和）
ALTER TABLE sync_run ADD COLUMN added_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sync_run ADD COLUMN updated_count INTEGER NOT NULL DEFAULT 0;
