//! 17 任務 5.2：資產畫面的「由 ISMS 清冊產生草稿」（檔案對話框替身）。

mod support;

use std::path::PathBuf;

use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_frontend::Level;
use eframe::egui;
use support::*;

const BUTTON: &str = "由 ISMS 清冊產生草稿";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn asset_count(h: &egui_kittest::Harness<'static, cvewatch_gui::GuiApp>) -> i64 {
    h.state()
        .ws
        .conn
        .query_row("SELECT COUNT(*) FROM asset", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn assets_isms_generating_a_draft_from_the_window() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(fixture("isms_workbook.xlsx"));
    *f.dialogs.save.borrow_mut() = Some(None);
    let before = asset_count(&h);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, BUTTON);
    let name = "cvewatch-isms-draft-20261107.xlsx";
    let path = f.rec.documents_dir().join(name);
    assert_eq!(
        f.dialogs.save_asked.borrow().last().cloned(),
        Some((f.rec.documents_dir(), name.to_string()))
    );
    assert_eq!(
        f.dialogs.xlsx_titles.borrow().as_slice(),
        ["儲存 ISMS 草稿"]
    );
    let book = read_inventory(&path, None).expect("草稿可讀");
    assert_eq!(book.assets.rows.len(), 6);
    let s = status(&h);
    for part in [
        path.display().to_string().as_str(),
        "轉換 6 個資產（沿用既有 0）",
        "略過 5 列",
        "待補版本 7 個元件",
        "匯入清冊",
        "不要勾選",
    ] {
        assert!(s.contains(part), "{part}：{s}");
    }
    assert_eq!(asset_count(&h), before, "資料庫不變");
    assert!(f.companion.calls().is_empty(), "不需要 cvewatch.exe");
}

#[test]
fn assets_isms_not_an_isms_workbook_in_the_window() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(fixture("isms_not_workbook.xlsx"));
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, BUTTON);
    let s = status(&h);
    assert!(s.contains("找不到 ISMS 資訊資產清冊的表頭"), "{s}");
    assert_eq!(h.state().status.as_ref().unwrap().level, Level::Error);
    let files: Vec<_> = std::fs::read_dir(f.rec.documents_dir()).unwrap().collect();
    assert!(files.is_empty(), "不產生草稿：{files:?}");
}

#[test]
fn assets_isms_cancelling_the_save_dialog() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(fixture("isms_workbook.xlsx"));
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, BUTTON);
    assert!(status(&h).contains("已取消"), "{}", status(&h));
    assert_eq!(f.dialogs.save_asked.borrow().len(), 1, "已開啟另存對話框");
    // 開啟對話框就取消
    *f.dialogs.isms.borrow_mut() = None;
    click(&mut h, BUTTON);
    assert!(status(&h).contains("已取消"), "{}", status(&h));
    assert_eq!(f.dialogs.save_asked.borrow().len(), 1, "未再開啟另存對話框");
    let files: Vec<_> = std::fs::read_dir(f.rec.documents_dir()).unwrap().collect();
    assert!(files.is_empty(), "{files:?}");
}
