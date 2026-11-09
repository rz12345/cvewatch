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
    open_edit(&mut h, "FW-EDGE");
    let note = rename_note("FW-EDGE");
    assert!(!shows(&h, &note), "未更名時不顯示提示");
    replace_text(&mut h, "資產編號 *", "FW-BORDER");
    assert!(shows(&h, &note), "編號與原編號不同時顯示提示");
    assert!(shows(&h, "編輯 FW-EDGE"), "標題維持原編號");
    replace_text(&mut h, "資產編號 *", "FW-EDGE");
    assert!(!shows(&h, &note), "改回原編號後提示消失");
}

#[test]
fn asset_rename_in_the_edit_dialog() {
    let (mut h, f) = gui(seeded_world());
    let before = findings_of(&h, "FW-EDGE");
    assert!(before > 0, "FW-EDGE 有命中");
    open_edit(&mut h, "FW-EDGE");
    replace_text(&mut h, "資產編號 *", "FW-BORDER");
    replace_text(&mut h, "名稱 *", "fwborder");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("FW-BORDER"));
    let entry = h.state().ws.data.asset("FW-BORDER").expect("以新編號列出");
    assert_eq!(entry.asset.record.name, "fwborder");
    assert!(h.state().ws.data.asset("FW-EDGE").is_none());
    assert_eq!(
        findings_of(&h, "FW-BORDER"),
        before,
        "原有命中仍在新編號之下"
    );
    assert!(listed(&h, "FW-BORDER"));
    assert!(
        status(&h).contains("更名 FW-EDGE → FW-BORDER"),
        "{}",
        status(&h)
    );
    assert_eq!(match_calls(&f), 1, "與其他欄位一起儲存時照常比對");
}

#[test]
fn asset_rename_only_skips_matching() {
    let (mut h, f) = gui(seeded_world());
    open_edit(&mut h, "FW-EDGE");
    replace_text(&mut h, "資產編號 *", "FW-BORDER");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("FW-BORDER"));
    assert_eq!(match_calls(&f), 0, "只更名不需要比對");
}

#[test]
fn asset_rename_to_a_tag_in_use_keeps_the_dialog() {
    let (mut h, _f) = gui(seeded_world());
    open_edit(&mut h, "FW-EDGE");
    replace_text(&mut h, "資產編號 *", "fw-core");
    replace_text(&mut h, "名稱 *", "fwborder");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    assert!(shows(&h, "已被使用（FW-CORE）"), "錯誤顯示在資產編號欄位旁");
    assert!(h.state().status.is_none(), "本地驗證失敗不寫狀態列");
    let entry = h.state().ws.data.asset("FW-EDGE").expect("編號不變");
    assert_eq!(entry.asset.record.name, "FW-EDGE", "名稱不變");
}
