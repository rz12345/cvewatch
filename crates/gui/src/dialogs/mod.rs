//! 對話框：忽略原因與理由（[`reason`]）、備註（[`note`]）、等待區套用目標（[`pending_target`]）、
//! 未儲存變更確認（[`confirm_leave`]）、清冊匯入（[`import`]）、資產編輯（[`asset_edit`]、
//! [`asset_admin`]、[`asset_components`]、產品選擇器 [`product_picker`]）、資產說明管理
//! （[`asset_systems`]）、退役與永久刪除確認（[`purge`]）。
//!
//! 對話框只回傳 [`Outcome`]；寫入由 `GuiApp` 經 `Workspace` 執行。

pub mod asset_admin;
pub mod asset_components;
pub mod asset_edit;
pub mod asset_systems;
pub mod confirm_leave;
pub mod import;
pub mod note;
pub mod pending_target;
pub mod product_picker;
pub mod purge;
pub mod reason;

use cvewatch_core::matching::pending_ops::Action;
use cvewatch_core::matching::triage::{IgnoreReason, Justification, Status};
use cvewatch_frontend::workspace::TriageRequest;
use eframe::egui;

use crate::app::GuiApp;

/// 狀態變更的內容（備註之外）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageDraft {
    pub targets: Vec<i64>,
    pub status: Status,
    pub reason: Option<IgnoreReason>,
    pub justification: Option<Justification>,
}

/// 備註對話框確認後要做的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotePurpose {
    Triage(TriageDraft),
    Pending { cve: String, action: Action },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Reason {
        targets: Vec<i64>,
    },
    Justification {
        targets: Vec<i64>,
    },
    Note {
        purpose: NotePurpose,
        text: String,
        /// 已把焦點移到輸入框
        focused: bool,
    },
    PendingTarget {
        cve: String,
        options: Vec<String>,
    },
}

/// 資產畫面的對話框（寫入由對話框確認後經 `Workspace` 執行）。
pub enum AssetDialog {
    Edit(Box<asset_edit::AssetEdit>),
    /// 「退役…」確認（標為退役）
    Retire {
        tag: String,
    },
    Purge(purge::PurgeConfirm),
    /// 「資產說明…」管理
    Systems(Box<asset_systems::SystemsDialog>),
}

/// 對話框這一幀的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Close,
    Next(Dialog),
    Submit(NotePurpose, String),
}

impl Dialog {
    pub fn note(purpose: NotePurpose) -> Dialog {
        Dialog::Note {
            purpose,
            text: String::new(),
            focused: false,
        }
    }

    /// 備註對話框有尚未送出的內容。
    pub fn has_unsaved_note(&self) -> bool {
        matches!(self, Dialog::Note { text, .. } if !text.trim().is_empty())
    }

    fn show(&mut self, ctx: &egui::Context) -> Outcome {
        match self {
            Dialog::Reason { targets } => reason::reason(ctx, targets),
            Dialog::Justification { targets } => reason::justification(ctx, targets),
            Dialog::Note {
                purpose,
                text,
                focused,
            } => note::show(ctx, purpose, text, focused),
            Dialog::PendingTarget { cve, options } => pending_target::show(ctx, cve, options),
        }
    }
}

/// Esc 鍵（關閉對話框）。
pub(crate) fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

impl GuiApp {
    /// 繪製開啟中的對話框並處理結果。
    pub fn show_dialogs(&mut self, ctx: &egui::Context) {
        // 未儲存確認開啟時只處理它：下層的備註與匯入對話框不繪製，也不接收 Esc
        if self.leave.is_some() {
            self.show_confirm_leave(ctx);
            return;
        }
        if let Some(mut dialog) = self.dialog.take() {
            match dialog.show(ctx) {
                Outcome::Keep => self.dialog = Some(dialog),
                Outcome::Close => {}
                Outcome::Next(next) => self.dialog = Some(next),
                Outcome::Submit(purpose, note) => {
                    // 寫入失敗時保留對話框與備註，讓使用者重試
                    if !self.submit(purpose.clone(), note.clone()) {
                        self.dialog = Some(Dialog::Note {
                            purpose,
                            text: note,
                            focused: true,
                        });
                    }
                }
            }
        }
        if let Some(dialog) = self.asset_dialog.take() {
            match dialog {
                AssetDialog::Edit(edit) => self.show_asset_edit(ctx, edit),
                AssetDialog::Retire { tag } => self.show_retire_confirm(ctx, tag),
                AssetDialog::Purge(p) => self.show_purge_confirm(ctx, p),
                AssetDialog::Systems(d) => self.show_systems_dialog(ctx, d),
            }
        }
        self.show_import(ctx);
    }

    /// 備註確認：經 `Workspace` 寫入並重新載入。寫入失敗時回傳 false。
    pub fn submit(&mut self, purpose: NotePurpose, note: String) -> bool {
        let open = self.open_ids();
        let note = note.trim().to_string();
        let (refreshed, pending) = match purpose {
            NotePurpose::Triage(d) => (
                self.ws.set_status(
                    TriageRequest {
                        targets: d.targets,
                        status: d.status,
                        reason: d.reason,
                        justification: d.justification,
                        note,
                    },
                    &open,
                ),
                None,
            ),
            NotePurpose::Pending { cve, action } => (
                self.ws.resolve_pending(&cve, &action, &note, &open),
                Some(cve),
            ),
        };
        // 寫入失敗為 Error；寫入成功但重新載入失敗為 Warning
        let written = refreshed.report.level != cvewatch_frontend::Level::Error;
        if written && pending.is_some() && self.pending_selected == pending {
            self.pending_selected = None;
        }
        self.apply_refreshed(refreshed);
        written
    }
}
