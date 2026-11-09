//! 清冊匯入（design D8）：選檔或拖放 → 以 companion 執行 `asset import … --dry-run` 顯示預覽 →
//! 按「匯入」以相同參數去掉 `--dry-run` 再執行 → 顯示結果並重新載入。「從 ISMS 清冊匯入」
//! （18 design D8）走同一流程，改用 `asset import-isms`，沒有元件檔與退役選項。

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use cvewatch_frontend::{Level, Report};
use eframe::egui;

use crate::app::{GuiApp, NO_COMPANION};
use crate::companion::{Companion, ImportArgs, ImportKind, Output};
use crate::files::{inventory_kind, InventoryKind};
use crate::screens::ALERT;
use cvewatch_core::assets::pending::PENDING_REMINDER;

/// 子程序結果。
pub type RunResult = Result<Output, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// 預覽中（`--dry-run` 子程序執行中）
    Previewing,
    Preview(RunResult),
    /// 匯入中
    Importing,
    Done(RunResult),
}

pub struct ImportFlow {
    pub args: ImportArgs,
    pub kind: InventoryKind,
    pub stage: Stage,
    rx: Option<Receiver<RunResult>>,
}

impl ImportFlow {
    pub fn running(&self) -> bool {
        matches!(self.stage, Stage::Previewing | Stage::Importing)
    }
}

/// 匯入預覽的覆寫提醒（gui-console：GUI import overwrite reminder）。
pub const OVERWRITE_REMINDER: &str = "注意：匯入會以檔案內容覆寫資產欄位與清冊元件（檔案中沒有的清冊元件會被移除）。     在視窗中修改過資產時，建議先「匯出清冊」再編輯該檔後匯入。";

/// ISMS 匯入預覽的覆寫提醒。
pub const ISMS_OVERWRITE_REMINDER: &str = "注意：只更新 ISMS 欄位（類別、資產說明、C/I/A 與管理欄位）並加入版本待補元件，不會移除元件，也不會將資產標為退役。";

/// 預覽有版本待補元件時的警告（18 design Risks）。
pub const PENDING_WARNING: &str = "有版本待補的元件：匯入後不參與比對，請在編輯對話框補上版本";

/// 預覽輸出是否有版本待補的元件：CLI 以 core 的 [`PENDING_REMINDER`] 整行結尾（整行比對，
/// 資產名稱或備註中出現相同字詞不會誤判）。
fn has_pending(r: &RunResult) -> bool {
    matches!(r, Ok(o) if o.success() && o.stdout.lines().any(|l| l.trim() == PENDING_REMINDER))
}

/// 不支援的檔案或一次多個檔案時的說明。
pub const UNSUPPORTED: &str = "只支援 .xlsx 與 .csv 清冊";

/// CSV 清冊的元件檔：標籤與按鈕。
pub const COMPONENTS_FILE_LABEL: &str = "元件檔：";
pub const PICK_COMPONENTS_BUTTON: &str = "選擇元件檔";

/// CSV 清冊的 IP 檔（20 design D11）：標籤與按鈕。
pub const IP_FILE_LABEL: &str = "IP 檔：";
pub const PICK_IPS_BUTTON: &str = "選擇 IP 檔";

/// 子程序輸出的顯示文字（stdout，失敗時附 stderr 與結束碼）。
pub fn output_text(r: &RunResult) -> String {
    match r {
        Err(e) => e.clone(),
        Ok(o) if o.success() => o.stdout.clone(),
        Ok(o) => {
            let code = o.code.map_or("無".to_string(), |c| c.to_string());
            let mut text = format!("結束碼 {code}");
            for part in [&o.stderr, &o.stdout] {
                if !part.is_empty() {
                    text.push_str("\n\n");
                    text.push_str(part);
                }
            }
            text
        }
    }
}

/// 部分失敗的結束碼：清冊已寫入，但匯入後的自動比對失敗。
const PARTIAL_FAILURE: i32 = cvewatch_core::ExitCode::PartialFailure as i32;

fn succeeded(r: &RunResult) -> bool {
    matches!(r, Ok(o) if o.success())
}

