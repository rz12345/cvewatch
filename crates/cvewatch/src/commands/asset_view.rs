//! 資產與產品目錄的顯示輸出。

use std::collections::BTreeMap;

use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::{
    AssetClasses, AssetFilter, AssetIp, AssetIpRepo, AssetRecord, AssetRepo, ClassFilter,
    ComponentRepo, SystemDirectory,
};
use cvewatch_core::catalog::Product;
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

use cvewatch_frontend::asset_form::{personal_data_label, rated_text};

use super::component_view::component_lines;
use crate::app::App;

pub fn print_plan_details(plan: &ImportPlan) {
    for line in plan_detail_lines(plan) {
        println!("{line}");
    }
}

/// 「  版本待補 1 個元件」；沒有待補時為 None（18 design D3）。
pub fn pending_line(plan: &ImportPlan) -> Option<String> {
    let pending = plan.pending_components();
    (pending > 0).then(|| format!("  版本待補 {pending} 個元件"))
}

/// 更新資產的欄位變更如何呈現（18 design D5）。
#[derive(Clone, Copy)]
pub enum FieldDetail<'a> {
    /// 只列標準欄名（一般清冊匯入）
    Names,
    /// 逐欄列出新舊值（ISMS 匯入）
    Values(&'a AssetClasses),
}

/// 匯入差異的明細（GUI 匯入預覽顯示同一份輸出）：資產說明變更、新增、更新、未辨識產品、
/// 版本警告，之後是 notice（舊版 category 值、停用欄位；13 design D3），最後是清冊中消失的資產。
pub fn plan_detail_lines(plan: &ImportPlan) -> Vec<String> {
    plan_detail_lines_with(plan, FieldDetail::Names)
}

/// 同 [`plan_detail_lines`]，欄位變更依 `fields` 呈現。
pub fn plan_detail_lines_with(plan: &ImportPlan, fields: FieldDetail<'_>) -> Vec<String> {
    let mut out = Vec::new();
    if !plan.systems.is_empty() {
        out.push("資產說明：".to_string());
    }
    out.extend(plan.systems.iter().map(|c| format!("  {}", c.describe())));
    if !plan.added.is_empty() {
        out.push(format!("新增：{}", plan.added.join("、")));
    }
    if !plan.updated.is_empty() {
        out.push("更新：".to_string());
    }
    for u in &plan.updated {
        for v in &u.version_changes {
            out.push(format!(
                "  {} 版本 {}（{}）",
                u.asset_tag,
                v.describe(),
                v.product_key
            ));
        }
        let mut notes = Vec::new();
        if !u.changed_fields.is_empty() || u.ip_change.is_some() {
            let changes = match fields {
                FieldDetail::Names => u.field_names().join("、"),
                FieldDetail::Values(classes) => u
                    .changed_fields
                    .iter()
                    .map(|c| c.describe(classes))
                    .collect::<Vec<_>>()
                    .join("、"),
            };
            if !changes.is_empty() {
                notes.push(format!("欄位變更：{changes}"));
            }
        }
        if let Some(c) = &u.ip_change {
            notes.push(format!("IP {}", c.describe()));
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
            out.push(format!("  {} {}", u.asset_tag, notes.join("；")));
        }
    }
    if !plan.unresolved.is_empty() {
        out.push("未辨識產品（不建立元件，可在 catalog.d 補別名）：".to_string());
    }
    for item in &plan.unresolved {
        let hint = if item.suggestions.is_empty() {
            "無相似產品".to_string()
        } else {
            format!("建議：{}", item.suggestions.join("、"))
        };
        out.push(format!(
            "  {} {}「{}」→ {hint}",
            item.row, item.asset_tag, item.product_raw
        ));
    }
    if !plan.warnings.is_empty() {
        out.push("版本無法解析（仍保存原始字串）：".to_string());
    }
    for w in &plan.warnings {
        out.push(match w.row {
            Some(row) => format!("  {row} {}", w.describe()),
            None => format!("  {}", w.describe()),
        });
    }
    if !plan.notices.is_empty() {
        out.push("提醒：".to_string());
    }
    out.extend(plan.notices.iter().map(|n| format!("  {n}")));
    if !plan.disappeared.is_empty() {
        out.push(format!("清冊中消失：{}", plan.disappeared.join("、")));
    }
    out
}

