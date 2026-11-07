//! CSAF `product_tree` 走訪與產品目錄對應。
//!
//! branches（vendor → product_name → product_version / product_version_range）、
//! `full_product_names`（以 cpe helper 對應）與 `relationships`（沿用 product_reference 的對應）。
//! 同一 `product_id` 可能出現在多個 branch（Palo Alto 實例），因此對應結果為清單。

use std::collections::HashMap;

use serde_json::Value;

use super::vers;
use crate::catalog::{normalize, normalize_vendor, Catalog};
use crate::ingest::relevance::cpe_vendor_product;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionKind {
    Exact,
    Range,
}

impl VersionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VersionKind::Exact => "exact",
            VersionKind::Range => "range",
        }
    }
}

/// 對應到目錄的一個產品版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRef {
    pub catalog_key: String,
    /// 版本或標準化後的 vers 字串；沒有版本資訊時為 `*`
    pub version_expr: String,
    pub kind: VersionKind,
}

/// `product_id` → 對應到目錄的產品版本（對應不到的產品不在表中）。
pub type ProductMap = HashMap<String, Vec<ProductRef>>;

/// 沒有版本資訊（例如只有 cpe helper）時的版本表示。
pub const ANY_VERSION: &str = "*";

#[derive(Default, Clone)]
struct Context {
    vendor: Option<String>,
    product: Option<String>,
}

struct Walker<'a> {
    catalog: &'a Catalog,
    map: ProductMap,
    /// 無法解析的 vers 字串數
    invalid_ranges: usize,
}

impl Walker<'_> {
    fn catalog_key(&self, ctx: &Context, product: &Value) -> Option<String> {
        if let (Some(v), Some(p)) = (&ctx.vendor, &ctx.product) {
            let vendors = [normalize_vendor(v), normalize(v)];
            let product = normalize(p);
            let found = self.catalog.products().iter().find(|prod| {
                prod.csaf
                    .iter()
                    .any(|n| n.product == product && vendors.contains(&n.vendor))
            });
            if let Some(prod) = found {
                return Some(prod.key.clone());
            }
        }
        let cpe = product
            .pointer("/product_identification_helper/cpe")
            .and_then(Value::as_str)?;
        let (vendor, name) = cpe_vendor_product(cpe)?;
        self.catalog
            .products()
            .iter()
            .find(|p| {
                p.cpe
                    .iter()
                    .any(|c| c.vendor == vendor && c.product == name)
            })
            .map(|p| p.key.clone())
    }

    fn add(&mut self, product: &Value, key: String, version_expr: String, kind: VersionKind) {
        let Some(id) = product.get("product_id").and_then(Value::as_str) else {
            return;
        };
        let entry = self.map.entry(id.to_string()).or_default();
        let r = ProductRef {
            catalog_key: key,
            version_expr,
            kind,
        };
        if !entry.contains(&r) {
            entry.push(r);
        }
    }

    fn branches(&mut self, branches: &[Value], ctx: &Context) {
        for b in branches {
            let category = b
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = b.get("name").and_then(Value::as_str).unwrap_or_default();
            let mut here = ctx.clone();
            match category {
                "vendor" => here.vendor = Some(name.to_string()),
                "product_name" | "product_family" => here.product = Some(name.to_string()),
                _ => {}
            }
            if let Some(product) = b.get("product") {
                self.leaf(product, category, name, &here);
            }
            if let Some(children) = b.get("branches").and_then(Value::as_array) {
                self.branches(children, &here);
            }
        }
    }

    fn leaf(&mut self, product: &Value, category: &str, name: &str, ctx: &Context) {
        let Some(key) = self.catalog_key(ctx, product) else {
            return;
        };
        let (expr, kind) = match category {
            "product_version" => (name.trim().to_string(), VersionKind::Exact),
            "product_version_range" => match vers::parse(name) {
                Some(range) => (range.canonical(), VersionKind::Range),
                None => {
                    self.invalid_ranges += 1;
                    return;
                }
            },
            _ => (ANY_VERSION.to_string(), VersionKind::Range),
        };
        self.add(product, key, expr, kind);
    }
}

/// 走訪 product_tree，回傳對應表與無法解析的 vers 數量。
pub fn map_products(tree: &Value, catalog: &Catalog) -> (ProductMap, usize) {
    let mut w = Walker {
        catalog,
        map: ProductMap::new(),
        invalid_ranges: 0,
    };
    if let Some(branches) = tree.get("branches").and_then(Value::as_array) {
        w.branches(branches, &Context::default());
    }
    for fpn in tree
        .get("full_product_names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(key) = w.catalog_key(&Context::default(), fpn) {
            w.add(fpn, key, ANY_VERSION.to_string(), VersionKind::Range);
        }
    }
    for rel in tree
        .get("relationships")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let reference = rel.get("product_reference").and_then(Value::as_str);
        let (Some(reference), Some(fpn)) = (reference, rel.get("full_product_name")) else {
            continue;
        };
        let Some(refs) = w.map.get(reference).cloned() else {
            continue;
        };
        for r in refs {
            w.add(fpn, r.catalog_key, r.version_expr, r.kind);
        }
    }
    (w.map, w.invalid_ranges)
}
