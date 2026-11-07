//! 儲存設定：只改寫有變更的鍵（`toml_edit` 保留註解、排列與行尾說明），寫入前先以 core 的
//! 解析器驗證結果，再以「暫存檔 + rename」取代原檔；秘密只寫入憑證庫。

use std::path::Path;

use cvewatch_core::secrets::{SecretKey, SecretStore};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use super::form::{email_config, redmine_config, Form, Secret};

/// 改寫 `src` 中與 `original` 不同的欄位。
pub fn render(src: &str, original: &Form, form: &Form) -> Result<String, String> {
    let mut doc: DocumentMut = src
        .parse()
        .map_err(|e| format!("設定檔格式錯誤，未儲存：{e}"))?;
    email(&mut doc, original, form)?;
    redmine(&mut doc, original, form)?;
    schedule(&mut doc, original, form)?;
    Ok(doc.to_string())
}

/// 寫回設定檔（不存在時建立）。結果無法通過設定驗證時不寫入。
pub fn write_config(path: &Path, original: &Form, form: &Form) -> Result<(), String> {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("無法讀取設定檔 {}：{e}", path.display())),
    };
    let text = render(&src, original, form)?;
    cvewatch_core::config::parse(&text, path).map_err(|e| format!("設定未儲存：{e}"))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("無法建立 {}：{e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| format!("無法寫入 {}：{e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("無法取代設定檔 {}：{e}", path.display())
    })
}

/// 套用秘密變更（新值寫入、清除刪除）；未變更者不動。
pub fn apply_secrets(store: &dyn SecretStore, form: &Form) -> Result<(), String> {
    for (secret, key) in [
        (&form.smtp_password, SecretKey::SmtpPassword),
        (&form.api_key, SecretKey::RedmineApiKey),
    ] {
        match secret {
            Secret::New(v) => store.set(key, v.as_str()).map_err(|e| e.to_string())?,
            Secret::Clear => store.delete(key).map_err(|e| e.to_string())?,
            Secret::Stored(_) => {}
        }
    }
    Ok(())
}

/// 取得（必要時建立）`a.b` 表格。
fn table<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> Result<&'a mut Table, String> {
    let mut t = doc.as_table_mut();
    for key in path {
        if !t.contains_key(key) {
            let mut new = Table::new();
            new.set_implicit(false);
            t.insert(key, Item::Table(new));
        }
        t = t[*key]
            .as_table_mut()
            .ok_or_else(|| format!("設定檔的 `{}` 不是表格，請手動修改", path.join(".")))?;
    }
    Ok(t)
}

/// 設定值並沿用原本的前後空白與行尾註解。
fn set(t: &mut Table, key: &str, value: impl Into<Value>) {
    let mut new: Value = value.into();
    if let Some(old) = t.get(key).and_then(Item::as_value) {
        *new.decor_mut() = old.decor().clone();
    }
    t.insert(key, Item::Value(new));
}

fn email(doc: &mut DocumentMut, original: &Form, form: &Form) -> Result<(), String> {
    let (o, n) = (email_config(original), email_config(form));
    if o == n {
        return Ok(());
    }
    let t = table(doc, &["notify", "email"])?;
    if o.enabled != n.enabled {
        set(t, "enabled", n.enabled);
    }
    if o.smtp_host != n.smtp_host {
        set(t, "smtp_host", n.smtp_host.as_str());
    }
    if o.smtp_port != n.smtp_port {
        set(t, "smtp_port", i64::from(n.smtp_port));
    }
    if o.security != n.security {
        set(t, "security", n.security.as_str());
    }
    if o.username != n.username {
        set(t, "username", n.username.as_str());
    }
    if o.from != n.from {
        set(t, "from", n.from.as_str());
    }
    if o.to != n.to {
        set(
            t,
            "to",
            n.to.iter()
                .map(String::as_str)
                .collect::<toml_edit::Array>(),
        );
    }
    if o.send_empty != n.send_empty {
        set(t, "send_empty", n.send_empty);
    }
    Ok(())
}

fn redmine(doc: &mut DocumentMut, original: &Form, form: &Form) -> Result<(), String> {
    let (o, n) = (redmine_config(original), redmine_config(form));
    if o == n {
        return Ok(());
    }
    let t = table(doc, &["notify", "redmine"])?;
    if o.enabled != n.enabled {
        set(t, "enabled", n.enabled);
    }
    if o.base_url != n.base_url {
        set(t, "base_url", n.base_url.as_str());
    }
    if o.project_id != n.project_id {
        match n.project_id.parse::<i64>() {
            Ok(id) => set(t, "project_id", id),
            Err(_) => set(t, "project_id", n.project_id.as_str()),
        }
    }
    if o.tracker_id != n.tracker_id {
        set(t, "tracker_id", i64::from(n.tracker_id));
    }
    if o.priority_map != n.priority_map {
        let mut map = InlineTable::new();
        for (k, v) in &n.priority_map {
            map.insert(k, Value::from(i64::from(*v)));
        }
        set(t, "priority_map", map);
    }
    if o.assigned_to_id != n.assigned_to_id {
        set(t, "assigned_to_id", i64::from(n.assigned_to_id));
    }
    if o.add_note_on_triage != n.add_note_on_triage {
        set(t, "add_note_on_triage", n.add_note_on_triage);
    }
    Ok(())
}

