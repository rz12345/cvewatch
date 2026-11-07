//! 產品目錄：把清冊中的各種產品寫法正規化為單一產品，並提供 CPE、CNA、CSAF 識別與版本格式。
//!
//! 載入順序：內嵌種子 → `<data_dir>\catalog.d\*.toml`（依檔名排序）。

pub mod file;
pub mod normalize;
pub mod scheme;
pub mod store;
mod suggest;

use std::collections::HashMap;
use std::path::Path;

use file::{CatalogFile, CpeDef, NameDef, ProductDef};
pub use normalize::{normalize, normalize_vendor};
use scheme::{scheme_by_name, VersionScheme};

use crate::error::CoreError;

/// 內建種子目錄。
pub const SEED_TOML: &str = include_str!("../../catalog/seed.toml");
pub const SEED_SOURCE_NAME: &str = "內建種子目錄";
const CPE_PARTS: [&str; 3] = ["a", "o", "h"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpe {
    pub part: String,
    pub vendor: String,
    pub product: String,
}

/// 正規化後的 vendor / product 名稱組。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePair {
    pub vendor: String,
    pub product: String,
}

#[derive(Debug, Clone)]
pub struct Product {
    pub key: String,
    pub display_name: String,
    pub vendor_display: String,
    pub version_scheme: String,
    pub advisory_source: Option<String>,
    /// 原始寫法（含 key 與 display_name）
    pub aliases: Vec<String>,
    pub cpe: Vec<Cpe>,
    pub cna: Vec<NamePair>,
    pub csaf: Vec<NamePair>,
    /// 英文描述關鍵字（原始寫法）
    pub keywords: Vec<String>,
}

impl Product {
    pub fn scheme(&self) -> &'static dyn VersionScheme {
        scheme_by_name(&self.version_scheme).expect("載入時已驗證 version_scheme")
    }
}

/// 一份目錄來源（檔名與內容）。
pub struct CatalogSource {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    products: Vec<Product>,
    by_key: HashMap<String, usize>,
    by_alias: HashMap<String, usize>,
}

fn catalog_error(source: &str, message: impl std::fmt::Display) -> CoreError {
    CoreError::Catalog(format!("{source}：{message}"))
}

impl Catalog {
    /// 只載入內建種子。
    pub fn seed() -> Result<Self, CoreError> {
        Self::from_sources(&[seed_source()])
    }

    /// 內建種子 + 使用者目錄下的 `*.toml`（目錄不存在時視為空）。
    pub fn load(user_dir: &Path) -> Result<Self, CoreError> {
        let mut sources = vec![seed_source()];
        sources.extend(read_user_dir(user_dir)?);
        Self::from_sources(&sources)
    }

    pub fn from_sources(sources: &[CatalogSource]) -> Result<Self, CoreError> {
        let mut merged: Vec<(ProductDef, String)> = Vec::new();
        for source in sources {
            let parsed: CatalogFile =
                toml::from_str(&source.text).map_err(|e| catalog_error(&source.name, e))?;
            for def in parsed.product {
                merge_into(&mut merged, def, &source.name);
            }
        }
        let products = merged
            .into_iter()
            .map(|(def, origin)| build_product(def, &origin))
            .collect::<Result<Vec<_>, _>>()?;
        index(products)
    }

    pub fn products(&self) -> &[Product] {
        &self.products
    }

    pub fn get(&self, key: &str) -> Option<&Product> {
        self.by_key.get(key).map(|&i| &self.products[i])
    }

    /// 以正規化後的名稱完全比對別名。
    pub fn resolve(&self, name: &str) -> Option<&Product> {
        self.by_alias
            .get(&normalize(name))
            .map(|&i| &self.products[i])
    }

    /// 未辨識名稱的相似產品 key（Jaro-Winkler ≥ 0.85，最多 3 個）。
    pub fn suggest(&self, name: &str) -> Vec<&str> {
        let candidates = self
            .by_alias
            .iter()
            .map(|(alias, &i)| (alias.as_str(), self.products[i].key.as_str()));
        suggest::best_keys(&normalize(name), candidates)
    }
}

fn seed_source() -> CatalogSource {
    CatalogSource {
        name: SEED_SOURCE_NAME.to_string(),
        text: SEED_TOML.to_string(),
    }
}

fn read_user_dir(dir: &Path) -> Result<Vec<CatalogSource>, CoreError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))?;
            Ok(CatalogSource {
                name: path.display().to_string(),
                text,
            })
        })
        .collect()
}