/// 實際匯入已寫入資料庫（成功，或只有自動比對失敗）。
fn wrote_inventory(r: &RunResult) -> bool {
    matches!(r, Ok(o) if o.success() || o.code == Some(PARTIAL_FAILURE))
}

/// 實際匯入的結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportOutcome {
    Done,
    /// 清冊已寫入，自動比對失敗（結束碼 4）
    MatchFailed,
    /// 失敗、無法執行或逾時（資料庫是否變更未知）
    Failed,
}

impl ImportOutcome {
    fn of(r: &RunResult) -> ImportOutcome {
        match r {
            Ok(o) if o.success() => ImportOutcome::Done,
            Ok(o) if o.code == Some(PARTIAL_FAILURE) => ImportOutcome::MatchFailed,
            _ => ImportOutcome::Failed,
        }
    }
}

impl GuiApp {
    /// 開始匯入 `file`（檔案對話框或拖放）。
    pub fn begin_import(&mut self, file: PathBuf) {
        let Some(companion) = self.companion.clone() else {
            return self.report(Report::warning(NO_COMPANION));
        };
        if self.import.as_ref().is_some_and(ImportFlow::running) {
            return self.report(Report::info("清冊匯入仍在執行"));
        }
        let Some(kind) = inventory_kind(&file) else {
            return self.report(Report::warning(format!(
                "{UNSUPPORTED}：{}，未匯入",
                file.display()
            )));
        };
        self.start_flow(ImportKind::Inventory, file, kind, companion);
    }

    /// 開始從 ISMS 清冊匯入（只接受 .xlsx；表頭由 `asset import-isms` 檢查）。
    pub fn begin_isms_import(&mut self, file: PathBuf) {
        let Some(companion) = self.companion.clone() else {
            return self.report(Report::warning(NO_COMPANION));
        };
        if self.import.as_ref().is_some_and(ImportFlow::running) {
            return self.report(Report::info("清冊匯入仍在執行"));
        }
        self.start_flow(ImportKind::Isms, file, InventoryKind::Xlsx, companion);
    }

    fn start_flow(
        &mut self,
        import: ImportKind,
        file: PathBuf,
        kind: InventoryKind,
        companion: Arc<dyn Companion>,
    ) {
        let args = ImportArgs {
            kind: import,
            file,
            components: None,
            ips: None,
            prune: false,
        };
        let mut flow = ImportFlow {
            args,
            kind,
            stage: Stage::Previewing,
            rx: None,
        };
        self.spawn_import(&mut flow, companion, true);
        self.import = Some(flow);
    }

    /// 「從 ISMS 清冊匯入」按鈕：選擇 ISMS 清冊後預覽；取消時不做任何事。
    pub fn pick_isms_workbook(&mut self) {
        match self.dialogs.open_isms_workbook() {
            Some(path) => self.begin_isms_import(path),
            None => self.report(Report::info("已取消從 ISMS 清冊匯入")),
        }
    }

    /// 「匯入清冊」按鈕。
    pub fn pick_inventory(&mut self) {
        if let Some(path) = self.dialogs.open_inventory() {
            self.begin_import(path);
        }
    }