fn schedule(doc: &mut DocumentMut, original: &Form, form: &Form) -> Result<(), String> {
    if original.urgent_cvss.trim() != form.urgent_cvss.trim() {
        let v: f64 = form
            .urgent_cvss
            .trim()
            .parse()
            .map_err(|_| "緊急門檻不是數字".to_string())?;
        set(table(doc, &["notify"])?, "urgent_cvss", v);
    }
    if original.sync_hours.trim() != form.sync_hours.trim() {
        let v: i64 = form
            .sync_hours
            .trim()
            .parse()
            .map_err(|_| "同步間隔不是整數".to_string())?;
        set(table(doc, &["schedule"])?, "sync_every_hours", v);
    }
    if original.digest_at.trim() != form.digest_at.trim() {
        set(
            table(doc, &["schedule"])?,
            "digest_at",
            form.digest_at.trim(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::form::FieldId;
    use cvewatch_core::config::{example_config, parse, Config};
    use cvewatch_core::secrets::MemoryStore;

    fn load(src: &str) -> Form {
        let (config, _) = parse(src, Path::new("t.toml")).unwrap();
        Form::load(&config, &MemoryStore::new())
    }

    #[test]
    fn edits_keep_comments_of_example_config() {
        let src = example_config();
        let original = load(&src);
        let form = original
            .toggled(FieldId::EmailEnabled)
            .with_text(FieldId::SmtpHost, "mail.example.edu.tw")
            .with_text(FieldId::From, "cvewatch <rz@example.edu.tw>")
            .with_recipient(None, "rz@example.edu.tw")
            .with_text(FieldId::Project, "12")
            .with_text(FieldId::Tracker, "3")
            .with_text(FieldId::PriorityP1, "5")
            .with_text(FieldId::DigestAt, "07:45");
        let out = render(&src, &original, &form).unwrap();
        insta::assert_snapshot!(out);
        let (config, unknown) = parse(&out, Path::new("t.toml")).unwrap();
        assert!(unknown.is_empty());
        assert_eq!(config.notify.redmine.project_id, "12");
        assert_eq!(config.notify.redmine.priority_map.get("P1"), Some(&5));
    }

    #[test]
    fn priorities_not_shown_in_console_are_kept() {
        let src = "[notify.redmine]
priority_map = { P1 = 5, P2 = 4, P3 = 3 }
";
        let original = load(src);
        let form = original.with_text(FieldId::PriorityP1, "6");
        let out = render(src, &original, &form).unwrap();
        assert!(
            out.contains("priority_map = { P1 = 6, P2 = 4, P3 = 3 }"),
            "{out}"
        );
    }

    #[test]
    fn unchanged_form_rewrites_nothing() {
        let src = example_config();
        let f = load(&src);
        assert_eq!(render(&src, &f, &f).unwrap(), src);
    }

    #[test]
    fn missing_file_gets_new_sections() {
        let original = Form::load(&Config::default(), &MemoryStore::new());
        let form = original.with_text(FieldId::SmtpHost, "mail");
        let out = render("", &original, &form).unwrap();
        assert!(
            out.contains("[notify.email]") && out.contains("smtp_host = \"mail\""),
            "{out}"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("cvewatch.toml");
        write_config(&path, &original, &form).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), out);
        assert!(!dir.path().join("sub").join("cvewatch.toml.tmp").exists());
    }

    #[test]
    fn secrets_go_to_store_only() {
        let store = MemoryStore::new();
        let original = Form::load(&Config::default(), &store);
        let form = original.with_secret(
            FieldId::ApiKey,
            Secret::New(zeroize::Zeroizing::new("k-123".into())),
        );
        let out = render(&example_config(), &original, &form).unwrap();
        assert!(!out.contains("k-123"));
        apply_secrets(&store, &form).unwrap();
        assert!(store.is_set(SecretKey::RedmineApiKey).unwrap());
        apply_secrets(&store, &form.with_secret(FieldId::ApiKey, Secret::Clear)).unwrap();
        assert!(!store.is_set(SecretKey::RedmineApiKey).unwrap());
    }
}
