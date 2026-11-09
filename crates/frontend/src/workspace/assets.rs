//! 資產編修（design D9）：表單儲存、退役、恢復、永久刪除、清冊匯出與範本。
//!
//! 寫入一律呼叫 core 的 [`edit`] 操作。編輯既有資產時把表單差異轉成 [`EditOp`]，以
//! [`edit::apply_edits`] 在同一個交易執行：任何一步失敗就全部不寫入。寫入成功的結果標示
//! `needs_match`，由前端決定如何執行比對。

use std::path::{Path, PathBuf};

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::edit::{
    self, only_lifecycle_changed, EditOp, EditOutcome, NewComponent, NewLifecycle, PurgePreview,
};
use cvewatch_core::assets::export::export_inventory;
use cvewatch_core::assets::plan::VersionWarning;
use cvewatch_core::assets::template::write_template;
use cvewatch_core::assets::validate::{validate_record, RecordContext};
use cvewatch_core::assets::AssetRecord;
use cvewatch_core::catalog::Catalog;
use cvewatch_core::CoreError;

use super::reload::Refreshed;
use super::Workspace;
use crate::asset_form::{
    isms_conflict, property_number_conflict, rename_conflict, AssetForm, FormErrors,
    UnresolvedAction,
};
use crate::data::AssetEntry;
use crate::display::product_name;
use crate::findings::ListView;
use crate::report::{Level, Report};

/// 範本的預設檔名。
pub const TEMPLATE_FILE_NAME: &str = "cvewatch-inventory-template.xlsx";

/// 匯出清冊後的提示（與 VEX 匯出相同）。
pub const UNENCRYPTED_NOTICE: &str = "此檔未加密，含資產清單";

/// 一個步驟與其說明（成功訊息用；產品以顯示名稱呈現）。
type Step<'a> = (String, EditOp<'a>);

/// 資產欄位與資料庫的差異（以與 core 相同的驗證結果比較，忽略前後空白）：None 為沒有變更，
/// `Some(true)` 為需要重新比對（不只改生命週期日期；22 design D11）。編號另由更名步驟處理，
/// 比較時換回原編號（12 design D3）；硬體 EoL 覆寫另由 [`EditOp::SetHardwareEol`] 處理。
fn fields_change(form: &AssetForm, entry: &AssetEntry, ctx: &RecordContext<'_>) -> Option<bool> {
    match validate_record(&form.record_input(), ctx) {
        Ok(record) => {
            let old = &entry.asset.record;
            let record = AssetRecord {
                asset_tag: old.asset_tag.clone(),
                hw_eol_override: old.hw_eol_override.clone(),
                ..record
            };
            (record != *old).then(|| !only_lifecycle_changed(old, &record))
        }
        // 驗證失敗時交給 core 回報錯誤
        Err(_) => Some(true),
    }
}

/// EoL 覆寫的變更（硬體與各清冊元件；本地驗證已通過）。
fn eol_steps<'a>(
    form: &'a AssetForm,
    entry: Option<&AssetEntry>,
    catalog: &Catalog,
) -> Vec<Step<'a>> {
    let mut steps = Vec::new();
    let hardware = form.lifecycle.hardware_override().ok().flatten();
    let old = entry.and_then(|e| e.asset.record.hw_eol_override.clone());
    if hardware != old {
        let label = if hardware.is_some() {
            "設定硬體 EoL 覆寫"
        } else {
            "清除硬體 EoL 覆寫"
        };
        steps.push((label.to_string(), EditOp::SetHardwareEol(hardware)));
    }
    for d in form.components.iter().filter(|d| !d.package && !d.removed) {
        if let Some(value) = d.eol.change() {
            let verb = if value.is_some() { "設定" } else { "清除" };
            steps.push((
                format!("{verb} {} 的 EoL 覆寫", d.name(catalog)),
                EditOp::SetComponentEol {
                    product_key: d.product_key.clone(),
                    value,
                },
            ));
        }
    }
    steps
}

/// 移除元件與刪除未辨識產品（先執行：之後可加回同一產品；「至少一個元件」在最後才檢查）。
fn removal_steps<'a>(form: &'a AssetForm, catalog: &Catalog) -> Vec<Step<'a>> {
    let components = form
        .components
        .iter()
        .filter(|d| !d.package && d.removed)
        .filter_map(|d| {
            d.id.map(|id| {
                (
                    format!("移除 {}", d.name(catalog)),
                    EditOp::RemoveComponent(id),
                )
            })
        });
    let unresolved = form
        .unresolved
        .iter()
        .filter(|u| u.action == UnresolvedAction::Delete)
        .map(|u| {
            (
                format!("刪除未辨識產品「{}」", u.product_raw),
                EditOp::DeleteUnresolved(u.id),
            )
        });
    components.chain(unresolved).collect()
}

