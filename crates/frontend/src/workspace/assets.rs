//! 資產編修（design D9）：表單儲存、退役、恢復、永久刪除、清冊匯出與範本。
//!
//! 寫入一律呼叫 core 的 [`edit`] 操作。編輯既有資產時把表單差異轉成 [`EditOp`]，以
//! [`edit::apply_edits`] 在同一個交易執行：任何一步失敗就全部不寫入。寫入成功的結果標示
//! `needs_match`，由前端決定如何執行比對。

use std::path::{Path, PathBuf};

use cvewatch_core::assets::edit::{self, EditOp, EditOutcome, NewComponent, PurgePreview};
use cvewatch_core::assets::export::export_inventory;
use cvewatch_core::assets::template::write_template;
use cvewatch_core::assets::validate::validate_record;
use cvewatch_core::CoreError;

use super::reload::Refreshed;
use super::Workspace;
use crate::asset_form::{AssetForm, FormMode, UnresolvedAction};
use crate::data::AssetEntry;
use crate::findings::ListView;
use crate::report::{Level, Report};

/// 範本的預設檔名。
pub const TEMPLATE_FILE_NAME: &str = "cvewatch-inventory-template.xlsx";

/// 匯出清冊後的提示（與 VEX 匯出相同）。
pub const UNENCRYPTED_NOTICE: &str = "此檔未加密，含資產清單";

/// 一個步驟與其說明（成功訊息用）。
type Step<'a> = (String, EditOp<'a>);

/// 資產欄位是否與資料庫不同（以與 core 相同的驗證結果比較，忽略前後空白）。
fn fields_changed(form: &AssetForm, entry: &AssetEntry) -> bool {
    match validate_record(&form.record_input()) {
        Ok(record) => record != entry.asset.record,
        // 驗證失敗時交給 core 回報錯誤
        Err(_) => true,
    }
}

/// 移除元件與刪除未辨識產品（先執行：之後可加回同一產品；「至少一個元件」在最後才檢查）。
fn removal_steps(form: &AssetForm) -> Vec<Step<'_>> {
    let components = form
        .components
        .iter()
        .filter(|d| !d.package && d.removed)
        .filter_map(|d| {
            d.id.map(|id| {
                (
                    format!("移除 {}", d.product_key),
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
fn component_steps(form: &AssetForm) -> Vec<Step<'_>> {
    let mut steps = Vec::new();
    for d in form.components.iter().filter(|d| !d.package && !d.removed) {
        let version = d.version.trim().to_string();
        match (d.id, &d.original_version) {
            (None, _) => steps.push((
                format!("加入 {} {version}", d.product_key),
                EditOp::AddComponent(NewComponent {
                    product_key: d.product_key.clone(),
                    version_raw: version,
                }),
            )),
            (Some(id), Some(old)) if old.trim() != version => steps.push((
                format!("{} 版本改為 {version}", d.product_key),
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
                format!("「{}」改為 {key}", u.product_raw),
                EditOp::ResolveUnresolved {
                    unresolved_id: u.id,
                    product_key: key.clone(),
                },
            ));
        }
    }
    steps
}

/// 編輯既有資產時，表單與資料庫目前內容的差異。
fn edit_steps<'a>(form: &'a AssetForm, entry: &AssetEntry) -> Vec<Step<'a>> {
    let mut steps = Vec::new();
    if fields_changed(form, entry) {
        steps.push((
            "修改資產欄位".to_string(),
            EditOp::Fields(form.record_input()),
        ));
    }
    steps.extend(removal_steps(form));
    steps.extend(component_steps(form));
    steps
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

/// 成功訊息；有版本格式警告時為 Warning。
fn saved_message(done: String, outcome: &EditOutcome) -> (String, Level) {
    if outcome.warnings.is_empty() {
        return (done, Level::Info);
    }
    let warnings: Vec<String> = outcome.warnings.iter().map(|w| w.describe()).collect();
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
        if let Err(errors) = form.validate() {
            return Refreshed::unchanged(Report::error(errors.summary()), open);
        }
        let now = self.services.now();
        let tag = form.asset_tag.trim().to_string();
        let catalog = &self.settings.catalog;
        let (done, result) = match form.mode {
            FormMode::New => (
                format!("已新增資產 {tag}"),
                edit::create_asset(
                    &mut self.conn,
                    catalog,
                    &form.record_input(),
                    &new_components(form),
                    now,
                ),
            ),
            FormMode::Edit => {
                let Some(entry) = self.data.asset(&tag) else {
                    let r = Report::error(format!("找不到使用中的資產 {tag}，請重新載入"));
                    return Refreshed::unchanged(r, open);
                };
                let steps = edit_steps(form, entry);
                if steps.is_empty() {
                    return Refreshed::unchanged(Report::info(format!("{tag} 沒有變更")), open);
                }
                let labels: Vec<String> = steps.iter().map(|(l, _)| l.clone()).collect();
                let ops: Vec<EditOp<'_>> = steps.into_iter().map(|(_, op)| op).collect();
                (
                    format!("已儲存 {tag}（{}）", labels.join("、")),
                    edit::apply_edits(&mut self.conn, catalog, &tag, &ops, now),
                )
            }
        };
        match result {
            Ok(outcome) => {
                let (message, level) = saved_message(done, &outcome);
                self.after_asset_write(message, level, open)
            }
            Err(e) => {
                let r = Report::failure(&format!("儲存 {tag} 失敗（未寫入任何變更）"), &e);
                Refreshed::unchanged(r, open)
            }
        }
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
        let day = self
            .services
            .tz
            .format(self.services.now().fixed_offset(), "%Y%m%d");
        (
            self.services.documents_dir.clone(),
            format!("cvewatch-inventory-{day}.xlsx"),
        )
    }

    /// 寫出空白範本（不覆蓋既有檔案）。
    pub fn write_template(&self, path: &Path) -> Report {
        match write_template(path) {
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
