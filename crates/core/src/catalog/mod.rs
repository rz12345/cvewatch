//! 產品目錄：把清冊中的各種產品寫法正規化為單一產品，並提供 CPE、CNA、CSAF 識別與版本格式。
//!
//! 載入順序：內嵌種子 → `<data_dir>\catalog.d\*.toml`（依檔名排序）。

pub mod file;
pub mod hardware;
pub mod lifecycle;
pub mod normalize;
pub mod role;
pub mod scheme;
pub mod store;
mod suggest;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use file::{CatalogFile, CpeDef, EsxiBuildDef, NameDef, ProductDef};
use hardware::HardwareBuilder;
pub use hardware::{HardwareCatalog, HardwareModel};
pub use lifecycle::EolCycle;
pub use normalize::{normalize, normalize_vendor};
pub use role::ComponentRole;
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
    /// 元件角色（只影響顯示，不納入指紋）
    pub role: ComponentRole,
    pub advisory_source: Option<String>,
    /// 原始寫法（含 key 與 display_name）
    pub aliases: Vec<String>,
    pub cpe: Vec<Cpe>,
    pub cna: Vec<NamePair>,
    pub csaf: Vec<NamePair>,
    /// 英文描述關鍵字（原始寫法）
    pub keywords: Vec<String>,
    /// endoflife.date 產品代稱（不影響比對，不納入指紋）
    pub endoflife: Option<String>,
    /// 目錄宣告的版本分支 EoL（不影響比對，不納入指紋）
    pub eol_cycles: Vec<EolCycle>,
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
    /// ESXi build → 版本名稱；後載入的來源覆寫相同 build
    esxi_builds: BTreeMap<u64, String>,
    /// 硬體型號 EoL 清單（22 design D4）
    hardware: HardwareCatalog,
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
        let mut esxi_builds = BTreeMap::new();
        let mut hardware = HardwareBuilder::default();
        for source in sources {
            let parsed: CatalogFile =
                toml::from_str(&source.text).map_err(|e| catalog_error(&source.name, e))?;
            for def in parsed.product {
                validate_role(&def, &source.name)?;
                lifecycle::validate(&def, &source.name)?;
                merge_into(&mut merged, def, &source.name);
            }
            for def in parsed.esxi_build {
                let (build, version) = esxi_build(def, &source.name)?;
                esxi_builds.insert(build, version);
            }
            for def in parsed.hardware {
                hardware.add(def, &source.name)?;
            }
        }
        let products = merged
            .into_iter()
            .map(|(def, origin)| build_product(def, &origin))
            .collect::<Result<Vec<_>, _>>()?;
        let mut catalog = index(products)?;
        catalog.esxi_builds = esxi_builds;
        catalog.hardware = hardware.build()?;
        Ok(catalog)
    }

    /// 影響比對結果的目錄內容（產品識別、版本格式、ESXi build 對照表）的雜湊。
    ///
    /// 只含影響比對的欄位（key、版本格式、CPE、CNA、CSAF、關鍵字、ESXi build 對照表），
    /// 以明確的 JSON 序列化計算；顯示名稱與別名的變更不影響。
    pub fn fingerprint(&self) -> String {
        let products: Vec<serde_json::Value> = self
            .products
            .iter()
            .map(|p| {
                serde_json::json!({
                    "key": p.key,
                    "scheme": p.version_scheme,
                    "cpe": p.cpe.iter().map(|c| [&c.part, &c.vendor, &c.product]).collect::<Vec<_>>(),
                    "cna": p.cna.iter().map(|n| [&n.vendor, &n.product]).collect::<Vec<_>>(),
                    "csaf": p.csaf.iter().map(|n| [&n.vendor, &n.product]).collect::<Vec<_>>(),
                    "keywords": p.keywords,
                })
            })
            .collect();
        let text = serde_json::json!({ "products": products, "esxi_builds": self.esxi_builds })
            .to_string();
        crate::hash::sha256_hex(&text)
    }

    /// ESXi build 對照表（build → 版本名稱）。
    pub fn esxi_builds(&self) -> &BTreeMap<u64, String> {
        &self.esxi_builds
    }

    /// 硬體型號 EoL 清單（種子沒有任何型號）。
    pub fn hardware(&self) -> &HardwareCatalog {
        &self.hardware
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

fn esxi_build(def: EsxiBuildDef, origin: &str) -> Result<(u64, String), CoreError> {
    let version = def.version.trim().to_string();
    if !crate::version_text::esxi::is_release_name(&version) {
        return Err(catalog_error(
            origin,
            format!(
                "ESXi build {} 的版本「{version}」須為寫明 Update 的名稱（例如 8.0 U3c；不接受 8.0.3）",
                def.build
            ),
        ));
    }
    Ok((def.build, version))
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

/// 在合併前逐來源驗證 `role`，讓錯誤指向實際寫錯的檔案（合併後的條目只保留第一個來源名稱）。
fn validate_role(def: &ProductDef, origin: &str) -> Result<(), CoreError> {
    product_role(def, origin).map(|_| ())
}

/// 條目的角色：未寫為「其他」，值不在清單時為錯誤（不靜默改為其他）。
fn product_role(def: &ProductDef, origin: &str) -> Result<ComponentRole, CoreError> {
    let Some(value) = def.role.as_deref() else {
        return Ok(ComponentRole::default());
    };
    ComponentRole::parse(value).ok_or_else(|| {
        catalog_error(
            origin,
            format!(
                "產品 `{}` 的 role `{value}` 不存在；可用：{}",
                def.key.trim(),
                ComponentRole::allowed()
            ),
        )
    })
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
        role: def.role.or(existing.role.take()),
        advisory_source: def.advisory_source.or(existing.advisory_source.take()),
        aliases: existing.aliases.drain(..).chain(def.aliases).collect(),
        cpe: def.cpe.or(existing.cpe.take()),
        cna: def.cna.or(existing.cna.take()),
        csaf: def.csaf.or(existing.csaf.take()),
        keywords: def.keywords.or(existing.keywords.take()),
        endoflife: def.endoflife.or(existing.endoflife.take()),
        eol_cycles: lifecycle::merge_cycles(existing.eol_cycles.take(), def.eol_cycles),
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
    let role = product_role(&def, origin)?;
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
    let eol_cycles = def
        .eol_cycles
        .unwrap_or_default()
        .iter()
        .map(|c| lifecycle::build_cycle(c, &key, origin))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Product {
        endoflife: lifecycle::slug(def.endoflife),
        eol_cycles,
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
        role,
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
        esxi_builds: BTreeMap::new(),
        hardware: HardwareCatalog::default(),
    })
}
