//! 對話框：忽略原因／理由選單、備註輸入、連結選單、篩選面板、等待區套用目標、離開確認。
//!
//! 狀態轉移是純函式：`Dialog::handle(self, key)` 回傳下一個狀態或要執行的命令。

use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::pending_ops::Action;
use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::{IgnoreReason, Justification, Status};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use tui_input::{Input, InputRequest};

use crate::filter::Filter;
use crate::links::Link;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotePurpose {
    Triage {
        targets: Vec<i64>,
        status: Status,
        reason: Option<IgnoreReason>,
        justification: Option<Justification>,
    },
    Pending {
        cve: String,
        action: Action,
    },
}

#[derive(Debug, Clone)]
pub enum Dialog {
    Reason {
        targets: Vec<i64>,
        cursor: usize,
    },
    Justification {
        targets: Vec<i64>,
        cursor: usize,
    },
    Note {
        purpose: NotePurpose,
        input: Input,
    },
    Links {
        links: Vec<Link>,
        cursor: usize,
    },
    Filter {
        draft: Filter,
        assets: Vec<String>,
        cursor: usize,
    },
    PendingTarget {
        cve: String,
        options: Vec<String>,
        cursor: usize,
    },
    /// 匯出 VEX：確認或修改輸出路徑（`count` 為目前篩選的命中數）
    ExportVex {
        input: Input,
        count: usize,
    },
    ConfirmQuit {
        back: Box<Dialog>,
    },
}

/// 變更的原因與理由說明，例如「（判定不受影響，需特定設定）」；沒有原因時為空字串。
pub fn change_detail(reason: Option<IgnoreReason>, justification: Option<Justification>) -> String {
    match (reason, justification) {
        (Some(r), Some(j)) => format!("（{}，{}）", r.label(), j.label()),
        (Some(r), None) => format!("（{}）", r.label()),
        _ => String::new(),
    }
}

/// 對話框送出的命令（由 App 執行）。
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Triage {
        purpose: NotePurpose,
        note: String,
    },
    ResolvePending {
        cve: String,
        action: Action,
        note: String,
    },
    Open(String),
    ApplyFilter(Filter),
    /// 以目前篩選匯出 VEX 到指定路徑
    ExportVex(String),
    Quit,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Keep(Dialog),
    Close,
    Submit(Command),
}

/// 篩選面板的列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterRow {
    Status(Status),
    Priority(Priority),
    Confidence,
    Kev,
    Asset,
    Reset,
}

pub fn filter_rows() -> Vec<FilterRow> {
    Status::ALL
        .iter()
        .map(|s| FilterRow::Status(*s))
        .chain(Priority::ALL.iter().map(|p| FilterRow::Priority(*p)))
        .chain([
            FilterRow::Confidence,
            FilterRow::Kev,
            FilterRow::Asset,
            FilterRow::Reset,
        ])
        .collect()
}

/// 上下移動（`j`/`k` 與方向鍵），不循環。
pub fn moved(cursor: usize, len: usize, code: KeyCode) -> usize {
    let last = len.saturating_sub(1);
    match code {
        KeyCode::Up | KeyCode::Char('k') => cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => (cursor + 1).min(last),
        KeyCode::PageUp => cursor.saturating_sub(10),
        KeyCode::PageDown => (cursor + 10).min(last),
        KeyCode::Home | KeyCode::Char('g') => 0,
        KeyCode::End | KeyCode::Char('G') => last,
        _ => cursor.min(last),
    }
}

pub fn note_dialog(purpose: NotePurpose) -> Dialog {
    Dialog::Note {
        purpose,
        input: Input::default(),
    }
}

fn triage_note(
    targets: Vec<i64>,
    reason: IgnoreReason,
    justification: Option<Justification>,
) -> Dialog {
    note_dialog(NotePurpose::Triage {
        targets,
        status: Status::Ignored,
        reason: Some(reason),
        justification,
    })
}

fn input_request(key: KeyEvent) -> Option<InputRequest> {
    Some(match key.code {
        KeyCode::Char(c) => InputRequest::InsertChar(c),
        KeyCode::Backspace => InputRequest::DeletePrevChar,
        KeyCode::Delete => InputRequest::DeleteNextChar,
        KeyCode::Left => InputRequest::GoToPrevChar,
        KeyCode::Right => InputRequest::GoToNextChar,
        KeyCode::Home => InputRequest::GoToStart,
        KeyCode::End => InputRequest::GoToEnd,
        _ => return None,
    })
}

