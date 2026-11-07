//! 05 任務 1.4：TestBackend 快照（空資料庫）— 各主畫面、說明畫面、ASCII 框線；全域鍵位。

mod support;

use cvewatch_tui::{glyphs, Screen};
use ratatui::crossterm::event::KeyCode;
use support::{app, app_with_glyphs, empty_world, keys, press, render};

#[test]
fn empty_database_opens_on_dashboard() {
    let (app, _) = app(empty_world());
    assert_eq!(app.screen, Screen::Dashboard);
    insta::assert_snapshot!(render(&app));
}

#[test]
fn empty_database_every_screen() {
    let (mut app, _) = app(empty_world());
    for (key, screen) in ['2', '3', '4', '5'].into_iter().zip(&Screen::ALL[1..]) {
        keys(&mut app, &key.to_string());
        assert_eq!(app.screen, *screen);
        insta::assert_snapshot!(screen.tab(), render(&app));
    }
}

#[test]
fn help_overlay_closes_on_any_key() {
    let (mut app, _) = app(empty_world());
    keys(&mut app, "?");
    assert!(app.help);
    insta::assert_snapshot!(render(&app));
    keys(&mut app, "x");
    assert!(!app.help);
    assert_eq!(app.screen, Screen::Dashboard, "關閉說明的按鍵不作其他用途");
}

#[test]
fn conhost_uses_ascii_frame() {
    let (app, _) = app_with_glyphs(empty_world(), glyphs::ASCII);
    let out = render(&app);
    assert!(!out.contains('┌') && !out.contains('│'));
    insta::assert_snapshot!(out);
}

#[test]
fn q_quits_and_vim_keys_are_safe_on_empty_lists() {
    let (mut app, _) = app(empty_world());
    for screen in ['2', '3', '4', '5'] {
        keys(&mut app, &screen.to_string());
        keys(&mut app, "jkgG ");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Esc);
    }
    assert!(app.stack.is_empty() && app.dialog.is_none());
    keys(&mut app, "q");
    assert!(app.quit);
}
