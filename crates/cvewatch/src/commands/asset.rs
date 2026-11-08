//! `cvewatch asset import | import-packages | list | show | export | template | catalog`

use std::path::Path;

use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::export::export_inventory;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::packages::import::{import_packages, PackageImportReport};
use cvewatch_core::assets::packages::{parse_packages, Distro, PackageFormat, ParsedPackages};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::{ensure_absent, write_template};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

use super::asset_view;
use crate::app::App;
use crate::cli::{AssetAction, CatalogAction, PackageFormatArg};

pub fn run(app: &App, action: AssetAction) -> Result<ExitCode, CoreError> {
    match action {
        AssetAction::Import {
            file,
            components,
            dry_run,
            prune,
        } => import(
            app,
            &file,
            components.as_deref(),
            ImportOptions { dry_run, prune },
        ),
        AssetAction::ImportPackages {
            asset_tag,
            file,
            format,
            distro,
        } => packages(app, &asset_tag, &file, format, distro.as_deref()),
        AssetAction::List {
            category,
            unresolved,
        } => asset_view::list(app, category.as_deref(), unresolved),
        AssetAction::Show { asset_tag } => asset_view::show(app, &asset_tag),
        AssetAction::Export { path } => export(app, &path),
        AssetAction::Template { path } => template(&path),
        AssetAction::Catalog { unresolved, action } => match (unresolved, action) {
            (true, _) => asset_view::catalog_unresolved(app),
            (false, None | Some(CatalogAction::List)) => asset_view::catalog_list(app),
            (false, Some(CatalogAction::Show { key })) => asset_view::catalog_show(app, &key),
        },
    }
}

fn import(
    app: &App,
    file: &Path,
    components: Option<&Path>,
    options: ImportOptions,
) -> Result<ExitCode, CoreError> {
    let columns = ColumnMap::from_config(&app.loaded.config.import.columns)?;
    let book = read_inventory(file, components)?;
    let inventory = parse_inventory(&book, &columns)?;
    let catalog = app.load_catalog()?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let plan = import_inventory(&mut conn, &catalog, &inventory, options, chrono::Utc::now())?;
    print_plan(&plan, options);
    if options.dry_run {
        return Ok(ExitCode::Success);
    }
    // 影響比對的變更已在匯入交易中記錄為比對待補；另含先前未完成的比對與目錄／評分設定變更
    Ok(
        super::matching::rematch_if_needed(app, &mut conn, &catalog).unwrap_or_else(|| {
            println!("清冊沒有影響比對的變更，略過比對。");
            ExitCode::Success
        }),
    )
}

fn print_plan(plan: &ImportPlan, options: ImportOptions) {
    let title = if options.dry_run {
        "匯入預覽（--dry-run，未寫入資料庫）"
    } else {
        "匯入完成"
    };
    let prune_note = match (options.prune, options.dry_run) {
        (false, _) => "（未加 --prune 不處理）",
        (true, true) => "（加上 --prune 後會標為退役）",
        (true, false) => "（已標為退役）",
    };
    println!("{title}");
    println!(
        "  新增 {}、更新 {}（版本變更 {}）、未變 {}、未辨識產品 {}、清冊中消失 {}{}",
        plan.added.len(),
        plan.updated.len(),
        plan.version_change_count(),
        plan.unchanged,
        plan.unresolved.len(),
        plan.disappeared.len(),
        if plan.disappeared.is_empty() {
            ""
        } else {
            prune_note
        }
    );
    asset_view::print_plan_details(plan);
}

fn packages(
    app: &App,
    asset_tag: &str,
    file: &Path,
    format: PackageFormatArg,
    distro: Option<&str>,
) -> Result<ExitCode, CoreError> {
    let text = std::fs::read_to_string(file).map_err(|e| CoreError::io(file, e))?;
    let distro = distro.map(Distro::parse).transpose()?;
    let format = match format {
        PackageFormatArg::Auto => None,
        PackageFormatArg::Cyclonedx => Some(PackageFormat::CycloneDx),
        PackageFormatArg::Spdx => Some(PackageFormat::Spdx),
        PackageFormatArg::Dpkg => Some(PackageFormat::Dpkg),
        PackageFormatArg::Rpm => Some(PackageFormat::Rpm),
    };
    let parsed = parse_packages(&text, format, distro.as_ref())?;
    // 目錄先載入：catalog.d 有錯時在寫入前就失敗，而不是寫入後才回報錯誤
    let catalog = app.load_catalog()?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let report = import_packages(&mut conn, asset_tag, &parsed, chrono::Utc::now())?;
    print_package_report(asset_tag, &parsed, &report);
    if let Some(code) = super::matching::rematch_if_needed(app, &mut conn, &catalog) {
        return Ok(code);
    }
    // 新增或變更的套件要等下次 sync 以 OSV 查詢後才能判定；移除的套件命中維持原狀
    let unchanged =
        report.added == 0 && report.version_changes.is_empty() && report.removed.is_empty();
    let message = if unchanged {
        "套件清單沒有變更，略過比對。"
    } else if app.loaded.config.sources.osv_enabled {
        "套件已更新；新版本將於下次 `cvewatch sync` 以 OSV 查詢後比對。"
    } else {
        "套件已更新；但 OSV 來源已停用（sources.osv_enabled = false），套件不會比對。"
    };
    println!("{message}");
    Ok(ExitCode::Success)
}

fn print_package_report(asset_tag: &str, parsed: &ParsedPackages, report: &PackageImportReport) {
    print_skipped(parsed);
    println!(
        "{asset_tag} 套件匯入完成（{}）：新增 {}、版本變更 {}、移除 {}、未變 {}",
        parsed.format.as_str(),
        report.added,
        report.version_changes.len(),
        report.removed.len(),
        report.unchanged
    );
    for (purl, from, to) in &report.version_changes {
        println!("  {purl}  {from} → {to}");
    }
    for purl in &report.removed {
        println!("  移除 {purl}");
    }
}

fn print_skipped(parsed: &ParsedPackages) {
    let notes = [
        (parsed.skipped_without_purl, "沒有 purl"),
        (parsed.skipped_without_version, "沒有版本"),
        (parsed.duplicates, "重複"),
    ];
    for (count, reason) in notes {
        if count > 0 {
            eprintln!("提醒：略過 {count} 筆{reason}的項目");
        }
    }
}

/// 未加密匯出後的提示（與 VEX 匯出相同的語意）。
const UNENCRYPTED_NOTICE: &str = "注意：此檔未加密，含資產清單，請妥善保管。";

fn export(app: &App, path: &Path) -> Result<ExitCode, CoreError> {
    // 先檢查目的檔，避免開啟資料庫（要求金鑰）後才失敗
    ensure_absent(path)?;
    let catalog = app.load_catalog()?;
    let conn = open_working_db(&app.paths, &app.store)?;
    let summary = export_inventory(&conn, &catalog, path)?;
    println!("已匯出 {} 個資產：{}", summary.assets, path.display());
    for w in &summary.warnings {
        println!("警告：{w}");
    }
    println!("{UNENCRYPTED_NOTICE}");
    println!(
        "編輯此檔後可用 `cvewatch asset import` 匯入；匯入會以檔案內容覆寫資產欄位與清冊元件。"
    );
    Ok(ExitCode::Success)
}

fn template(path: &Path) -> Result<ExitCode, CoreError> {
    write_template(path)?;
    println!("已產生範本：{}", path.display());
    println!("工作表 assets：一列一台設備；components：同一台設備的其他元件（選填）。");
    Ok(ExitCode::Success)
}
