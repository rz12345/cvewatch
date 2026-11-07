//! ESXi / vCenter 版本名稱的文法（`8.0 U3c`、`8.0 Update 3c`、`8.0u3 P01`、`7.0 U3 EP 1`、`8.0c`、
//! `ESXi80U3d-24585383`）。目錄的 build 對照表驗證與 `esxi` 版本比較器共用同一套文法。

use std::sync::LazyLock;

use regex::Regex;

use super::canonical;

static RELEASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(\d+)\.(\d+)(?:\.(\d+))?([a-z])?",
        r"(?: ?(?:u|update) ?(\d+)([a-z])?)?",
        r"(?: ?(?:p|patch) ?(\d+))?",
        r"(?: ?(?:ep|express patch) ?(\d+))?",
        r"(?:[ ,]*\(?build[- ]?(\d{6,9})\)?)?$",
    ))
    .expect("固定的正規表示式必定可編譯")
});

/// `u3ep1`、`8.0ep01`：數字後緊接的 `ep` 可讀成 Express Patch，也可讀成 Update 字母 e + Patch
/// （Broadcom 也有 `U3sP01` 這種字母後緊接 Patch 的寫法），無法判斷 → 視為無法解析（判定為未知）。
/// 有空白分隔時沒有歧義：`U3 EP 1`、`U3e P01`。
static AMBIGUOUS_EP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\dep ?\d").expect("固定的正規表示式必定可編譯"));

/// Broadcom CVE 紀錄的精簡寫法：`ESXi80U3d-24585383`、`ESXi70U3sP01`（主、次版本各一位數）。
static COMPACT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^esxi(\d)(\d)(?:u(\d+)([a-z])?)?(?:p(\d+))?(?:-(\d{6,9}))?$")
        .expect("固定的正規表示式必定可編譯")
});

/// 名稱形式的版本；`letter` 以 a=1、b=2… 表示，沒有字母為 0。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Release {
    pub major: u64,
    pub minor: u64,
    pub update: u64,
    pub letter: u64,
    pub patch: u64,
    pub ep: u64,
}

fn letter_rank(m: Option<regex::Match<'_>>) -> u64 {
    m.and_then(|m| m.as_str().bytes().next())
        .map_or(0, |b| u64::from(b - b'a') + 1)
}

/// 擷取群組的數字；沒有該群組為 `Ok(None)`，數字溢位為 `Err`（整個版本視為無法解析）。
fn group(caps: &regex::Captures<'_>, i: usize) -> Result<Option<u64>, std::num::ParseIntError> {
    caps.get(i).map(|m| m.as_str().parse::<u64>()).transpose()
}

/// 名稱解析結果：(版本, build, 字母未表達)。`8.0.3` 這種數字寫法無法表達 Update 字母。
pub type Named = (Release, Option<u64>, bool);

fn parse_compact(text: &str) -> Option<Named> {
    let caps = COMPACT.captures(text)?;
    let num = |i: usize| group(&caps, i).ok();
    let release = Release {
        major: num(1)??,
        minor: num(2)??,
        update: num(3)?.unwrap_or(0),
        letter: letter_rank(caps.get(4)),
        patch: num(5)?.unwrap_or(0),
        ep: 0,
    };
    Some((release, num(6)?, false))
}

pub fn parse_named(text: &str) -> Option<Named> {
    if let Some(compact) = parse_compact(text) {
        return Some(compact);
    }
    if is_ambiguous(text) {
        return None;
    }
    let caps = RELEASE.captures(text)?;
    let num = |i: usize| group(&caps, i).ok();
    // `8.0.3 U3` 這類重複寫法以 Update 名稱為準
    let update = num(5)?.or(num(3)?).unwrap_or(0);
    let letter = letter_rank(caps.get(6).or(caps.get(4)));
    // 以第三段數字表示 Update（沒有 U/Update 寫法）且沒有字母：實際字母未知
    let unspecified = caps.get(5).is_none() && caps.get(3).is_some() && letter == 0;
    let release = Release {
        major: num(1)??,
        minor: num(2)??,
        update,
        letter,
        patch: num(7)?.unwrap_or(0),
        ep: num(8)?.unwrap_or(0),
    };
    Some((release, num(9)?, unspecified))
}

/// 名稱形式（不接受純 build），供目錄 build 對照表驗證。
pub fn parse_release(raw: &str) -> Option<Release> {
    parse_named(&canonical(raw)).map(|(r, _, _)| r)
}

/// 數字後緊接 `ep` 的歧義寫法（`8.0u3ep1`）：比較器不解析，清冊驗證也不接受。
pub fn is_ambiguous(text: &str) -> bool {
    AMBIGUOUS_EP.is_match(text)
}

/// 目錄 build 對照表可用的名稱：可解析、沒有附 build，且寫明 Update（不接受 `8.0.3` 這種無法表達字母的寫法）。
pub fn is_release_name(raw: &str) -> bool {
    matches!(parse_named(&canonical(raw)), Some((_, None, false)))
}