/// 同 key：有寫的欄位覆寫，`aliases` 合併；新 key：新增。
fn merge_into(merged: &mut Vec<(ProductDef, String)>, def: ProductDef, origin: &str) {
    let Some((existing, _)) = merged.iter_mut().find(|(d, _)| d.key == def.key) else {
        merged.push((def, origin.to_string()));
        return;
    };
    let combined = ProductDef {
        key: def.key,
        display_name: def.display_name.or(existing.display_name.take()),
        vendor_display: def.vendor_display.or(existing.vendor_display.take()),
        version_scheme: def.version_scheme.or(existing.version_scheme.take()),
        advisory_source: def.advisory_source.or(existing.advisory_source.take()),
        aliases: existing.aliases.drain(..).chain(def.aliases).collect(),
        cpe: def.cpe.or(existing.cpe.take()),
        cna: def.cna.or(existing.cna.take()),
        csaf: def.csaf.or(existing.csaf.take()),
        keywords: def.keywords.or(existing.keywords.take()),
    };
    *existing = combined;
}

fn required(
    value: Option<String>,
    field: &str,
    key: &str,
    origin: &str,
) -> Result<String, CoreError> {
    value
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| catalog_error(origin, format!("產品 `{key}` 缺少 {field}")))
}

fn build_product(def: ProductDef, origin: &str) -> Result<Product, CoreError> {
    let key = def.key.trim().to_string();
    let valid_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid_key {
        return Err(catalog_error(
            origin,
            format!("產品 key `{key}` 只能包含小寫英數字與 -"),
        ));
    }
    let version_scheme = required(def.version_scheme, "version_scheme", &key, origin)?;
    if scheme_by_name(&version_scheme).is_none() {
        return Err(catalog_error(
            origin,
            format!(
                "產品 `{key}` 的 version_scheme `{version_scheme}` 不存在；可用：{}",
                scheme::scheme_names().join("、")
            ),
        ));
    }
    let display_name = required(def.display_name, "display_name", &key, origin)?;
    let mut aliases = vec![key.clone(), display_name.clone()];
    aliases.extend(def.aliases);
    Ok(Product {
        vendor_display: required(def.vendor_display, "vendor_display", &key, origin)?,
        cpe: build_cpes(def.cpe.unwrap_or_default(), &key, origin)?,
        cna: name_pairs(def.cna.unwrap_or_default()),
        csaf: name_pairs(def.csaf.unwrap_or_default()),
        keywords: def
            .keywords
            .unwrap_or_default()
            .into_iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect(),
        advisory_source: def.advisory_source,
        aliases,
        key,
        display_name,
        version_scheme,
    })
}

fn build_cpes(defs: Vec<CpeDef>, key: &str, origin: &str) -> Result<Vec<Cpe>, CoreError> {
    defs.into_iter()
        .map(|d| {
            if !CPE_PARTS.contains(&d.part.as_str()) {
                return Err(catalog_error(
                    origin,
                    format!("產品 `{key}` 的 CPE part `{}` 必須是 a、o 或 h", d.part),
                ));
            }
            Ok(Cpe {
                part: d.part,
                vendor: d.vendor.trim().to_lowercase(),
                product: d.product.trim().to_lowercase(),
            })
        })
        .collect()
}

fn name_pairs(defs: Vec<NameDef>) -> Vec<NamePair> {
    defs.into_iter()
        .map(|d| NamePair {
            vendor: normalize(&d.vendor),
            product: normalize(&d.product),
        })
        .collect()
}

/// 建立 key 與別名索引；不同產品宣告相同別名時拒絕載入。
fn index(products: Vec<Product>) -> Result<Catalog, CoreError> {
    let mut by_key = HashMap::new();
    let mut by_alias: HashMap<String, usize> = HashMap::new();
    for (i, product) in products.iter().enumerate() {
        by_key.insert(product.key.clone(), i);
        for alias in &product.aliases {
            let norm = normalize(alias);
            if norm.is_empty() {
                continue;
            }
            match by_alias.get(&norm) {
                Some(&other) if other != i => {
                    return Err(CoreError::Catalog(format!(
                        "別名「{alias}」同時屬於產品 `{}` 與 `{}`",
                        products[other].key, product.key
                    )));
                }
                _ => {
                    by_alias.insert(norm, i);
                }
            }
        }
    }
    Ok(Catalog {
        products,
        by_key,
        by_alias,
    })
}