pub fn list(
    app: &App,
    category: Option<&str>,
    unresolved_only: bool,
) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let repo = AssetRepo::new(&conn);
    let filter = AssetFilter {
        category: category.and_then(ClassFilter::parse),
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
        "{:<14} {:<16} {:<9} {:<9} {:>4} {:>5}  未辨識",
        "asset_tag", "name", "category", "exposure", "價值", "元件"
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
            "{:<14} {:<16} {:<9} {:<9} {:>4} {:>5}  {}",
            r.asset_tag,
            r.name,
            r.category.as_str(),
            r.exposure.as_str(),
            r.cia.value(),
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
    // 目錄在輸出任何內容前載入：catalog.d 有錯時直接失敗，不留下半份輸出
    let catalog = app.load_catalog()?;
    let conn = open_working_db(&app.paths, &app.store)?;
    let asset = AssetRepo::new(&conn)
        .find_by_tag(tag)?
        .ok_or_else(|| CoreError::Invalid(format!("找不到資產「{tag}」")))?;
    let r = &asset.record;
    let classes = app.loaded.config.asset_classes()?;
    println!("{}（{}）", r.asset_tag, r.name);
    println!(
        "  類別 {}、曝露度 {}、狀態 {}",
        classes.label(&r.category),
        r.exposure,
        asset.status
    );
    println!("  C/I/A {}（價值 {}）", r.cia, r.cia.value());
    if let Some(id) = &r.isms_id {
        println!("  ISMS 編號 {id}");
    }
    for line in admin_lines(r, &SystemDirectory::load(&conn)?) {
        println!("  {line}");
    }
    for line in identity_lines(r, &AssetIpRepo::new(&conn).for_asset(asset.id)?) {
        println!("  {line}");
    }
    for (label, value) in [("地點", &r.location), ("備註", &r.notes)] {
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
    let repo = ComponentRepo::new(&conn);
    let components = repo.for_asset(asset.id)?;
    let history = repo.history_for_asset(asset.id)?;
    let unresolved: Vec<_> = AssetRepo::new(&conn)
        .unresolved()?
        .into_iter()
        .filter(|u| u.asset_tag == r.asset_tag)
        .collect();
    println!("元件：");
    for line in component_lines(&catalog, &components, &history, &unresolved) {
        println!("{line}");
    }
    Ok(ExitCode::Success)
}

/// `asset show` 的 ISMS 管理欄位（16 任務 4.1）：資產說明、單位與保管人只在有值時列出，
/// 法規遵循性、防護等級與是否含個人資料一律列出。
pub fn admin_lines(r: &AssetRecord, systems: &SystemDirectory) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(code) = &r.system_code {
        out.push(format!("資產說明：{}", systems.label(code)));
    }
    for (label, value) in [
        ("權責單位", &r.responsible_unit),
        ("保管人", &r.custodian),
        ("使用單位", &r.user_unit),
    ] {
        if let Some(v) = value {
            out.push(format!("{label}：{v}"));
        }
    }
    out.push(format!(
        "法規遵循性 {}、防護等級 {}、是否含個人資料 {}",
        rated_text(r.compliance),
        rated_text(r.protection_level()),
        personal_data_label(r.personal_data)
    ));
    out
}

/// `asset show` 的識別欄位與 IP（20 design D12）：只列出有值的欄位，每筆 IP 一行
/// （「IP：192.0.2.1（管理）」）。
pub fn identity_lines(r: &AssetRecord, ips: &[AssetIp]) -> Vec<String> {
    let mut out: Vec<String> = [
        ("廠牌", &r.brand),
        ("型號", &r.model),
        ("序號", &r.serial_number),
        ("財產編號", &r.property_number),
        ("主機名稱", &r.hostname),
        ("管理介面", &r.management_url),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.as_ref().map(|v| format!("{label}：{v}")))
    .collect();
    out.extend(ips.iter().map(|ip| format!("IP：{ip}")));
    out
}

