//! 本機保存的 endoflife.date 資料（`eol_product`、`eol_release`）。

use std::collections::HashMap;

use rusqlite::Connection;

use crate::assets::dates::parse_iso_date;
use crate::error::CoreError;

/// 一個版本分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EolRelease {
    pub name: String,
    pub is_eol: bool,
    pub eol_from: Option<chrono::NaiveDate>,
    pub latest_name: Option<String>,
}

/// 一個代稱的資料（分支依 API 回應順序，新到舊）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EolProduct {
    /// endoflife.date 回應 404（保留先前的分支資料）
    pub not_found: bool,
    pub html_link: Option<String>,
    pub releases: Vec<EolRelease>,
}

/// 代稱 → 資料。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EolData {
    pub products: HashMap<String, EolProduct>,
}

impl EolData {
    pub fn get(&self, slug: &str) -> Option<&EolProduct> {
        self.products.get(slug)
    }
}

/// 讀出所有代稱與分支（日期格式不符的值視為未知）。
pub fn load_eol(conn: &Connection) -> Result<EolData, CoreError> {
    let mut products: HashMap<String, EolProduct> = HashMap::new();
    let mut stmt = conn.prepare("SELECT slug, status, html_link FROM eol_product")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (slug, status, html_link) = row?;
        products.insert(
            slug,
            EolProduct {
                not_found: status == "not_found",
                html_link,
                releases: Vec::new(),
            },
        );
    }
    let mut stmt = conn.prepare(
        "SELECT slug, name, is_eol, eol_from, latest_name FROM eol_release ORDER BY slug, position",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            EolRelease {
                name: r.get(1)?,
                is_eol: r.get(2)?,
                eol_from: r
                    .get::<_, Option<String>>(3)?
                    .as_deref()
                    .and_then(parse_iso_date),
                latest_name: r.get(4)?,
            },
        ))
    })?;
    for row in rows {
        let (slug, release) = row?;
        if let Some(p) = products.get_mut(&slug) {
            p.releases.push(release);
        }
    }
    Ok(EolData { products })
}
