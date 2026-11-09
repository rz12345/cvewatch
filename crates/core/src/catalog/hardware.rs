//! 硬體型號的 EoL 清單（22 design D4、D5）：只由使用者在 `catalog.d` 宣告。

use std::collections::HashMap;

use chrono::NaiveDate;

use super::catalog_error;
use super::file::HardwareDef;
use super::lifecycle::is_http_url;
use super::normalize::{normalize, normalize_vendor};
use crate::assets::dates::parse_iso_date;
use crate::error::CoreError;

/// 一個硬體型號的 EoL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareModel {
    pub vendor: String,
    pub vendor_aliases: Vec<String>,
    pub model: String,
    pub aliases: Vec<String>,
    pub eol: NaiveDate,
    pub source: String,
}

impl HardwareModel {
    /// 「Aruba XYZ-100」
    pub fn label(&self) -> String {
        format!("{} {}", self.vendor, self.model)
    }

    fn key(&self) -> (String, String) {
        (normalize_vendor(&self.vendor), normalize(&self.model))
    }

    fn vendor_keys(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(&self.vendor)
            .chain(&self.vendor_aliases)
            .map(|v| normalize_vendor(v))
            .filter(|v| !v.is_empty())
    }

    fn model_keys(&self) -> impl Iterator<Item = (String, &str)> + '_ {
        std::iter::once(&self.model)
            .chain(&self.aliases)
            .map(|m| (normalize(m), m.as_str()))
            .filter(|(m, _)| !m.is_empty())
    }
}

/// 硬體型號清單與 `(廠牌, 型號)` 索引（含廠牌別名與型號別名）。
#[derive(Debug, Clone, Default)]
pub struct HardwareCatalog {
    models: Vec<HardwareModel>,
    index: HashMap<(String, String), usize>,
}

impl HardwareCatalog {
    pub fn models(&self) -> &[HardwareModel] {
        &self.models
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// 以資產的廠牌與型號查詢；兩者都必須有值。
    pub fn lookup(&self, brand: &str, model: &str) -> Option<&HardwareModel> {
        let key = (normalize_vendor(brand), normalize(model));
        if key.0.is_empty() || key.1.is_empty() {
            return None;
        }
        self.index.get(&key).map(|&i| &self.models[i])
    }
}

/// 收集各來源的宣告：相同 `(廠牌, 型號)` 以後載入者為準。
#[derive(Default)]
pub(super) struct HardwareBuilder {
    models: Vec<HardwareModel>,
}

impl HardwareBuilder {
    pub(super) fn add(&mut self, def: HardwareDef, origin: &str) -> Result<(), CoreError> {
        let model = build_model(def, origin)?;
        let key = model.key();
        match self.models.iter_mut().find(|m| m.key() == key) {
            Some(slot) => *slot = model,
            None => self.models.push(model),
        }
        Ok(())
    }

    /// 建立索引；同一廠牌下的同一型號寫法屬於兩個型號時拒絕載入。
    pub(super) fn build(self) -> Result<HardwareCatalog, CoreError> {
        let mut index: HashMap<(String, String), usize> = HashMap::new();
        for (i, model) in self.models.iter().enumerate() {
            for vendor in model.vendor_keys() {
                for (name, raw) in model.model_keys() {
                    match index.get(&(vendor.clone(), name.clone())) {
                        Some(&other) if other != i => {
                            return Err(CoreError::Catalog(format!(
                                "硬體型號寫法「{raw}」同時屬於 `{}` 與 `{}`",
                                self.models[other].label(),
                                model.label()
                            )));
                        }
                        _ => {
                            index.insert((vendor.clone(), name), i);
                        }
                    }
                }
            }
        }
        Ok(HardwareCatalog {
            models: self.models,
            index,
        })
    }
}

fn build_model(def: HardwareDef, origin: &str) -> Result<HardwareModel, CoreError> {
    let vendor = def.vendor.trim().to_string();
    let model = def.model.trim().to_string();
    if normalize_vendor(&vendor).is_empty() || normalize(&model).is_empty() {
        return Err(catalog_error(
            origin,
            "硬體型號必須有廠牌（vendor）與型號（model）",
        ));
    }
    let label = format!("{vendor} {model}");
    let fail = |message: String| catalog_error(origin, format!("硬體型號 `{label}` {message}"));
    let eol_text = def.eol.as_deref().map(str::trim).unwrap_or_default();
    if eol_text.is_empty() {
        return Err(fail("缺少 EoL 日期（eol）".into()));
    }
    let eol = parse_iso_date(eol_text)
        .ok_or_else(|| fail(format!("的 EoL 日期「{eol_text}」不是 YYYY-MM-DD")))?;
    let source = def.source.as_deref().map(str::trim).unwrap_or_default();
    if source.is_empty() {
        return Err(fail("缺少來源網址（source）".into()));
    }
    if !is_http_url(source) {
        return Err(fail(format!("的來源「{source}」不是 http 或 https 網址")));
    }
    let clean = |v: Vec<String>| -> Vec<String> {
        v.into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    Ok(HardwareModel {
        vendor,
        vendor_aliases: clean(def.vendor_aliases),
        model,
        aliases: clean(def.aliases),
        eol,
        source: source.to_string(),
    })
}