/// 新元件、版本變更與未辨識產品改為目錄產品。
fn component_steps<'a>(form: &'a AssetForm, catalog: &Catalog) -> Vec<Step<'a>> {
    let mut steps = Vec::new();
    for d in form.components.iter().filter(|d| !d.package && !d.removed) {
        let version = d.version.trim().to_string();
        match (d.id, &d.original_version) {
            (None, _) => steps.push((
                format!("加入 {} {version}", d.name(catalog)),
                EditOp::AddComponent(NewComponent {
                    product_key: d.product_key.clone(),
                    version_raw: version,
                }),
            )),
            (Some(id), Some(old)) if old.trim() != version => steps.push((
                format!("{} 版本改為 {version}", d.name(catalog)),
                EditOp::SetVersion {
                    component_id: id,
                    version_raw: version,
                },
            )),
            _ => {}
        }
    }
    for u in &form.unresolved {
        if let UnresolvedAction::MapTo(key) = &u.action {
            steps.push((
                format!("「{}」改為 {}", u.product_raw, product_name(catalog, key)),
                EditOp::ResolveUnresolved {
                    unresolved_id: u.id,
                    product_key: key.clone(),
                },
            ));
        }
    }
    steps
}

/// 編輯既有資產時，表單與資料庫目前內容的差異，與這些步驟是否需要重新比對（修改欄位只改
/// 生命週期日期時不需要；其他步驟依 [`EditOp::affects_matching`]）。
fn edit_steps<'a>(
    form: &'a AssetForm,
    entry: &AssetEntry,
    catalog: &Catalog,
    ctx: &RecordContext<'_>,
) -> (Vec<Step<'a>>, bool) {
    let mut steps = Vec::new();
    let mut needs_match = false;
    if let Some(new_tag) = form.renamed_to() {
        let old = &entry.asset.record.asset_tag;
        steps.push((format!("更名 {old} → {new_tag}"), EditOp::Rename(new_tag)));
    }
    if let Some(affects) = fields_change(form, entry, ctx) {
        needs_match = affects;
        let label = if affects {
            "修改資產欄位"
        } else {
            "修改生命週期日期"
        };
        steps.push((
            label.to_string(),
            EditOp::Fields(Box::new(form.record_input())),
        ));
    }
    // IP 不影響比對（EditOp::affects_matching 為 false），只改 IP 時不需要重新比對
    if form.ips_changed(&entry.ips) {
        steps.push(("修改 IP".to_string(), EditOp::SetIps(form.ip_inputs())));
    }
    steps.extend(removal_steps(form, catalog));
    steps.extend(component_steps(form, catalog));
    // 覆寫放在最後：新加入的元件也可以同次設定覆寫
    steps.extend(eol_steps(form, Some(entry), catalog));
    needs_match |= steps
        .iter()
        .any(|(_, op)| !matches!(op, EditOp::Fields(_)) && op.affects_matching());
    (steps, needs_match)
}

/// 新增資產時一併寫入的 EoL 覆寫。
fn new_lifecycle(form: &AssetForm) -> NewLifecycle {
    NewLifecycle {
        hardware: form.lifecycle.hardware_override().ok().flatten(),
        components: form
            .components
            .iter()
            .filter(|d| !d.package && !d.removed)
            .filter_map(|d| {
                let value = d.eol.parsed().ok().flatten()?;
                Some((d.product_key.clone(), value))
            })
            .collect(),
    }
}

fn new_components(form: &AssetForm) -> Vec<NewComponent> {
    form.components
        .iter()
        .filter(|d| !d.package && !d.removed)
        .map(|d| NewComponent {
            product_key: d.product_key.clone(),
            version_raw: d.version.trim().to_string(),
        })
        .collect()
}

/// 版本格式警告（與 core 的 `describe` 相同，但產品用顯示名稱）。
fn describe_warning(w: &VersionWarning, catalog: &Catalog) -> String {
    w.describe_with(&product_name(catalog, &w.product_key))
}

/// 成功訊息；有版本格式警告時為 Warning。
fn saved_message(done: String, outcome: &EditOutcome, catalog: &Catalog) -> (String, Level) {
    if outcome.warnings.is_empty() {
        return (done, Level::Info);
    }
    let warnings: Vec<String> = outcome
        .warnings
        .iter()
        .map(|w| describe_warning(w, catalog))
        .collect();
    (
        format!(
            "{done}；版本無法解析（仍保存原始字串）：{}",
            warnings.join("、")
        ),
        Level::Warning,
    )
}