/// 文字輸入框處理一個按鍵（回傳新的輸入狀態）。
pub fn edit(input: &Input, key: KeyEvent) -> Input {
    let mut next = input.clone();
    if let Some(req) = input_request(key) {
        next.handle(req);
    }
    next
}

impl Dialog {
    /// 有尚未送出的備註文字。
    pub fn has_unsaved_note(&self) -> bool {
        match self {
            Dialog::Note { input, .. } => !input.value().trim().is_empty(),
            Dialog::ConfirmQuit { back } => back.has_unsaved_note(),
            _ => false,
        }
    }

    pub fn handle(self, key: KeyEvent) -> Outcome {
        if key.code == KeyCode::Esc {
            return match self {
                Dialog::ConfirmQuit { back } => Outcome::Keep(*back),
                _ => Outcome::Close,
            };
        }
        match self {
            Dialog::Reason { targets, cursor } => reason_key(targets, cursor, key.code),
            Dialog::Justification { targets, cursor } => {
                justification_key(targets, cursor, key.code)
            }
            Dialog::Note { purpose, input } => note_key(purpose, input, key),
            Dialog::Links { links, cursor } => match key.code {
                KeyCode::Enter => links.get(cursor).map_or(Outcome::Close, |l| {
                    Outcome::Submit(Command::Open(l.url.clone()))
                }),
                code => Outcome::Keep(Dialog::Links {
                    cursor: moved(cursor, links.len(), code),
                    links,
                }),
            },
            Dialog::Filter {
                draft,
                assets,
                cursor,
            } => filter_key(draft, assets, cursor, key.code),
            Dialog::PendingTarget {
                cve,
                options,
                cursor,
            } => target_key(cve, options, cursor, key.code),
            Dialog::ExportVex { input, count } => match key.code {
                KeyCode::Enter => {
                    Outcome::Submit(Command::ExportVex(input.value().trim().to_string()))
                }
                _ => Outcome::Keep(Dialog::ExportVex {
                    input: edit(&input, key),
                    count,
                }),
            },
            Dialog::ConfirmQuit { back } => match key.code {
                KeyCode::Char('y' | 'Y') => Outcome::Submit(Command::Quit),
                _ => Outcome::Keep(*back),
            },
        }
    }
}

fn reason_key(targets: Vec<i64>, cursor: usize, code: KeyCode) -> Outcome {
    let reasons = IgnoreReason::ALL;
    match code {
        KeyCode::Enter => match reasons[cursor.min(reasons.len() - 1)] {
            IgnoreReason::NotAffected => {
                Outcome::Keep(Dialog::Justification { targets, cursor: 0 })
            }
            reason => Outcome::Keep(triage_note(targets, reason, None)),
        },
        code => Outcome::Keep(Dialog::Reason {
            cursor: moved(cursor, reasons.len(), code),
            targets,
        }),
    }
}

fn justification_key(targets: Vec<i64>, cursor: usize, code: KeyCode) -> Outcome {
    let all = Justification::ALL;
    match code {
        KeyCode::Enter => Outcome::Keep(triage_note(
            targets,
            IgnoreReason::NotAffected,
            Some(all[cursor.min(all.len() - 1)]),
        )),
        code => Outcome::Keep(Dialog::Justification {
            cursor: moved(cursor, all.len(), code),
            targets,
        }),
    }
}

fn note_key(purpose: NotePurpose, input: Input, key: KeyEvent) -> Outcome {
    if key.code != KeyCode::Enter {
        return Outcome::Keep(Dialog::Note {
            input: edit(&input, key),
            purpose,
        });
    }
    let note = input.value().trim().to_string();
    Outcome::Submit(match purpose {
        NotePurpose::Pending { cve, action } => Command::ResolvePending { cve, action, note },
        triage @ NotePurpose::Triage { .. } => Command::Triage {
            purpose: triage,
            note,
        },
    })
}

