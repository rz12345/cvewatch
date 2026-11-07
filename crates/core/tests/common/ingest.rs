//! ingest 測試共用：wiremock、測試用 HTTP client、目錄、SyncCtx 與 zip 產生器。

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::time::Duration;

use chrono::{DateTime, Utc};
use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::catalog::{Catalog, CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
use cvewatch_core::config::HttpConfig;
use cvewatch_core::http::{HttpClient, RetryPolicy};
use cvewatch_core::ingest::endpoints::Endpoints;
use cvewatch_core::ingest::relevance::{PackageKey, RelevanceIndex};
use cvewatch_core::ingest::source::{NoProgress, SyncCtx, SyncOptions};
use cvewatch_core::paths::DataPaths;
use rusqlite::Connection;
use tempfile::TempDir;
use wiremock::MockServer;
use zip::write::SimpleFileOptions;

pub struct Harness {
    pub server: MockServer,
    pub endpoints: Endpoints,
    pub http: HttpClient,
    pub catalog: Catalog,
    pub product_ids: HashMap<String, i64>,
    pub index: RelevanceIndex,
    pub paths: DataPaths,
    pub _dir: TempDir,
}

/// 重試快速、次數少的 client，避免測試等待。
pub fn fast_http(max_retries: u32) -> HttpClient {
    let policy = RetryPolicy {
        max_retries,
        base_delay: Duration::from_millis(1),
        factor: 1,
        max_retry_after: Duration::from_secs(2),
    };
    HttpClient::with_policy(&HttpConfig::default(), policy).expect("http client")
}

pub fn catalog_with(extra: Option<&str>) -> Catalog {
    let mut sources = vec![CatalogSource {
        name: SEED_SOURCE_NAME.into(),
        text: SEED_TOML.into(),
    }];
    if let Some(text) = extra {
        sources.push(CatalogSource {
            name: "test.toml".into(),
            text: text.into(),
        });
    }
    Catalog::from_sources(&sources).expect("catalog")
}

/// 啟動 wiremock 並建立目錄、索引；目錄同步到 `conn`。
pub async fn harness(conn: &Connection, extra: Option<&str>, packages: Vec<PackageKey>) -> Harness {
    let server = MockServer::start().await;
    let endpoints = Endpoints::with_base(&server.uri());
    let catalog = catalog_with(extra);
    let product_ids = sync_catalog(conn, &catalog).expect("sync catalog");
    let index = RelevanceIndex::new(&catalog, packages);
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = DataPaths::new(dir.path().join("data"));
    Harness {
        server,
        endpoints,
        http: fast_http(1),
        catalog,
        product_ids,
        index,
        paths,
        _dir: dir,
    }
}

impl Harness {
    /// 目錄改變後重建索引（模擬使用者修改 catalog.d）。
    pub fn set_catalog(&mut self, conn: &Connection, extra: Option<&str>) {
        self.catalog = catalog_with(extra);
        self.product_ids = sync_catalog(conn, &self.catalog).expect("sync catalog");
        self.index = RelevanceIndex::new(&self.catalog, Vec::new());
    }

    pub fn ctx<'a>(&'a self, options: &'a SyncOptions, now: DateTime<Utc>) -> SyncCtx<'a> {
        SyncCtx {
            http: &self.http,
            endpoints: &self.endpoints,
            catalog: &self.catalog,
            product_ids: &self.product_ids,
            index: &self.index,
            paths: &self.paths,
            options,
            now,
            progress: &NoProgress,
            nvd_api_key: None,
            nvd_interval: Some(Duration::ZERO),
        }
    }
}

pub fn at(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .expect("time")
        .with_timezone(&Utc)
}

/// delta 形式（一層、無 data descriptor），entry 為 `deltaCves/<id>.json`。
pub fn delta_zip(records: &[Vec<u8>]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (i, rec) in records.iter().enumerate() {
        zip.start_file(format!("deltaCves/{}.json", record_id(rec, i)), deflate())
            .expect("start");
        zip.write_all(rec).expect("write");
    }
    zip.finish().expect("finish").into_inner()
}

/// baseline 形式：外層 `cves.zip`（串流寫入，帶 data descriptor），內層含目錄 entry 與紀錄。
pub fn baseline_zip(records: &[Vec<u8>]) -> Vec<u8> {
    let mut inner = zip::ZipWriter::new(Cursor::new(Vec::new()));
    inner.add_directory("cves/", deflate()).expect("dir");
    for (i, rec) in records.iter().enumerate() {
        let id = record_id(rec, i);
        let year = id.split('-').nth(1).unwrap_or("2099").to_string();
        inner
            .start_file(format!("cves/{year}/xxx/{id}.json"), deflate())
            .expect("start");
        inner.write_all(rec).expect("write");
    }
    let inner = inner.finish().expect("finish").into_inner();
    let mut outer = zip::ZipWriter::new_stream(Vec::new());
    outer.start_file("cves.zip", deflate()).expect("outer");
    outer.write_all(&inner).expect("write");
    outer.finish().expect("finish").into_inner()
}

fn deflate() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn record_id(rec: &[u8], i: usize) -> String {
    serde_json::from_slice::<serde_json::Value>(rec)
        .ok()
        .and_then(|v| v["cveMetadata"]["cveId"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("CVE-2099-{:05}", 90000 + i))
}

/// 修改 fixture 紀錄（例如狀態、dateUpdated）。
pub fn edit_record(bytes: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut v: serde_json::Value = serde_json::from_slice(bytes).expect("json");
    edit(&mut v);
    serde_json::to_vec(&v).expect("json")
}