impl<V: ListView> Workspace<V> {
    /// 儲存資產表單（新增，或依差異在同一交易編修）。表單先以本地驗證檢查；
    /// 回傳的 `Refreshed.report` 為 Error 時表示沒有寫入（對話框應保留）。
    pub fn save_asset_form(&mut self, form: &AssetForm, open: &[i64]) -> Refreshed {
        if let Err(errors) = self.validate_asset_form(form) {
            return Refreshed::unchanged(Report::error(errors.summary()), open);
        }
        match form.original_tag() {
            None => self.save_new_asset(form, open),
            Some(original) => self.save_asset_edits(form, original, open),
        }
    }

    /// 寫入前的本地驗證：欄位與元件（[`AssetForm::validate`]），以及更名的新編號是否已被使用。
    pub fn validate_asset_form(&self, form: &AssetForm) -> Result<(), FormErrors> {
        let mut errors = form
            .validate(
                &self.settings.catalog,
                &self.settings.classes,
                &self.data.systems,
            )
            .err()
            .unwrap_or_default();
        let tag_ok = errors.field(StandardField::AssetTag).is_none();
        if let Some(msg) = rename_conflict(form, &self.data).filter(|_| tag_ok) {
            errors.fields.insert(0, (StandardField::AssetTag, msg));
        }
        let isms_ok = errors.field(StandardField::IsmsId).is_none();
        if let Some(msg) = isms_conflict(form, &self.data).filter(|_| isms_ok) {
            errors.fields.push((StandardField::IsmsId, msg));
        }
        let property_ok = errors.field(StandardField::PropertyNumber).is_none();
        if let Some(msg) = property_number_conflict(form, &self.data).filter(|_| property_ok) {
            errors.fields.push((StandardField::PropertyNumber, msg));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn save_new_asset(&mut self, form: &AssetForm, open: &[i64]) -> Refreshed {
        let tag = form.asset_tag.trim().to_string();
        let result = edit::create_asset_with(
            &mut self.conn,
            &self.settings.catalog,
            &self.settings.classes,
            &form.record_input(),
            &new_components(form),
            &form.ip_inputs(),
            &new_lifecycle(form),
            self.services.now(),
        );
        self.after_save(format!("已新增資產 {tag}"), &tag, result, open)
    }

    /// 編輯既有資產：以原編號找資產，把差異轉成步驟，在同一交易寫入。是否需要比對由 core 的
    /// [`EditOp::affects_matching`] 判斷（只更名時不需要，12 design D2）。
    fn save_asset_edits(&mut self, form: &AssetForm, original: &str, open: &[i64]) -> Refreshed {
        let Some(entry) = self.data.asset(original) else {
            let r = Report::error(format!("找不到使用中的資產 {original}，請重新載入"));
            return Refreshed::unchanged(r, open);
        };
        let catalog = &self.settings.catalog;
        let classes = &self.settings.classes;
        let ctx = RecordContext {
            classes,
            systems: &self.data.systems,
        };
        let (steps, needs_match) = edit_steps(form, entry, catalog, &ctx);
        if steps.is_empty() {
            return Refreshed::unchanged(Report::info(format!("{original} 沒有變更")), open);
        }
        let shown = form.renamed_to().unwrap_or(original).to_string();
        let labels: Vec<String> = steps.iter().map(|(l, _)| l.clone()).collect();
        let done = format!("已儲存 {shown}（{}）", labels.join("、"));
        let ops: Vec<EditOp<'_>> = steps.into_iter().map(|(_, op)| op).collect();
        let result = edit::apply_edits(
            &mut self.conn,
            catalog,
            classes,
            original,
            &ops,
            self.services.now(),
        );
        let mut r = self.after_save(done, original, result, open);
        r.needs_match &= needs_match;
        r
    }

    /// 寫入結果：成功時重新載入並標示需要比對；失敗時不重新載入（沒有寫入任何變更）。
    fn after_save(
        &mut self,
        done: String,
        tag: &str,
        result: Result<EditOutcome, CoreError>,
        open: &[i64],
    ) -> Refreshed {
        match result {
            Ok(outcome) => {
                let (message, level) = saved_message(done, &outcome, &self.settings.catalog);
                self.after_asset_write(message, level, open)
            }
            Err(e) => self.save_failed(tag, &e, open),
        }
    }

    fn save_failed(&self, tag: &str, e: &CoreError, open: &[i64]) -> Refreshed {
        let r = Report::failure(&format!("儲存 {tag} 失敗（未寫入任何變更）"), e);
        Refreshed::unchanged(r, open)
    }

    /// 寫入後重新載入並標示需要比對；重新載入失敗時保留其警告。
    fn after_asset_write(&mut self, done: String, level: Level, open: &[i64]) -> Refreshed {
        let mut r = self.after_write(done, open);
        if r.report.level == Level::Info {
            r.report.level = level;
        }
        r.needs_match = true;
        r
    }

    /// 單一操作：成功時重新載入，失敗時回報「{what}失敗：…」。
    fn single(
        &mut self,
        what: &str,
        done: String,
        result: Result<EditOutcome, CoreError>,
        open: &[i64],
    ) -> Refreshed {
        match result {
            Ok(_) => self.after_asset_write(done, Level::Info, open),
            Err(e) => Refreshed::unchanged(Report::failure(&format!("{what}失敗"), &e), open),
        }
    }

    /// 刪除 = 標為退役（保留命中與處理紀錄、不再比對、可恢復）。
    pub fn retire_asset(&mut self, tag: &str, open: &[i64]) -> Refreshed {
        let r = edit::retire_asset(&mut self.conn, tag, self.services.now());
        let done = format!("已將 {tag} 標為退役（命中與處理紀錄保留，可在「顯示已退役」中恢復）");
        self.single(&format!("將 {tag} 標為退役"), done, r, open)
    }

    pub fn restore_asset(&mut self, tag: &str, open: &[i64]) -> Refreshed {
        let r = edit::restore_asset(&mut self.conn, tag, self.services.now());
        self.single(&format!("恢復 {tag}"), format!("已恢復 {tag}"), r, open)
    }

    /// 永久刪除前的預覽（元件數、命中數）。
    pub fn purge_preview(&self, tag: &str) -> Result<PurgePreview, Report> {
        edit::purge_preview(&self.conn, tag).map_err(|e| Report::failure("無法永久刪除", &e))
    }

    /// 永久刪除已退役資產（無法復原）。
    pub fn purge_asset(&mut self, tag: &str, open: &[i64]) -> Refreshed {
        match edit::purge_asset(&mut self.conn, tag, self.services.now()) {
            Ok(p) => self.after_asset_write(
                format!(
                    "已永久刪除 {tag}（元件 {} 個、命中 {} 筆）",
                    p.components, p.findings
                ),
                Level::Info,
                open,
            ),
            Err(e) => Refreshed::unchanged(Report::failure("永久刪除失敗", &e), open),
        }
    }
}

impl<V> Workspace<V> {
    /// 「下載範本」對話框的預設資料夾（「文件」）與檔名。
    pub fn template_default(&self) -> (PathBuf, String) {
        (
            self.services.documents_dir.clone(),
            TEMPLATE_FILE_NAME.to_string(),
        )
    }

    /// 「匯出清冊」的預設資料夾與檔名（含顯示時區的當天日期）。
    pub fn inventory_export_default(&self) -> (PathBuf, String) {
        self.dated_xlsx_default("cvewatch-inventory")
    }

    /// 另存 xlsx 的預設資料夾（「文件」）與檔名 `<prefix>-YYYYMMDD.xlsx`（顯示時區的當天日期）。
    pub(super) fn dated_xlsx_default(&self, prefix: &str) -> (PathBuf, String) {
        let day = self
            .services
            .tz
            .format(self.services.now().fixed_offset(), "%Y%m%d");
        (
            self.services.documents_dir.clone(),
            format!("{prefix}-{day}.xlsx"),
        )
    }

    /// 寫出空白範本（不覆蓋既有檔案）。
    pub fn write_template(&self, path: &Path) -> Report {
        match write_template(path, &self.settings.classes) {
            Ok(()) => Report::info(format!("已產生範本：{}", path.display())),
            Err(e) => Report::failure("無法產生範本", &e),
        }
    }

    /// 匯出使用中資產（與 `cvewatch asset export` 相同；不覆蓋既有檔案）。
    pub fn export_inventory(&self, path: &Path) -> Report {
        match export_inventory(&self.conn, &self.settings.catalog, path) {
            Ok(s) => {
                let done = format!(
                    "已匯出 {} 個資產至 {}（{UNENCRYPTED_NOTICE}）",
                    s.assets,
                    path.display()
                );
                if s.warnings.is_empty() {
                    Report::info(done)
                } else {
                    Report::warning(format!("{done}；{}", s.warnings.join("；")))
                }
            }
            Err(e) => Report::failure("匯出清冊失敗", &e),
        }
    }
}