fn target_key(cve: String, options: Vec<String>, cursor: usize, code: KeyCode) -> Outcome {
    match code {
        KeyCode::Enter => match options.get(cursor) {
            Some(target) => Outcome::Keep(note_dialog(NotePurpose::Pending {
                cve,
                action: Action::Apply(target.clone()),
            })),
            None => Outcome::Close,
        },
        code => Outcome::Keep(Dialog::PendingTarget {
            cursor: moved(cursor, options.len(), code),
            cve,
            options,
        }),
    }
}

/// 循環選擇：`None`（全部）→ 第一個 → … → 最後一個 → `None`。
fn cycle<T: Clone + PartialEq>(current: &Option<T>, items: &[T], forward: bool) -> Option<T> {
    let pos = current
        .as_ref()
        .and_then(|c| items.iter().position(|x| x == c));
    let len = items.len();
    let next = match (pos, forward) {
        (None, true) => (len > 0).then_some(0),
        (None, false) => len.checked_sub(1),
        (Some(i), true) => (i + 1 < len).then_some(i + 1),
        (Some(i), false) => i.checked_sub(1),
    };
    next.map(|i| items[i].clone())
}

fn changed_filter(draft: &Filter, row: FilterRow, assets: &[String], forward: bool) -> Filter {
    match row {
        FilterRow::Status(s) => draft.with_status_toggled(s),
        FilterRow::Priority(p) => draft.with_priority_toggled(p),
        FilterRow::Confidence => {
            draft.with_confidence(cycle(&draft.confidence, Confidence::ALL, forward))
        }
        FilterRow::Kev => draft.with_kev(draft.kev.next()),
        FilterRow::Asset => draft.with_asset(cycle(&draft.asset, assets, forward)),
        FilterRow::Reset => Filter::default(),
    }
}