    /// 拖放：只接受單一 .xlsx／.csv。
    pub fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        // 與快捷鍵相同：有對話框開啟時不開始匯入（避免兩層對話框搶輸入）
        if self.modal_open() {
            return self.report(Report::info("請先關閉目前的對話框，再拖放清冊"));
        }
        let paths: Vec<PathBuf> = dropped.iter().map(|f| f.path().to_path_buf()).collect();
        match paths.as_slice() {
            [one] => self.begin_import(one.clone()),
            _ => self.report(Report::warning(format!(
                "{UNSUPPORTED}，一次只能匯入一個檔案"
            ))),
        }
    }

    fn spawn_import(&self, flow: &mut ImportFlow, companion: Arc<dyn Companion>, dry_run: bool) {
        let (tx, rx) = mpsc::channel();
        let args = flow.args.to_args(dry_run);
        self.ws.services.spawner.spawn(Box::new(move || {
            let _ = tx.send(companion.run(&args));
        }));
        flow.rx = Some(rx);
        flow.stage = if dry_run {
            Stage::Previewing
        } else {
            Stage::Importing
        };
    }

    /// 重新預覽（勾選退役或換元件檔後）。
    fn repreview(&mut self, mut flow: ImportFlow) {
        if let Some(companion) = self.companion.clone() {
            self.spawn_import(&mut flow, companion, true);
        }
        self.import = Some(flow);
    }

    pub(crate) fn poll_import(&mut self) {
        let Some(flow) = &mut self.import else {
            return;
        };
        let Some(rx) = &flow.rx else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("清冊匯入異常結束".into()),
        };
        flow.rx = None;
        if flow.stage != Stage::Importing {
            flow.stage = Stage::Preview(result);
            return;
        }
        let outcome = ImportOutcome::of(&result);
        let source = flow.args.kind;
        flow.stage = Stage::Done(result);
        self.after_import(outcome, source);
    }

    /// 匯入步驟結束：一律重新載入（失敗或逾時時資料庫仍可能已寫入，例如交易已提交、
    /// 自動比對仍在執行時被終止），並依結果提示；重新載入失敗時保留其訊息。
    fn after_import(&mut self, outcome: ImportOutcome, source: ImportKind) {
        let open = self.open_ids();
        let mut refreshed = self.ws.reload(&open);
        let reloaded = refreshed.report.level == Level::Info;
        // 匯入種類決定訊息用語；ISMS 匯入成功時另提醒補上版本
        let (imported, failed, hint) = match source {
            ImportKind::Inventory => ("已匯入清冊", "清冊匯入失敗", ""),
            ImportKind::Isms => (
                "已從 ISMS 清冊匯入",
                "ISMS 清冊匯入失敗",
                "；版本待補的元件請在編輯對話框補上版本",
            ),
        };
        refreshed.report = match (outcome, reloaded) {
            (ImportOutcome::Done, true) => Report::info(format!("{imported}並重新載入{hint}")),
            (ImportOutcome::MatchFailed, true) => Report::warning(format!(
                "{imported}並重新載入，但匯入後的自動比對失敗；請執行 `cvewatch match`（詳見匯入視窗）{hint}"
            )),
            (ImportOutcome::Failed, true) => Report::error(format!(
                "{failed}，詳見匯入視窗；已重新載入以顯示資料庫目前的狀態"
            )),
            (ImportOutcome::Failed, false) => Report {
                message: format!("{failed}，詳見匯入視窗；{}", refreshed.report.message),
                level: Level::Error,
                ..refreshed.report.clone()
            },
            (_, false) => Report {
                message: format!("{imported}，但{}", refreshed.report.message),
                ..refreshed.report.clone()
            },
        };
        self.apply_refreshed(refreshed);
    }

    pub(super) fn show_import(&mut self, ctx: &egui::Context) {
        let Some(mut flow) = self.import.take() else {
            return;
        };
        let mut action = None;
        egui::Modal::new(egui::Id::unique("dialog-import")).show(ctx, |ui| {
            action = body(ui, &mut flow);
        });
        match action {
            None => self.import = Some(flow),
            Some(Action::Close) => {}
            Some(Action::Import) => {
                if let Some(companion) = self.companion.clone() {
                    self.spawn_import(&mut flow, companion, false);
                }
                self.import = Some(flow);
            }
            Some(Action::Repreview) => self.repreview(flow),
            Some(Action::PickFile(side)) => {
                let picked = match side {
                    SideFile::Components => self.dialogs.open_components(),
                    SideFile::Ips => self.dialogs.open_ips(),
                };
                if let Some(path) = picked {
                    *side.slot(&mut flow.args) = Some(path);
                    self.repreview(flow);
                } else {
                    self.import = Some(flow);
                }
            }
        }
    }
}

enum Action {
    Close,
    Import,
    Repreview,
    PickFile(SideFile),
}

/// CSV 清冊另外指定的檔案（元件檔、IP 檔）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SideFile {
    Components,
    Ips,
}

