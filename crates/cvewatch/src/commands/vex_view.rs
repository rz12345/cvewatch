//! `cvewatch vex import` 的報告輸出。

use cvewatch_core::vex::import::{ImportReport, Item};

fn line(it: &Item) -> String {
    let method = it
        .method
        .map(|m| format!("  [{}]", m.label()))
        .unwrap_or_default();
    let target = if it.target.is_empty() {
        String::new()
    } else {
        format!("  {}", it.target)
    };
    format!("  {}{target}{method}  {}", it.vuln, it.note)
}

fn section(title: &str, items: &[Item]) {
    if items.is_empty() {
        return;
    }
    println!();
    println!("{title}（{}）", items.len());
    for it in items {
        println!("{}", line(it));
    }
}

pub fn print_report(r: &ImportReport) {
    let p = &r.plan;
    let mode = if r.applied {
        "已套用"
    } else {
        "dry-run，資料庫未變更"
    };
    println!("VEX 匯入：{}（{mode}）", r.file_name);
    let apply_title = if r.applied { "已套用" } else { "將套用" };
    println!(
        "  {apply_title} {}、未對應 {}、本機較新 {}、格式不完整 {}（另有 {} 筆與本機相同，略過）",
        p.apply.len(),
        p.unmatched.len(),
        p.local_newer.len(),
        p.incomplete.len(),
        p.unchanged.len()
    );
    let applied: Vec<Item> = p.apply.iter().map(|a| a.item.clone()).collect();
    section(apply_title, &applied);
    section("未對應（本機沒有對應的命中，不新建）", &p.unmatched);
    section("本機較新（未套用）", &p.local_newer);
    section("格式不完整（未套用）", &p.incomplete);
    section("與本機相同（略過）", &p.unchanged);
    if !p.local_newer.is_empty() {
        println!();
        println!("本機較新的項目可加 --force 以檔案內容覆蓋。");
    }
}
