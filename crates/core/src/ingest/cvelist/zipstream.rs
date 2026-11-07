//! zip 串流解壓：不落地、不需 seek。
//!
//! - delta：一層 zip，entry 為 `deltaCves/<id>.json`
//! - baseline：外層 zip 只有一個 `cves.zip`（deflate、帶 data descriptor），內層 zip 的 entry
//!   （`cves/<year>/<Nxxx>/<id>.json`）在 local header 有大小，可逐筆串流讀取
//!
//! 下載以 async 進行，解壓在獨立執行緒；兩者以 channel 串接（[`stream_http`]）。

use std::io::{self, Read};
use std::sync::mpsc;
use std::time::Duration;

use flate2::read::DeflateDecoder;

use crate::error::CoreError;
use crate::http::HttpClient;
use crate::ingest::source::Progress;

/// baseline 下載的整體逾時（約 650 MB）。
pub const BASELINE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// 下載與解壓之間最多暫存的區塊數。
const CHUNK_QUEUE: usize = 64;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const LOCAL_HEADER_LEN: usize = 30;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

/// 每筆 entry 的回呼：(entry 名稱, 內容)。
pub type EntryFn<'a> = dyn FnMut(&str, &[u8]) -> Result<(), CoreError> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// 一層 zip（delta）
    Flat,
    /// zip 內只有一個 zip（baseline）
    Nested,
}

fn zip_error(message: impl std::fmt::Display) -> CoreError {
    CoreError::Invalid(format!("zip 格式錯誤：{message}"))
}

/// 只處理 `CVE-*.json` entry。
fn is_record(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    file.starts_with("CVE-") && file.ends_with(".json")
}

/// 逐筆呼叫 `on_entry(entry 名稱, 內容)`，回傳處理的筆數。
pub fn for_each_entry<R: Read>(
    reader: R,
    layout: Layout,
    on_entry: &mut EntryFn<'_>,
) -> Result<u64, CoreError> {
    match layout {
        Layout::Flat => flat(reader, on_entry),
        Layout::Nested => {
            let mut reader = reader;
            let method = skip_local_header(&mut reader)?;
            match method {
                METHOD_DEFLATE => flat(DeflateDecoder::new(reader), on_entry),
                METHOD_STORED => flat(reader, on_entry),
                other => Err(zip_error(format!("不支援的壓縮方式 {other}"))),
            }
        }
    }
}

fn flat<R: Read>(mut reader: R, on_entry: &mut EntryFn<'_>) -> Result<u64, CoreError> {
    let mut count = 0;
    let mut buf = Vec::new();
    while let Some(mut file) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(zip_error)?
    {
        let name = file.name().map_err(zip_error)?.to_string();
        if file.is_dir() || !is_record(&name) {
            continue;
        }
        buf.clear();
        file.read_to_end(&mut buf)
            .map_err(|e| zip_error(format!("{name}：{e}")))?;
        drop(file);
        on_entry(&name, &buf)?;
        count += 1;
    }
    Ok(count)
}

/// 讀過外層第一個 entry 的 local header，回傳壓縮方式；之後的位元組就是 entry 內容。
fn skip_local_header<R: Read>(reader: &mut R) -> Result<u16, CoreError> {
    let mut header = [0u8; LOCAL_HEADER_LEN];
    reader
        .read_exact(&mut header)
        .map_err(|e| zip_error(format!("讀取外層標頭：{e}")))?;
    let u16_at = |i: usize| u16::from_le_bytes([header[i], header[i + 1]]);
    let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if signature != LOCAL_HEADER_SIGNATURE {
        return Err(zip_error("外層不是 zip"));
    }
    let method = u16_at(8);
    let skip = u64::from(u16_at(26)) + u64::from(u16_at(28));
    io::copy(&mut reader.take(skip), &mut io::sink())
        .map_err(|e| zip_error(format!("讀取外層標頭：{e}")))?;
    Ok(method)
}

/// 以 channel 接收區塊的 `Read`；傳送端關閉即 EOF。
struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    current: Vec<u8>,
    pos: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.current.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.current = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.current.len() - self.pos);
        out[..n].copy_from_slice(&self.current[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

type Entry = (String, Vec<u8>);

/// 下載 `url` 並串流解壓，每筆呼叫 `on_entry`。解壓失敗、下載失敗或 `on_entry`
/// 回傳錯誤都會中止並回傳該錯誤。
pub async fn stream_http(
    http: &HttpClient,
    url: &str,
    layout: Layout,
    progress: &dyn Progress,
    on_entry: &mut EntryFn<'_>,
) -> Result<u64, CoreError> {
    let mut resp = http.get_with_timeout(url, BASELINE_TIMEOUT).await?;
    progress.begin("下載並解壓 cvelistV5", resp.content_length());
    let (chunk_tx, chunk_rx) = mpsc::sync_channel::<Vec<u8>>(CHUNK_QUEUE);
    let (entry_tx, entry_rx) = mpsc::channel::<Entry>();
    let parser = std::thread::Builder::new()
        .name("cvelist-unzip".into())
        .spawn(move || {
            let reader = ChannelReader {
                rx: chunk_rx,
                current: Vec::new(),
                pos: 0,
            };
            let mut forward = |name: &str, bytes: &[u8]| {
                entry_tx
                    .send((name.to_string(), bytes.to_vec()))
                    .map_err(|_| CoreError::Invalid("處理已中止".into()))
            };
            for_each_entry(reader, layout, &mut forward)
        })
        .map_err(|e| CoreError::Invalid(format!("無法建立解壓執行緒：{e}")))?;

    let pumped = pump(http, &mut resp, &chunk_tx, &entry_rx, progress, on_entry).await;
    drop(chunk_tx);
    let drained = pumped.and_then(|()| entry_rx.iter().try_for_each(|(n, b)| on_entry(&n, &b)));
    drop(entry_rx);
    let parsed = parser
        .join()
        .map_err(|_| CoreError::Invalid("解壓執行緒異常結束".into()))?;
    progress.end();
    drained?;
    parsed
}

async fn pump(
    http: &HttpClient,
    resp: &mut reqwest::Response,
    chunk_tx: &mpsc::SyncSender<Vec<u8>>,
    entry_rx: &mpsc::Receiver<Entry>,
    progress: &dyn Progress,
    on_entry: &mut EntryFn<'_>,
) -> Result<(), CoreError> {
    while let Some(chunk) = http.chunk(resp).await? {
        progress.advance(chunk.len() as u64);
        if chunk_tx.send(chunk).is_err() {
            // 解壓執行緒已結束（格式錯誤）；錯誤由 join 取得
            return Ok(());
        }
        for (name, bytes) in entry_rx.try_iter() {
            on_entry(&name, &bytes)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_names() {
        assert!(is_record("deltaCves/CVE-2026-1.json"));
        assert!(is_record("cves/2026/1xxx/CVE-2026-1001.json"));
        assert!(!is_record("cves/delta.json"));
        assert!(!is_record("cves/2026/"));
    }

    #[test]
    fn non_zip_outer_is_rejected() {
        let mut seen = |_: &str, _: &[u8]| Ok(());
        let err = for_each_entry(
            &b"not a zip at all, definitely"[..],
            Layout::Nested,
            &mut seen,
        )
        .unwrap_err();
        assert!(err.to_string().contains("zip"), "{err}");
    }
}
