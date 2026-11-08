//! 資產與產品目錄的顯示輸出。

use std::collections::BTreeMap;

use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::{AssetFilter, AssetRepo, Category, ComponentRepo};
use cvewatch_core::catalog::Product;
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

use crate::app::App;

pub fn print_plan_details(plan: &ImportPlan) {
    if !plan.added.is_empty() {
        println!("新增：{}", plan.added.join("、"));
    }
    if !plan.updated.is_empty() {
        println!("更新：");
    }
    for u in &plan.updated {
        for v in &u.version_changes {
            println!(
                "  {} 版本 {} → {}（{}）",
                u.asset_tag, v.from, v.to, v.product_key
            );
        }
        let mut notes = Vec::new();
        if !u.changed_fields.is_empty() {
            notes.push(format!("欄位變更：{}", u.changed_fields.join("、")));
        }
        if !u.components_added.is_empty() {
            notes.push(format!("新增元件：{}", u.components_added.join("、")));
        }
        if !u.components_removed.is_empty() {
            notes.push(format!("移除元件：{}", u.components_removed.join("、")));
        }
        if u.reactivated {
            notes.push("重新啟用（先前已退役）".to_string());
        }
        if !notes.is_empty() {
            println!("  {} {}", u.asset_tag, notes.join("；"));
        }
    }
    if !plan.unresolved.is_empty() {
        println!("未辨識產品（不建立元件，可在 catalog.d 補別名）：");
    }
    for item in &plan.unresolved {
        let hint = if item.suggestions.is_empty() {
            "無相似產品".to_string()
        } else {
            format!("建議：{}", item.suggestions.join("、"))
        };
        println!(
            "  {} {}「{}」→ {hint}",
            item.row, item.asset_tag, item.product_raw
        );
    }
    if !plan.warnings.is_empty() {
        println!("版本無法解析（仍保存原始字串）：");
    }
    for w in &plan.warnings {
        match w.row {
            Some(row) => println!("  {row} {}", w.describe()),
            None => println!("  {}", w.describe()),
        }
    }
    if !plan.disappeared.is_empty() {
        println!("清冊中消失：{}", plan.disappeared.join("、"));
    }
}

pub fn list(
    app: &App,
    category: Option<&str>,
    unresolved_only: bool,
) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let repo = AssetRepo::new(&conn);
    let filter = AssetFilter {
        category: category.and_then(Category::parse),
        include_retired: false,
    };
    let unresolved: BTreeMap<String, usize> =
        repo.unresolved()?
            .into_iter()
            .fold(BTreeMap::new(), |mut acc, u| {
                *acc.entry(u.asset_tag).or_insert(0) += 1;
                acc
            });
    let components = ComponentRepo::new(&conn);
    println!(
        "{:<14} {:<16} {:<15} {:<9} {:>4} {:>5}  未辨識",
        "asset_tag", "name", "category", "exposure", "重要", "元件"
    );
    let mut shown = 0;
    for asset in repo.list(&filter)? {
        let pending = unresolved
            .get(&asset.record.asset_tag)
            .copied()
            .unwrap_or(0);
        if unresolved_only && pending == 0 {
            continue;
        }
        let r = &asset.record;
        println!(
            "{:<14} {:<16} {:<15} {:<9} {:>4} {:>5}  {}",
            r.asset_tag,
            r.name,
            r.category.as_str(),
            r.exposure.as_str(),
            r.criticality,
            components.for_asset(asset.id)?.len(),
            if pending > 0 {
                pending.to_string()
            } else {
                String::new()
            }
        );
        shown += 1;
    }
    println!("共 {shown} 筆");
    Ok(ExitCode::Success)
}