impl SideFile {
    const ALL: [SideFile; 2] = [SideFile::Components, SideFile::Ips];

    fn label(self) -> &'static str {
        match self {
            SideFile::Components => COMPONENTS_FILE_LABEL,
            SideFile::Ips => IP_FILE_LABEL,
        }
    }

    fn button(self) -> &'static str {
        match self {
            SideFile::Components => PICK_COMPONENTS_BUTTON,
            SideFile::Ips => PICK_IPS_BUTTON,
        }
    }

    fn slot(self, args: &mut ImportArgs) -> &mut Option<PathBuf> {
        match self {
            SideFile::Components => &mut args.components,
            SideFile::Ips => &mut args.ips,
        }
    }
}

fn body(ui: &mut egui::Ui, flow: &mut ImportFlow) -> Option<Action> {
    let isms = flow.args.kind == ImportKind::Isms;
    let (title, reminder) = if isms {
        ("從 ISMS 清冊匯入", ISMS_OVERWRITE_REMINDER)
    } else {
        ("匯入資產清冊", OVERWRITE_REMINDER)
    };
    ui.heading(title);
    ui.colored_label(egui::Color32::from_rgb(0xc0, 0x80, 0x00), reminder);
    ui.label(format!("檔案：{}", flow.args.file.display()));
    let mut action = None;
    let idle = !flow.running();
    if !isms && flow.kind == InventoryKind::Csv {
        let can_pick = idle && !matches!(flow.stage, Stage::Done(_));
        for side in SideFile::ALL {
            ui.horizontal(|ui| {
                let shown = side
                    .slot(&mut flow.args)
                    .as_ref()
                    .map_or("（未選擇）".to_string(), |p| p.display().to_string());
                ui.label(format!("{}{shown}", side.label()));
                if ui
                    .add_enabled(can_pick, egui::Button::new(side.button()))
                    .clicked()
                {
                    action = Some(Action::PickFile(side));
                }
            });
        }
    }
    // ISMS 匯入不提供退役選項（18 spec GUI ISMS import）
    if !isms {
        let can_change = idle && matches!(flow.stage, Stage::Preview(_));
        let prune = ui.add_enabled(
            can_change,
            egui::Checkbox::new(&mut flow.args.prune, "將清冊中消失的資產標為退役"),
        );
        if prune.changed() {
            action = Some(Action::Repreview);
        }
    }
    ui.separator();
    match &flow.stage {
        Stage::Previewing => {
            ui.label("預覽中…（資料庫尚未變更）");
        }
        Stage::Importing => {
            ui.label("匯入中…");
        }
        Stage::Preview(r) => {
            ui.label(if succeeded(r) {
                "預覽（資料庫尚未變更；匯入時會重新讀取檔案）"
            } else {
                "無法預覽："
            });
            if has_pending(r) {
                ui.colored_label(ALERT, PENDING_WARNING);
            }
            output(ui, r);
        }
        Stage::Done(r) => {
            ui.label(if succeeded(r) {
                "匯入完成"
            } else if wrote_inventory(r) {
                "清冊已匯入，但自動比對失敗："
            } else {
                "匯入失敗："
            });
            output(ui, r);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        let ready = matches!(&flow.stage, Stage::Preview(r) if succeeded(r));
        if !matches!(flow.stage, Stage::Done(_))
            && ui.add_enabled(ready, egui::Button::new("匯入")).clicked()
        {
            action = Some(Action::Import);
        }
        let close = if matches!(flow.stage, Stage::Done(_)) {
            "關閉"
        } else {
            "取消"
        };
        // 預覽中可以取消（--dry-run 不寫入）；實際匯入中不可取消，由逾時保護
        let can_close = flow.stage != Stage::Importing;
        if ui
            .add_enabled(can_close, egui::Button::new(close))
            .clicked()
        {
            action = Some(Action::Close);
        }
    });
    action
}

fn output(ui: &mut egui::Ui, r: &RunResult) {
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(output_text(r)).monospace()).wrap());
        });
}
