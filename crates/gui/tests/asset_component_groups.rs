//! 21 任務 4.4：資產詳情與編輯對話框的元件依目錄角色分組（組名順序、元件位於組名之下、
//! 加入元件後排入其組並取得焦點、套件組唯讀）。

mod support;

use cvewatch_gui::dialogs::asset_components::PACKAGE_NOTE;
use cvewatch_gui::dialogs::AssetDialog;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use support::*;

const LIBC: &str = "pkg:deb/ubuntu/libc6";

/// 編輯對話框元件區塊的標題。
const DIALOG_COMPONENTS: &str = "元件 *（至少一個）";

/// 種子世界加上 `SRV-01`：Ubuntu、MySQL、iDRAC9 三個清冊元件與一個 dpkg 套件。
fn srv01_world() -> (Harness<'static, cvewatch_gui::GuiApp>, Fakes) {
    let w = seeded_world();
    w.asset(
        "SRV-01",
        "internal",
        &[
            ("ubuntu", "22.04"),
            ("mysql", "8.0.36"),
            ("dell-idrac9", "7.00.00.00"),
        ],
    );
    w.package("SRV-01", LIBC, "2.35-0ubuntu3", NOW);
    gui(w)
}

/// 標籤完全相符的唯一節點位置。
fn rect_of(h: &Harness<'_, cvewatch_gui::GuiApp>, label: &str) -> egui::Rect {
    let nodes: Vec<_> = h.query_all_by_label(label).collect();
    assert_eq!(nodes.len(), 1, "「{label}」應只有一個節點");
    nodes[0].rect()
}

/// 編輯對話框內的組名位置：詳情窗格也有相同組名，取與對話框元件區塊標題左緣對齊者。
fn dialog_header(h: &Harness<'_, cvewatch_gui::GuiApp>, label: &str) -> egui::Rect {
    let left = rect_of(h, DIALOG_COMPONENTS).left();
    h.query_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| (a.left() - left).abs().total_cmp(&(b.left() - left).abs()))
        .unwrap_or_else(|| panic!("找不到組名「{label}」"))
}

/// 標籤包含 `text` 的第一個節點位置。
fn rect_containing(h: &Harness<'_, cvewatch_gui::GuiApp>, text: &str) -> egui::Rect {
    h.query_all_by_label_contains(text)
        .next()
        .unwrap_or_else(|| panic!("找不到「{text}」"))
        .rect()
}

/// `rows` 由上到下排列。
fn assert_top_to_bottom(rows: &[(&str, egui::Rect)]) {
    for pair in rows.windows(2) {
        assert!(
            pair[0].1.center().y < pair[1].1.center().y,
            "「{}」應在「{}」之上",
            pair[0].0,
            pair[1].0
        );
    }
}

#[test]
fn grouped_components_in_the_detail_pane() {
    let (mut h, _f) = srv01_world();
    select_asset(&mut h, "SRV-01");
    let rows = [
        ("BMC", rect_of(&h, "BMC")),
        ("iDRAC9", rect_containing(&h, "iDRAC9 7.00.00.00")),
        ("作業系統", rect_of(&h, "作業系統")),
        ("Ubuntu", rect_containing(&h, "Ubuntu 22.04")),
        ("資料庫管理系統", rect_of(&h, "資料庫管理系統")),
        ("MySQL", rect_containing(&h, "MySQL 8.0.36")),
        ("套件", rect_of(&h, "套件")),
        ("libc6", rect_containing(&h, LIBC)),
    ];
    assert_top_to_bottom(&rows);
    // 元件縮排在組名之下
    assert!(rows[1].1.left() > rows[0].1.left(), "元件縮排");
    assert!(h.query_by_label("韌體").is_none(), "沒有元件的組不顯示");
}

fn open_edit(h: &mut Harness<'static, cvewatch_gui::GuiApp>) {
    select_asset(h, "SRV-01");
    click(h, "編輯");
    assert!(matches!(h.state().asset_dialog, Some(AssetDialog::Edit(_))));
}

#[test]
fn added_component_joins_its_group() {
    let (mut h, _f) = srv01_world();
    open_edit(&mut h);
    replace_text(&mut h, "MySQL 版本", "8.0.40");
    h.get_by_label("加入元件").click();
    h.run();
    h.get_by_label("搜尋產品").type_text("nginx");
    h.run();
    click_option(&mut h, "nginx（F5）");
    assert!(
        h.get_by_label("nginx 版本").is_focused(),
        "焦點移到 nginx 的版本欄"
    );
    assert_top_to_bottom(&[
        ("資料庫管理系統", dialog_header(&h, "資料庫管理系統")),
        ("MySQL 版本", rect_of(&h, "MySQL 版本")),
        ("網站服務系統", dialog_header(&h, "網站服務系統")),
        ("nginx 版本", rect_of(&h, "nginx 版本")),
        ("套件", dialog_header(&h, "套件")),
    ]);
    let Some(AssetDialog::Edit(edit)) = &h.state().asset_dialog else {
        panic!("編輯對話框未開啟");
    };
    let mysql = edit
        .form
        .components
        .iter()
        .find(|c| c.product_key == "mysql")
        .unwrap();
    assert_eq!(mysql.version, "8.0.40", "其他元件已輸入的版本不變");
    assert_eq!(edit.form.components.last().unwrap().product_key, "nginx");
}

#[test]
fn package_group_in_the_edit_dialog() {
    let (mut h, _f) = srv01_world();
    open_edit(&mut h);
    let header = dialog_header(&h, "套件");
    let package = h
        .query_all_by_label(LIBC)
        .map(|n| n.rect())
        .find(|r| (r.left() - header.left()).abs() < 1.0)
        .expect("對話框的套件列");
    assert!(
        package.center().y > header.center().y,
        "套件列在「套件」組名之下"
    );
    assert!(
        h.query_by_label(&format!("{LIBC} 版本")).is_none(),
        "沒有版本欄"
    );
    assert!(
        h.query_by_label(&format!("移除 {LIBC}")).is_none(),
        "沒有移除按鈕"
    );
    assert!(shows(&h, PACKAGE_NOTE));
}
