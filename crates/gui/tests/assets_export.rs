//! 10 任務 5.3：資產畫面的「下載範本」與「匯出清冊」（檔案對話框替身）。

mod support;

use cvewatch_core::assets::sheet::read_inventory;
use eframe::egui;
use support::*;

#[test]
fn assets_export_saving_the_template() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "下載範本");
    let path = f
        .rec
        .documents_dir()
        .join("cvewatch-inventory-template.xlsx");
    assert_eq!(
        f.dialogs.save_asked.borrow().last().cloned(),
        Some((
            f.rec.documents_dir(),
            "cvewatch-inventory-template.xlsx".into()
        ))
    );
    assert_eq!(f.dialogs.xlsx_titles.borrow().as_slice(), ["下載清冊範本"]);
    let book = read_inventory(&path, None).expect("範本可讀");
    assert!(!book.assets.rows.is_empty(), "含範例列");
    assert!(book.components.is_some(), "含 components 工作表");
    assert!(
        status(&h).contains(&path.display().to_string()),
        "{}",
        status(&h)
    );
}

#[test]
fn assets_export_template_target_already_exists() {
    let (mut h, f) = gui(seeded_world());
    let path = f.rec.documents_dir().join("mine.xlsx");
    std::fs::write(&path, b"original").unwrap();
    *f.dialogs.save.borrow_mut() = Some(Some(path.clone()));
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "下載範本");
    assert!(status(&h).contains("目的檔已存在"), "{}", status(&h));
    assert_eq!(
        h.state().status.as_ref().unwrap().level,
        cvewatch_frontend::Level::Error
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
}

#[test]
fn assets_export_exporting_the_inventory() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "匯出清冊");
    let name = "cvewatch-inventory-20261107.xlsx";
    let path = f.rec.documents_dir().join(name);
    assert_eq!(
        f.dialogs.save_asked.borrow().last().cloned(),
        Some((f.rec.documents_dir(), name.to_string()))
    );
    let book = read_inventory(&path, None).expect("清冊可讀");
    assert_eq!(book.assets.rows.len(), 3);
    let s = status(&h);
    assert!(
        s.contains("已匯出 3 個資產") && s.contains("此檔未加密"),
        "{s}"
    );
    assert!(s.contains(&path.display().to_string()), "{s}");
}

#[test]
fn assets_export_cancelled_dialog_writes_nothing() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "匯出清冊");
    assert!(status(&h).contains("已取消"), "{}", status(&h));
    click(&mut h, "下載範本");
    assert!(status(&h).contains("已取消"), "{}", status(&h));
    let files: Vec<_> = std::fs::read_dir(f.rec.documents_dir()).unwrap().collect();
    assert!(files.is_empty(), "{files:?}");
}