pub fn catalog_list(app: &App) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    println!(
        "{:<18} {:<26} {:<20} {:<7} {:<9} {:>4} {:>4} {:>4} {:>5}",
        "key", "display_name", "vendor", "scheme", "role", "別名", "CPE", "CNA", "CSAF"
    );
    for p in catalog.products() {
        println!(
            "{:<18} {:<26} {:<20} {:<7} {:<9} {:>4} {:>4} {:>4} {:>5}",
            p.key,
            p.display_name,
            p.vendor_display,
            p.version_scheme,
            p.role.as_str(),
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
    println!("  角色：{}（{}）", p.role.label(), p.role.as_str());
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cvewatch_core::assets::columns::ColumnMap;
    use cvewatch_core::assets::import::{import_inventory, ImportOptions};
    use cvewatch_core::assets::sheet::read_inventory;
    use cvewatch_core::assets::validate::parse_inventory;
    use cvewatch_core::assets::AssetClasses;
    use cvewatch_core::catalog::Catalog;
    use cvewatch_core::db::migrate::migrate;

    use super::*;

    /// 16 任務 4.1：以 inventory_basic 匯入後，`asset show FW-01` 的管理欄位與預覽的資產說明變更。
    #[test]
    fn show_lists_admin_fields_and_preview_lists_system_changes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/inventory_basic.xlsx");
        let book = read_inventory(&path, None, None).unwrap();
        let inventory =
            parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).unwrap();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let plan = import_inventory(
            &mut conn,
            &Catalog::seed().unwrap(),
            &inventory,
            ImportOptions::default(),
            chrono::Utc::now(),
        )
        .unwrap();
        let lines = plan_detail_lines(&plan);
        assert_eq!(lines[..2], ["資產說明：", "  新增資產說明 SYS 核心系統"]);
        let systems = SystemDirectory::load(&conn).unwrap();
        let fw = AssetRepo::new(&conn).find_by_tag("FW-01").unwrap().unwrap();
        assert_eq!(
            admin_lines(&fw.record, &systems),
            vec![
                "資產說明：SYS 核心系統",
                "權責單位：網管組",
                "保管人：王小明",
                "使用單位：網管組",
                "法規遵循性 未評定、防護等級 未評定、是否含個人資料 否",
            ]
        );
        let srv = AssetRepo::new(&conn)
            .find_by_tag("SRV-01")
            .unwrap()
            .unwrap();
        assert_eq!(
            admin_lines(&srv.record, &systems).last().unwrap(),
            "法規遵循性 3、防護等級 4、是否含個人資料 否"
        );
    }

    /// 20 任務 5.2：Showing a firewall with identity data，以及預覽列出 IP 變更。
    #[test]
    fn show_lists_identity_fields_and_preview_lists_address_changes() {
        let fixture = |name: &str| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures")
                .join(name)
        };
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let mut run = |file: &str, dry_run: bool| {
            let book = read_inventory(&fixture(file), None, None).unwrap();
            let inventory =
                parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).unwrap();
            import_inventory(
                &mut conn,
                &Catalog::seed().unwrap(),
                &inventory,
                ImportOptions {
                    dry_run,
                    prune: false,
                },
                chrono::Utc::now(),
            )
            .unwrap()
        };
        run("inventory_basic.xlsx", false);
        let preview = run("inventory_identity.xlsx", true);
        let lines = plan_detail_lines(&preview);
        let srv = lines
            .iter()
            .find(|l| l.starts_with("  SRV-01 "))
            .expect("SRV-01 列為更新");
        assert!(
            srv.contains("property_number、hostname、ip；IP - → 192.0.2.10（管理）、2001:db8::10"),
            "{srv}"
        );
        run("inventory_identity.xlsx", false);
        conn.execute(
            "UPDATE asset SET brand = 'Aruba', serial_number = NULL WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
        let fw = AssetRepo::new(&conn)
            .find_by_tag("FW-EDGE")
            .unwrap()
            .unwrap();
        let ips = AssetIpRepo::new(&conn).for_asset(fw.id).unwrap();
        let lines = identity_lines(&fw.record, &ips);
        assert_eq!(
            lines,
            [
                "廠牌：Aruba",
                "型號：XYZ-100",
                "財產編號：P-0001",
                "主機名稱：fw-edge",
                "管理介面：https://192.0.2.1/",
                "IP：192.0.2.1（管理）",
            ]
        );
        assert!(!lines.iter().any(|l| l.starts_with("序號")));
        let nas = AssetRepo::new(&conn)
            .find_by_tag("NAS-01")
            .unwrap()
            .unwrap();
        assert!(identity_lines(&nas.record, &[]).is_empty());
    }

    /// 13 任務 5.4：舊清冊的預覽在版本警告之後列出舊值轉換與 criticality 停用的提醒。
    #[test]
    fn legacy_inventory_preview_lists_notices_after_version_warnings() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/inventory_legacy.xlsx");
        let book = read_inventory(&path, None, None).unwrap();
        let inventory =
            parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).unwrap();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let options = ImportOptions {
            dry_run: true,
            prune: false,
        };
        let plan = import_inventory(
            &mut conn,
            &Catalog::seed().unwrap(),
            &inventory,
            options,
            chrono::Utc::now(),
        )
        .unwrap();
        let lines = plan_detail_lines(&plan);
        let at = |needle: &str| {
            lines
                .iter()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("缺少「{needle}」：{lines:#?}"))
        };
        assert!(at("版本無法解析") < at("提醒："), "{lines:#?}");
        assert!(at("提醒：") < at("security 已轉為 HW03 資安設備"));
        assert!(at("第 2 列 FW-01：security 已轉為 HW03 資安設備") > 0);
        at("criticality 欄已停用");
    }

    /// 18 任務 5.2：清冊中 version 空白的元件匯入成功，摘要列出版本待補數。
    #[test]
    fn blank_version_is_imported_as_pending() {
        use cvewatch_core::assets::template::{write_sheets, SheetData};
        let headers = ["asset_tag", "name", "category", "product", "version"];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inv.xlsx");
        write_sheets(
            &path,
            &[SheetData {
                name: "assets",
                headers: headers.iter().map(|h| h.to_string()).collect(),
                rows: vec![["NAS-03", "nas03", "HW05", "Synology DSM", ""]
                    .iter()
                    .map(|c| c.to_string())
                    .collect()],
            }],
        )
        .unwrap();
        let book = read_inventory(&path, None, None).unwrap();
        let inventory =
            parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).unwrap();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let plan = import_inventory(
            &mut conn,
            &Catalog::seed().unwrap(),
            &inventory,
            ImportOptions::default(),
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(pending_line(&plan).as_deref(), Some("  版本待補 1 個元件"));
        assert_eq!(pending_line(&ImportPlan::default()), None);
    }
}
