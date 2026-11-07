//! baseline 搜尋索引：每筆 CVE（不論是否相關）一行精簡 JSON，zstd 壓縮。
//!
//! baseline 時重建整檔（先寫暫存檔再改名），delta 時以新的 zstd frame 附加。
//! `cve search` 讀取全部 frame，相同 id 以後出現者為準。

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::ingest::cve5::CveRecord;

pub const INDEX_FILE_NAME: &str = "search-index.jsonl.zst";
const TEMP_SUFFIX: &str = ".tmp";
const DELTA_TEMP_SUFFIX: &str = ".delta.tmp";
/// 每筆保留的描述長度（字元）。
pub const DESCRIPTION_CHARS: usize = 300;
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub state: String,
    pub assigner: Option<String>,
    /// `Vendor / Product`
    pub products: Vec<String>,
    pub description: String,
}

impl IndexEntry {
    pub fn from_record(rec: &CveRecord) -> Self {
        let mut products: Vec<String> = rec
            .cna
            .affected
            .iter()
            .map(|a| {
                format!(
                    "{} / {}",
                    a.vendor.as_deref().unwrap_or("?"),
                    a.product.as_deref().unwrap_or("?")
                )
            })
            .collect();
        products.dedup();
        IndexEntry {
            id: rec.id.clone(),
            state: rec.state.as_str().to_string(),
            assigner: rec.assigner_short.clone(),
            products,
            description: rec
                .description_en
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(DESCRIPTION_CHARS)
                .collect(),
        }
    }

    fn matches(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle)
            || self.description.to_lowercase().contains(needle)
            || self
                .products
                .iter()
                .any(|p| p.to_lowercase().contains(needle))
    }
}

pub fn index_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(INDEX_FILE_NAME)
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> CoreError + '_ {
    move |e| CoreError::io(path, e)
}

/// 暫存檔完成後如何併入索引。
enum Commit {
    /// baseline 重建：暫存檔改名為索引
    Rename(PathBuf),
    /// delta：暫存檔內容（一個完整 frame）附加到索引尾端
    AppendTo(PathBuf),
}

/// 寫入一個 zstd frame 到暫存檔；`finish` 後才併入索引，中途失敗不會留下不完整的 frame。
pub struct IndexWriter {
    encoder: zstd::Encoder<'static, BufWriter<File>>,
    path: PathBuf,
    commit: Commit,
}

impl IndexWriter {
    /// 重建：寫到暫存檔。
    pub fn rebuild(cache_dir: &Path) -> Result<Self, CoreError> {
        let target = index_path(cache_dir);
        let temp = PathBuf::from(format!("{}{TEMP_SUFFIX}", target.display()));
        Self::open(cache_dir, temp, Commit::Rename(target))
    }

    /// 附加新的 frame（先寫暫存檔）。
    pub fn append(cache_dir: &Path) -> Result<Self, CoreError> {
        let target = index_path(cache_dir);
        let temp = PathBuf::from(format!("{}{DELTA_TEMP_SUFFIX}", target.display()));
        Self::open(cache_dir, temp, Commit::AppendTo(target))
    }

    fn open(cache_dir: &Path, path: PathBuf, commit: Commit) -> Result<Self, CoreError> {
        std::fs::create_dir_all(cache_dir).map_err(io_err(cache_dir))?;
        let file = File::create(&path).map_err(io_err(&path))?;
        let encoder =
            zstd::Encoder::new(BufWriter::new(file), ZSTD_LEVEL).map_err(io_err(&path))?;
        Ok(IndexWriter {
            encoder,
            path,
            commit,
        })
    }

    pub fn add(&mut self, rec: &CveRecord) -> Result<(), CoreError> {
        let line = serde_json::to_string(&IndexEntry::from_record(rec))
            .map_err(|e| CoreError::Invalid(format!("索引序列化失敗：{e}")))?;
        writeln!(self.encoder, "{line}").map_err(io_err(&self.path))
    }

