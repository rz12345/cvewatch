//! 目錄的軟體 EoL 宣告：endoflife.date 代稱與版本分支 EoL（22 design D4）。

use chrono::NaiveDate;

use super::catalog_error;
use super::file::{EolCycleDef, ProductDef};
use crate::assets::dates::parse_iso_date;
use crate::error::CoreError;

/// 目錄宣告的一個版本分支 EoL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EolCycle {
    /// 分支名稱（例如 `7.1`）
    pub cycle: String,
    pub eol: NaiveDate,
    /// 原廠公告網址
    pub source: String,
}

/// 代稱格式：小寫英數字與 `-`（空字串另表示取消，不經此檢查）。
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `http://` 或 `https://` 開頭、後面還有內容的網址。
pub fn is_http_url(url: &str) -> bool {
    let url = url.trim();
    ["http://", "https://"]
        .iter()
        // `get` 而非切片：非 ASCII 文字（覆寫來源「延長支援合約」）的位元組邊界不一定落在前綴長度
        .any(|p| {
            url.len() > p.len()
                && url
                    .get(..p.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(p))
        })
}

/// 逐來源驗證代稱與分支宣告，讓錯誤指向實際寫錯的檔案。
pub(super) fn validate(def: &ProductDef, origin: &str) -> Result<(), CoreError> {
    let key = def.key.trim();
    if let Some(slug) = def.endoflife.as_deref() {
        let slug = slug.trim();
        if !slug.is_empty() && !is_valid_slug(slug) {
            return Err(catalog_error(
                origin,
                format!(
                    "產品 `{key}` 的 endoflife 代稱「{slug}」格式不符（只能包含小寫英數字與 -）"
                ),
            ));
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    for def in def.eol_cycles.iter().flatten() {
        let cycle = def.cycle.trim();
        if seen.contains(&cycle) {
            return Err(catalog_error(
                origin,
                format!("產品 `{key}` 的分支 `{cycle}` 重複宣告"),
            ));
        }
        seen.push(cycle);
        build_cycle(def, key, origin)?;
    }
    Ok(())
}

/// 解析一筆分支宣告：分支名稱、`YYYY-MM-DD` 日期與 http(s) 來源網址都必須有。
pub(super) fn build_cycle(
    def: &EolCycleDef,
    key: &str,
    origin: &str,
) -> Result<EolCycle, CoreError> {
    let cycle = def.cycle.trim();
    if cycle.is_empty() {
        return Err(catalog_error(
            origin,
            format!("產品 `{key}` 有分支 EoL 缺少分支名稱（cycle）"),
        ));
    }
    let fail =
        |message: &str| catalog_error(origin, format!("產品 `{key}` 的分支 `{cycle}` {message}"));
    let eol_text = def.eol.as_deref().map(str::trim).unwrap_or_default();
    if eol_text.is_empty() {
        return Err(fail("缺少 EoL 日期（eol）"));
    }
    let eol = parse_iso_date(eol_text)
        .ok_or_else(|| fail(&format!("的 EoL 日期「{eol_text}」不是 YYYY-MM-DD")))?;
    let source = def.source.as_deref().map(str::trim).unwrap_or_default();
    if source.is_empty() {
        return Err(fail("缺少來源網址（source）"));
    }
    if !is_http_url(source) {
        return Err(fail(&format!("的來源「{source}」不是 http 或 https 網址")));
    }
    Ok(EolCycle {
        cycle: cycle.to_string(),
        eol,
        source: source.to_string(),
    })
}

/// 依 `cycle` 合併：相同分支以後者為準、新分支加入（22 design D4）。
pub(super) fn merge_cycles(
    existing: Option<Vec<EolCycleDef>>,
    incoming: Option<Vec<EolCycleDef>>,
) -> Option<Vec<EolCycleDef>> {
    let Some(incoming) = incoming else {
        return existing;
    };
    let mut merged = existing.unwrap_or_default();
    for def in incoming {
        match merged
            .iter_mut()
            .find(|d| d.cycle.trim() == def.cycle.trim())
        {
            Some(slot) => *slot = def,
            None => merged.push(def),
        }
    }
    Some(merged)
}

/// 合併後的代稱：空字串為取消。
pub(super) fn slug(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_format() {
        assert!(is_valid_slug("panos"));
        assert!(is_valid_slug("bind-9"));
        assert!(!is_valid_slug("Proxmox VE"));
        assert!(!is_valid_slug("proxmox_ve"));
        assert!(!is_valid_slug(""));
    }

    #[test]
    fn url_must_be_http_or_https() {
        assert!(is_http_url("https://vendor.example/eol"));
        assert!(is_http_url("HTTP://vendor.example"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("ftp://vendor.example"));
        assert!(!is_http_url("vendor.example"));
        assert!(!is_http_url("延長支援合約"), "非 ASCII 文字不會 panic");
    }
}