pub fn show(app: &App, tag: &str) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let asset = AssetRepo::new(&conn)
        .find_by_tag(tag)?
        .ok_or_else(|| CoreError::Invalid(format!("找不到資產「{tag}」")))?;
    let r = &asset.record;
    println!("{}（{}）", r.asset_tag, r.name);
    println!(
        "  類別 {}、曝露度 {}、重要性 {}、狀態 {}",
        r.category, r.exposure, r.criticality, asset.status
    );
    for (label, value) in [
        ("地點", &r.location),
        ("負責人", &r.owner),
        ("備註", &r.notes),
    ] {
        if let Some(v) = value {
            println!("  {label}：{v}");
        }
    }
    println!(
        "  首次匯入 {}、最後出現於清冊 {}",
        asset.first_seen, asset.last_seen
    );
    if let Some(at) = &asset.retired_at {
        println!("  退役於 {at}");
    }
    let components = ComponentRepo::new(&conn);
    println!("元件：");
    for c in components.for_asset(asset.id)? {
        println!("  {}  {}（{}）", c.label(), c.version_raw, c.source);
        for h in components.history(c.id)? {
            println!("      先前 {}，於 {} 變更", h.version_raw, h.changed_at);
        }
    }
    let unresolved: Vec<_> = AssetRepo::new(&conn)
        .unresolved()?
        .into_iter()
        .filter(|u| u.asset_tag == r.asset_tag)
        .collect();
    for u in unresolved {
        println!("  未辨識：「{}」{}", u.product_raw, u.version_raw);
    }
    Ok(ExitCode::Success)
}

pub fn catalog_list(app: &App) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    println!(
        "{:<18} {:<26} {:<20} {:<7} {:>4} {:>4} {:>4} {:>5}",
        "key", "display_name", "vendor", "scheme", "別名", "CPE", "CNA", "CSAF"
    );
    for p in catalog.products() {
        println!(
            "{:<18} {:<26} {:<20} {:<7} {:>4} {:>4} {:>4} {:>5}",
            p.key,
            p.display_name,
            p.vendor_display,
            p.version_scheme,
            p.aliases.len(),
            p.cpe.len(),
            p.cna.len(),
            p.csaf.len()
        );
    }
    println!(
        "共 {} 個產品；使用者擴充目錄：{}",
        catalog.products().len(),
        app.paths.catalog_dir.display()
    );
    Ok(ExitCode::Success)
}

pub fn catalog_show(app: &App, key: &str) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let p = catalog.get(key).ok_or_else(|| {
        CoreError::Invalid(format!(
            "目錄中沒有產品 `{key}`；可用 `asset catalog list` 查看"
        ))
    })?;
    print_product(p);
    Ok(ExitCode::Success)
}

fn print_product(p: &Product) {
    println!("{}（{} / {}）", p.key, p.display_name, p.vendor_display);
    println!("  版本格式：{}", p.version_scheme);
    if let Some(src) = &p.advisory_source {
        println!("  原廠公告來源：{src}");
    }
    println!("  別名：{}", p.aliases.join("、"));
    let cpe: Vec<String> = p
        .cpe
        .iter()
        .map(|c| format!("{}:{}:{}", c.part, c.vendor, c.product))
        .collect();
    let pairs = |v: &[cvewatch_core::catalog::NamePair]| -> String {
        if v.is_empty() {
            "（無）".to_string()
        } else {
            v.iter()
                .map(|n| format!("{} / {}", n.vendor, n.product))
                .collect::<Vec<_>>()
                .join("、")
        }
    };
    println!(
        "  CPE：{}",
        if cpe.is_empty() {
            "（無）".to_string()
        } else {
            cpe.join("、")
        }
    );
    println!("  CNA：{}", pairs(&p.cna));
    println!("  CSAF：{}", pairs(&p.csaf));
    println!(
        "  描述關鍵字：{}",
        if p.keywords.is_empty() {
            "（無）".to_string()
        } else {
            p.keywords.join("、")
        }
    );
}

pub fn catalog_unresolved(app: &App) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let conn = open_working_db(&app.paths, &app.store)?;
    let entries = AssetRepo::new(&conn).unresolved()?;
    if entries.is_empty() {
        println!("沒有未辨識的產品名稱。");
        return Ok(ExitCode::Success);
    }
    let grouped: BTreeMap<&str, Vec<&str>> = entries.iter().fold(BTreeMap::new(), |mut acc, e| {
        acc.entry(e.product_raw.as_str())
            .or_default()
            .push(e.asset_tag.as_str());
        acc
    });
    for (name, tags) in grouped {
        let suggestions = catalog.suggest(name);
        let hint = if suggestions.is_empty() {
            String::new()
        } else {
            format!("　建議：{}", suggestions.join("、"))
        };
        println!("「{name}」：{}{hint}", tags.join("、"));
    }
    println!(
        "在 {} 新增 .toml 補上別名後重新匯入即可辨識。",
        app.paths.catalog_dir.display()
    );
    Ok(ExitCode::Success)
}