fn filter_key(draft: Filter, assets: Vec<String>, cursor: usize, code: KeyCode) -> Outcome {
    let rows = filter_rows();
    let row = rows[cursor.min(rows.len() - 1)];
    let change = match code {
        KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l') => Some(true),
        KeyCode::Left => Some(false),
        KeyCode::Enter => return Outcome::Submit(Command::ApplyFilter(draft)),
        _ => None,
    };
    match change {
        Some(forward) => Outcome::Keep(Dialog::Filter {
            draft: changed_filter(&draft, row, &assets, forward),
            assets,
            cursor,
        }),
        None => Outcome::Keep(Dialog::Filter {
            cursor: moved(cursor, rows.len(), code),
            draft,
            assets,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn keep(o: Outcome) -> Dialog {
        match o {
            Outcome::Keep(d) => d,
            other => panic!("expected Keep, got {other:?}"),
        }
    }

    fn type_text(mut d: Dialog, text: &str) -> Dialog {
        for c in text.chars() {
            d = keep(d.handle(key(KeyCode::Char(c))));
        }
        d
    }

    #[test]
    fn ignore_flow_not_affected_requires_justification() {
        let d = Dialog::Reason {
            targets: vec![1, 2, 3],
            cursor: 0,
        };
        let d = keep(d.handle(key(KeyCode::Enter)));
        assert!(matches!(d, Dialog::Justification { .. }));
        let d = keep(d.handle(key(KeyCode::Down)));
        let d = keep(d.handle(key(KeyCode::Down)));
        let d = keep(d.handle(key(KeyCode::Enter)));
        let d = type_text(d, "未啟用 GP");
        assert!(d.has_unsaved_note());
        let Outcome::Submit(Command::Triage { purpose, note }) = d.handle(key(KeyCode::Enter))
        else {
            panic!("expected submit");
        };
        assert_eq!(note, "未啟用 GP");
        assert_eq!(
            purpose,
            NotePurpose::Triage {
                targets: vec![1, 2, 3],
                status: Status::Ignored,
                reason: Some(IgnoreReason::NotAffected),
                justification: Some(Justification::RequiresConfiguration),
            }
        );
    }

    #[test]
    fn other_reasons_skip_justification() {
        let d = Dialog::Reason {
            targets: vec![9],
            cursor: 0,
        };
        let d = keep(d.handle(key(KeyCode::Char('j'))));
        let d = keep(d.handle(key(KeyCode::Enter)));
        let Dialog::Note { purpose, .. } = d else {
            panic!("expected note");
        };
        assert!(matches!(
            purpose,
            NotePurpose::Triage {
                reason: Some(IgnoreReason::RiskAccepted),
                justification: None,
                ..
            }
        ));
    }

    #[test]
    fn esc_cancels_note() {
        let d = type_text(
            note_dialog(NotePurpose::Pending {
                cve: "CVE-2099-1".into(),
                action: Action::Dismiss,
            }),
            "x",
        );
        assert!(matches!(d.handle(key(KeyCode::Esc)), Outcome::Close));
    }

    #[test]
    fn confirm_quit_returns_to_note_unless_yes() {
        let note = type_text(
            note_dialog(NotePurpose::Pending {
                cve: "CVE-2099-1".into(),
                action: Action::Dismiss,
            }),
            "abc",
        );
        let confirm = Dialog::ConfirmQuit {
            back: Box::new(note.clone()),
        };
        assert!(confirm.has_unsaved_note());
        let back = keep(confirm.clone().handle(key(KeyCode::Char('n'))));
        assert!(back.has_unsaved_note());
        assert!(matches!(
            confirm.handle(key(KeyCode::Char('y'))),
            Outcome::Submit(Command::Quit)
        ));
    }

    #[test]
    fn filter_panel_toggles_and_cycles() {
        let assets = vec!["PA-460".to_string(), "PVE-01".to_string()];
        let d = Dialog::Filter {
            draft: Filter::default(),
            assets,
            cursor: 1,
        };
        let d = keep(d.handle(key(KeyCode::Char(' ')))); // 已處理
        let asset_row = filter_rows()
            .iter()
            .position(|r| *r == FilterRow::Asset)
            .unwrap();
        let d = keep(d.handle(key(KeyCode::End)));
        let d = keep(d.handle(key(KeyCode::Up)));
        let Dialog::Filter { cursor, .. } = &d else {
            panic!()
        };
        assert_eq!(*cursor, asset_row);
        let d = keep(d.handle(key(KeyCode::Right)));
        let d = keep(d.handle(key(KeyCode::Right)));
        let Outcome::Submit(Command::ApplyFilter(f)) = d.handle(key(KeyCode::Enter)) else {
            panic!()
        };
        assert_eq!(f.statuses, [Status::Open, Status::Resolved]);
        assert_eq!(f.asset.as_deref(), Some("PVE-01"));
    }

    #[test]
    fn export_dialog_edits_path_and_submits_trimmed() {
        let d = Dialog::ExportVex {
            input: Input::default().with_value("C:/docs/a.json".into()),
            count: 3,
        };
        let d = keep(d.handle(key(KeyCode::Backspace)));
        let d = keep(d.handle(key(KeyCode::Backspace)));
        let d = keep(d.handle(key(KeyCode::Backspace)));
        let d = keep(d.handle(key(KeyCode::Backspace)));
        let d = type_text(d, "vex ");
        assert!(!d.has_unsaved_note(), "路徑不是備註，離開時不需確認");
        let Outcome::Submit(Command::ExportVex(path)) = d.handle(key(KeyCode::Enter)) else {
            panic!("expected submit");
        };
        assert_eq!(path, "C:/docs/a.vex");
        let d = Dialog::ExportVex {
            input: Input::default(),
            count: 1,
        };
        assert!(matches!(d.handle(key(KeyCode::Esc)), Outcome::Close));
    }

    #[test]
    fn cycle_wraps_through_none() {
        let items = ["a", "b"];
        assert_eq!(cycle(&None, &items, true), Some("a"));
        assert_eq!(cycle(&Some("b"), &items, true), None);
        assert_eq!(cycle(&None, &items, false), Some("b"));
        assert_eq!(cycle(&Some("a"), &items, false), None);
        assert_eq!(cycle::<&str>(&None, &[], true), None);
    }

    #[test]
    fn links_menu_opens_selected() {
        let d = Dialog::Links {
            links: crate::links::for_cve("CVE-2024-3400", None),
            cursor: 0,
        };
        let d = keep(d.handle(key(KeyCode::Down)));
        assert!(matches!(
            d.handle(key(KeyCode::Enter)),
            Outcome::Submit(Command::Open(url)) if url.contains("nvd.nist.gov")
        ));
    }
}
