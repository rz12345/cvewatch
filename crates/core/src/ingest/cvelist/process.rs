//! 單筆 CVE 紀錄的處理：解析 → 寫搜尋索引 → 關聯性篩選 → 寫入資料庫。

use rusqlite::Connection;

use super::search_index::IndexWriter;
use crate::error::CoreError;
use crate::ingest::cve5;
use crate::ingest::relevance::{Relevance, RelevanceIndex};
use crate::ingest::store::{self, UpsertOutcome};

pub struct Processor<'a> {
    conn: &'a Connection,
    index: &'a RelevanceIndex,
    now: &'a str,
    writer: Option<&'a mut IndexWriter>,
    pub added: u64,
    pub updated: u64,
    /// 格式錯誤而略過的筆數
    pub malformed: u64,
    pub seen: u64,
}

impl<'a> Processor<'a> {
    pub fn new(
        conn: &'a Connection,
        index: &'a RelevanceIndex,
        now: &'a str,
        writer: Option<&'a mut IndexWriter>,
    ) -> Self {
        Processor {
            conn,
            index,
            now,
            writer,
            added: 0,
            updated: 0,
            malformed: 0,
            seen: 0,
        }
    }

    pub fn handle(&mut self, name: &str, bytes: &[u8]) -> Result<(), CoreError> {
        self.seen += 1;
        let record = match cve5::parse(bytes) {
            Ok(r) => r,
            Err(e) => {
                self.malformed += 1;
                tracing::warn!(entry = name, error = %e, "略過無法解析的 CVE 紀錄");
                return Ok(());
            }
        };
        if let Some(writer) = self.writer.as_deref_mut() {
            writer.add(&record)?;
        }
        let verdict = self.index.evaluate(&record);
        let reason = match (verdict, store::existing(self.conn, &record.id)?) {
            (Some(reason), _) => reason,
            // 規則 5：已在本機的 CVE 一律保留，沿用原因
            (None, Some(existing)) => existing.reason,
            (None, None) => return Ok(()),
        };
        self.record(store::upsert_record(
            self.conn, &record, bytes, reason, self.now,
        )?);
        Ok(())
    }

    /// 已知要保留的紀錄（例如補齊先前由 NVD/CSAF 加入的 CVE）。
    pub fn handle_known(&mut self, bytes: &[u8], fallback: Relevance) -> Result<(), CoreError> {
        let record = cve5::parse(bytes)?;
        let reason = self.index.evaluate(&record).unwrap_or(fallback);
        self.record(store::upsert_record(
            self.conn, &record, bytes, reason, self.now,
        )?);
        Ok(())
    }

    fn record(&mut self, outcome: UpsertOutcome) {
        match outcome {
            UpsertOutcome::Inserted => self.added += 1,
            UpsertOutcome::Updated => self.updated += 1,
            UpsertOutcome::Unchanged => {}
        }
    }
}