    pub fn finish(self) -> Result<(), CoreError> {
        let path = self.path;
        let mut writer = self.encoder.finish().map_err(io_err(&path))?;
        writer.flush().map_err(io_err(&path))?;
        drop(writer);
        match self.commit {
            Commit::Rename(target) => std::fs::rename(&path, &target).map_err(io_err(&target))?,
            Commit::AppendTo(target) => {
                let frame = std::fs::read(&path).map_err(io_err(&path))?;
                let mut index = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&target)
                    .map_err(io_err(&target))?;
                index.write_all(&frame).map_err(io_err(&target))?;
                std::fs::remove_file(&path).map_err(io_err(&path))?;
            }
        }
        Ok(())
    }
}

/// 搜尋索引；檔案不存在時為 None。結果依 CVE ID 排序。
pub fn search(cache_dir: &Path, keyword: &str) -> Result<Option<Vec<IndexEntry>>, CoreError> {
    let path = index_path(cache_dir);
    if !path.exists() {
        return Ok(None);
    }
    let needle = keyword.trim().to_lowercase();
    let file = File::open(&path).map_err(io_err(&path))?;
    let decoder = zstd::Decoder::new(file).map_err(io_err(&path))?;
    let mut hits: BTreeMap<String, IndexEntry> = BTreeMap::new();
    for line in BufReader::new(decoder).lines() {
        let line = line.map_err(io_err(&path))?;
        let Ok(entry) = serde_json::from_str::<IndexEntry>(&line) else {
            continue;
        };
        if entry.matches(&needle) {
            hits.insert(entry.id.clone(), entry);
        } else {
            // 較新的版本不再相符
            hits.remove(&entry.id);
        }
    }
    Ok(Some(hits.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::cve5;

    fn rec(id: &str, desc: &str) -> CveRecord {
        let json = format!(
            r#"{{"cveMetadata":{{"cveId":"{id}","state":"PUBLISHED"}},
               "containers":{{"cna":{{"descriptions":[{{"lang":"en","value":"{desc}"}}],
               "affected":[{{"vendor":"Acme","product":"Widget"}}]}}}}}}"#
        );
        cve5::parse(json.as_bytes()).unwrap()
    }

    #[test]
    fn rebuild_append_and_search() {
        let dir = tempfile::tempdir().unwrap();
        assert!(search(dir.path(), "x").unwrap().is_none());
        let mut w = IndexWriter::rebuild(dir.path()).unwrap();
        w.add(&rec("CVE-2099-1", "flaw in Proxmox Backup")).unwrap();
        w.add(&rec("CVE-2099-2", "unrelated")).unwrap();
        w.finish().unwrap();
        let mut w = IndexWriter::append(dir.path()).unwrap();
        w.add(&rec("CVE-2099-2", "now mentions proxmox")).unwrap();
        w.add(&rec("CVE-2099-1", "rewritten text")).unwrap();
        w.finish().unwrap();
        let hits = search(dir.path(), "PROXMOX").unwrap().unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["CVE-2099-2"]);
        let by_product = search(dir.path(), "acme / widget").unwrap().unwrap();
        assert_eq!(by_product.len(), 2);
        assert!(!dir
            .path()
            .join(format!("{INDEX_FILE_NAME}{TEMP_SUFFIX}"))
            .exists());
        assert!(!dir
            .path()
            .join(format!("{INDEX_FILE_NAME}{DELTA_TEMP_SUFFIX}"))
            .exists());
        // 未 finish 的 delta（例如交易失敗）不影響索引
        let mut w = IndexWriter::append(dir.path()).unwrap();
        w.add(&rec("CVE-2099-3", "proxmox again")).unwrap();
        drop(w);
        let hits = search(dir.path(), "proxmox").unwrap().unwrap();
        assert_eq!(hits.len(), 1);
    }
}
