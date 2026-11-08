//! 12 任務 4.1～4.2：編輯對話框的更名（提示、儲存後選取新編號、重複編號時保留對話框）。

mod support;

use cvewatch_gui::dialogs::asset_edit::rename_note;
use support::*;

type H = egui_kittest::Harness<'static, cvewatch_gui::GuiApp>;

fn open_edit(h: &mut H, tag: &str) {
    select_asset(h, tag);
    click(h, "編輯");
}

fn findings_of(h: &H, tag: &str) -> usize {
    h.state().ws.data.asset_findings(tag).len()
}

#[test]
fn asset_rename_note_follows_the_tag_field() {
    let (mut h, _f) = gui(seeded_world());
    open_edit(&mut h, "PA-460");
    let note = rename_note("PA-460");
    assert!(!shows(&h, &note), "未更名時不顯示提示");
    replace_text(&mut h, "資產編號 *", "PA-461");
    assert!(shows(&h, &note), "編號與原編號不同時顯示提示");
    assert!(shows(&h, "編輯 PA-460"), "標題維持原編號");
    replace_text(&mut h, "資產編號 *", "PA-460");
    assert!(!shows(&h, &note), "改回原編號後提示消失");
}

#[test]
fn asset_rename_in_the_edit_dialog() {
    let (mut h, f) = gui(seeded_world());
    let before = findings_of(&h, "PA-460");
    assert!(before > 0, "PA-460 有命中");
    open_edit(&mut h, "PA-460");
    replace_text(&mut h, "資產編號 *", "PA-461");
    replace_text(&mut h, "名稱 *", "pa461");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("PA-461"));
    let entry = h.state().ws.data.asset("PA-461").expect("以新編號列出");
    assert_eq!(entry.asset.record.name, "pa461");
    assert!(h.state().ws.data.asset("PA-460").is_none());
    assert_eq!(findings_of(&h, "PA-461"), before, "原有命中仍在新編號之下");
    assert!(listed(&h, "PA-461"));
    assert!(
        status(&h).contains("更名 PA-460 → PA-461"),
        "{}",
        status(&h)
    );
    assert_eq!(match_calls(&f), 1, "與其他欄位一起儲存時照常比對");
}

#[test]
fn asset_rename_only_skips_matching() {
    let (mut h, f) = gui(seeded_world());
    open_edit(&mut h, "PA-460");
    replace_text(&mut h, "資產編號 *", "PA-461");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("PA-461"));
    assert_eq!(match_calls(&f), 0, "只更名不需要比對");
}

#[test]
fn asset_rename_to_a_tag_in_use_keeps_the_dialog() {
    let (mut h, _f) = gui(seeded_world());
    open_edit(&mut h, "PA-460");
    replace_text(&mut h, "資產編號 *", "fw-core");
    replace_text(&mut h, "名稱 *", "pa461");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    assert!(shows(&h, "已被使用（FW-CORE）"), "錯誤顯示在資產編號欄位旁");
    assert!(h.state().status.is_none(), "本地驗證失敗不寫狀態列");
    let entry = h.state().ws.data.asset("PA-460").expect("編號不變");
    assert_eq!(entry.asset.record.name, "PA-460", "名稱不變");
}
