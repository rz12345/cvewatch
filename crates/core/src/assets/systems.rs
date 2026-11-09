//! 資產說明（ISMS 的系統／服務，16 design D1、D5）：代碼與名稱、不可變的查詢目錄、
//! 資料庫存取與新增、改名、刪除。
//!
//! 代碼為三碼英數字（全形轉半形、以大寫儲存），建立後不可修改；代碼與名稱都不分大小寫
//! 唯一。驗證函式為純函式，畫面可在寫入前用同一套規則把錯誤標在欄位旁。

use std::fmt;

use rusqlite::{params, Connection, OptionalExtension};
use unicode_normalization::UnicodeNormalization;

use crate::error::CoreError;

/// 資產說明代碼的長度（與 ISMS 編號的系統代碼相同）。
pub const SYSTEM_CODE_LEN: usize = 3;

fn nfkc(s: &str) -> String {
    s.trim().nfkc().collect()
}

/// 名稱比對用的正規化（NFKC、小寫）；所有名稱比對都經由此函式。
pub(crate) fn name_key(s: &str) -> String {
    nfkc(s).to_lowercase()
}

/// 資產說明代碼（例如 `SYS`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SystemCode(String);

impl SystemCode {
    /// 三碼英數字（不分大小寫、全形視同半形）；其他寫法為 None。
    pub fn parse(raw: &str) -> Option<SystemCode> {
        let v = nfkc(raw).to_ascii_uppercase();
        let ok = v.len() == SYSTEM_CODE_LEN && v.bytes().all(|b| b.is_ascii_alphanumeric());
        ok.then_some(SystemCode(v))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SystemCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 一個資產說明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSystem {
    pub code: SystemCode,
    pub name: String,
}

impl AssetSystem {
    /// 「NET 網路服務」。
    pub fn label(&self) -> String {
        format!("{} {}", self.code, self.name)
    }
}

/// 資產說明表單的欄位（畫面把錯誤顯示在對應欄位旁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemField {
    Code,
    Name,
}

/// 資產說明的驗證錯誤。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemError {
    pub field: SystemField,
    pub message: String,
}

impl SystemError {
    fn new(field: SystemField, message: String) -> SystemError {
        SystemError { field, message }
    }
}

fn join(errors: Vec<SystemError>) -> CoreError {
    let messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
    CoreError::Invalid(messages.join("；"))
}

/// 資產說明的不可變快照（依代碼排序），可依代碼或名稱查詢。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemDirectory {
    systems: Vec<AssetSystem>,
}

impl SystemDirectory {
    pub fn new(mut systems: Vec<AssetSystem>) -> SystemDirectory {
        systems.sort_by(|a, b| a.code.cmp(&b.code));
        SystemDirectory { systems }
    }

    /// 從資料庫載入。
    pub fn load(conn: &Connection) -> Result<SystemDirectory, CoreError> {
        SystemRepo::new(conn).list().map(SystemDirectory::new)
    }

    pub fn all(&self) -> &[AssetSystem] {
        &self.systems
    }

    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    pub fn get(&self, code: &SystemCode) -> Option<&AssetSystem> {
        self.systems.iter().find(|s| &s.code == code)
    }

    /// 依代碼（不分大小寫）或名稱（NFKC、不分大小寫）查詢；空白為 None。
    pub fn find(&self, raw: &str) -> Option<&AssetSystem> {
        let v = raw.trim();
        if v.is_empty() {
            return None;
        }
        SystemCode::parse(v)
            .and_then(|code| self.get(&code))
            .or_else(|| {
                let key = name_key(v);
                self.systems.iter().find(|s| name_key(&s.name) == key)
            })
    }

    /// 以名稱查詢（不分大小寫）。
    pub fn by_name(&self, name: &str) -> Option<&AssetSystem> {
        let key = name_key(name);
        self.systems.iter().find(|s| name_key(&s.name) == key)
    }

    /// `code` 這個資產說明使用 `name` 時與其他資產說明的衝突（16 code review）：名稱重複
    /// （NFKC、不分大小寫）、名稱等於其他資產說明的代碼、代碼等於其他資產說明的名稱。
    /// 依代碼或名稱查詢（[`find`](Self::find)）時，這些情況會對應到錯的資產說明。
    pub fn conflict(&self, code: &SystemCode, name: &str) -> Option<SystemError> {
        let key = name_key(name);
        let others = || self.systems.iter().filter(move |s| &s.code != code);
        if let Some(other) = others().find(|s| name_key(&s.name) == key) {
            return Some(SystemError::new(
                SystemField::Name,
                format!("名稱「{}」已被資產說明 {} 使用", name.trim(), other.code),
            ));
        }
        if let Some(other) = SystemCode::parse(name).and_then(|c| others().find(|s| s.code == c)) {
            return Some(SystemError::new(
                SystemField::Name,
                format!(
                    "名稱「{}」與資產說明 {} 的代碼相同",
                    name.trim(),
                    other.code
                ),
            ));
        }
        let code_key = name_key(code.as_str());
        others()
            .find(|s| name_key(&s.name) == code_key)
            .map(|other| {
                SystemError::new(
                    SystemField::Code,
                    format!("代碼 {code} 與資產說明 {} 的名稱相同", other.code),
                )
            })
    }

    /// 「NET 網路服務」；目錄中沒有的代碼為「NET（未定義）」。
    pub fn label(&self, code: &str) -> String {
        SystemCode::parse(code)
            .and_then(|c| self.get(&c))
            .map_or_else(|| format!("{code}（未定義）"), AssetSystem::label)
    }

    /// 套用新增或改名後的新目錄（清冊匯入時合併 `systems` 工作表；原目錄不變）。
    pub fn with(&self, changes: &[AssetSystem]) -> SystemDirectory {
        let mut systems: Vec<AssetSystem> = self
            .systems
            .iter()
            .filter(|s| changes.iter().all(|c| c.code != s.code))
            .cloned()
            .collect();
        systems.extend(changes.iter().cloned());
        SystemDirectory::new(systems)
    }
}

fn check_name(
    name: &str,
    own: Option<&SystemCode>,
    dir: &SystemDirectory,
    errors: &mut Vec<SystemError>,
) -> String {
    let name = name.trim().to_string();
    if name.is_empty() {
        errors.push(SystemError::new(
            SystemField::Name,
            "資產說明名稱不可空白".to_string(),
        ));
    } else {
        // 代碼不合法時只檢查名稱是否重複
        let conflict = match own {
            Some(code) => dir.conflict(code, &name),
            None => dir.by_name(&name).map(|other| {
                SystemError::new(
                    SystemField::Name,
                    format!("名稱「{name}」已被資產說明 {} 使用", other.code),
                )
            }),
        };
        errors.extend(conflict);
    }
    name
}

/// 驗證新增的資產說明（代碼格式、代碼與名稱不重複）。
pub fn validate_new(
    code: &str,
    name: &str,
    dir: &SystemDirectory,
) -> Result<AssetSystem, Vec<SystemError>> {
    let mut errors = Vec::new();
    let parsed = SystemCode::parse(code);
    match &parsed {
        None => errors.push(SystemError::new(
            SystemField::Code,
            format!("資產說明代碼「{}」必須是三碼英數字，例如 SYS", code.trim()),
        )),
        Some(c) if dir.get(c).is_some() => errors.push(SystemError::new(
            SystemField::Code,
            format!("資產說明代碼 {c} 已被使用"),
        )),
        Some(_) => {}
    }
    let name = check_name(name, parsed.as_ref(), dir, &mut errors);
    match parsed {
        Some(code) if errors.is_empty() => Ok(AssetSystem { code, name }),
        _ => Err(errors),
    }
}

/// 驗證改名（代碼須存在、名稱不與其他資產說明重複）。
pub fn validate_rename(
    code: &SystemCode,
    name: &str,
    dir: &SystemDirectory,
) -> Result<AssetSystem, Vec<SystemError>> {
    let mut errors = Vec::new();
    if dir.get(code).is_none() {
        errors.push(SystemError::new(
            SystemField::Code,
            format!("資產說明 {code} 不存在"),
        ));
    }
    let name = check_name(name, Some(code), dir, &mut errors);
    if errors.is_empty() {
        Ok(AssetSystem {
            code: code.clone(),
            name,
        })
    } else {
        Err(errors)
    }
}

/// `asset_system` 表的存取。
pub struct SystemRepo<'c> {
    conn: &'c Connection,
}

impl<'c> SystemRepo<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        SystemRepo { conn }
    }

    pub fn list(&self) -> Result<Vec<AssetSystem>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT code, name FROM asset_system ORDER BY code")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (code, name) = row?;
            // 代碼以大寫寫入；格式不符的資料（理論上不會發生）原樣保留以免遺失
            let code = SystemCode::parse(&code).unwrap_or(SystemCode(code));
            out.push(AssetSystem { code, name });
        }
        Ok(out)
    }

    /// 新增或改名（代碼相同時更新名稱）。
    pub fn upsert(&self, system: &AssetSystem) -> Result<(), CoreError> {
        self.conn.execute(
            "INSERT INTO asset_system (code, name) VALUES (?1, ?2)
             ON CONFLICT (code) DO UPDATE SET name = excluded.name",
            params![system.code.as_str(), system.name],
        )?;
        Ok(())
    }

    /// 歸屬此資產說明的資產數（含 retired）。
    pub fn usage(&self, code: &SystemCode) -> Result<u32, CoreError> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM asset WHERE system_code = ?1",
            params![code.as_str()],
            |r| r.get(0),
        )?)
    }

    /// 每個資產說明的歸屬資產數（含 retired）；沒有資產的資產說明為 0。
    pub fn usage_counts(&self) -> Result<Vec<(SystemCode, u32)>, CoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.code, count(a.id) FROM asset_system s
             LEFT JOIN asset a ON a.system_code = s.code GROUP BY s.code ORDER BY s.code",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (code, n) = row?;
            out.push((SystemCode::parse(&code).unwrap_or(SystemCode(code)), n));
        }
        Ok(out)
    }

    fn exists(&self, code: &SystemCode) -> Result<bool, CoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM asset_system WHERE code = ?1",
                params![code.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    fn delete(&self, code: &SystemCode) -> Result<(), CoreError> {
        self.conn.execute(
            "DELETE FROM asset_system WHERE code = ?1",
            params![code.as_str()],
        )?;
        Ok(())
    }
}

/// 新增資產說明。
pub fn create_system(conn: &Connection, code: &str, name: &str) -> Result<AssetSystem, CoreError> {
    let dir = SystemDirectory::load(conn)?;
    let system = validate_new(code, name, &dir).map_err(join)?;
    SystemRepo::new(conn).upsert(&system)?;
    Ok(system)
}

/// 修改資產說明名稱（代碼不可修改）。
pub fn rename_system(
    conn: &Connection,
    code: &SystemCode,
    name: &str,
) -> Result<AssetSystem, CoreError> {
    let dir = SystemDirectory::load(conn)?;
    let system = validate_rename(code, name, &dir).map_err(join)?;
    SystemRepo::new(conn).upsert(&system)?;
    Ok(system)
}

/// 刪除資產說明；仍有資產（含 retired）歸屬時拒絕。
pub fn delete_system(conn: &Connection, code: &SystemCode) -> Result<(), CoreError> {
    let repo = SystemRepo::new(conn);
    if !repo.exists(code)? {
        return Err(CoreError::Invalid(format!("資產說明 {code} 不存在")));
    }
    let used = repo.usage(code)?;
    if used > 0 {
        return Err(CoreError::Invalid(format!(
            "資產說明 {code} 仍有 {used} 個資產歸屬，無法刪除"
        )));
    }
    repo.delete(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(code: &str, name: &str) -> AssetSystem {
        AssetSystem {
            code: SystemCode::parse(code).unwrap(),
            name: name.into(),
        }
    }

    #[test]
    fn code_parsing() {
        assert_eq!(SystemCode::parse(" net ").unwrap().as_str(), "NET");
        assert_eq!(SystemCode::parse("Ａ０１").unwrap().as_str(), "A01");
        for bad in ["NET1", "NE", "N-T", ""] {
            assert_eq!(SystemCode::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn directory_finds_by_code_or_name() {
        let dir = SystemDirectory::new(vec![sys("SYS", "核心系統"), sys("NET", "網路服務")]);
        assert_eq!(dir.all()[0].code.as_str(), "NET", "依代碼排序");
        assert_eq!(dir.find("net").unwrap().name, "網路服務");
        assert_eq!(dir.find(" 網路服務 ").unwrap().code.as_str(), "NET");
        assert_eq!(dir.find("LAB"), None);
        assert_eq!(dir.find(""), None);
        assert_eq!(dir.label("NET"), "NET 網路服務");
        assert_eq!(dir.label("LAB"), "LAB（未定義）");
        let changed = dir.with(&[sys("NET", "校園網路服務"), sys("LAB", "實驗室")]);
        assert_eq!(changed.find("NET").unwrap().name, "校園網路服務");
        assert_eq!(changed.all().len(), 3);
        assert_eq!(dir.find("NET").unwrap().name, "網路服務", "原目錄不變");
    }

    #[test]
    fn new_system_validation() {
        let dir = SystemDirectory::new(vec![sys("NET", "網路服務")]);
        let ok = validate_new("sys", " 核心系統 ", &dir).unwrap();
        assert_eq!(ok, sys("SYS", "核心系統"));
        let dup = validate_new("Net", "網路服務二", &dir).unwrap_err();
        assert_eq!(dup[0].field, SystemField::Code);
        assert!(dup[0].message.contains("NET 已被使用"), "{dup:?}");
        let bad = validate_new("NET1", "", &dir).unwrap_err();
        assert_eq!(bad.len(), 2);
        assert!(bad[0].message.contains("三碼英數字"));
        assert_eq!(bad[1].field, SystemField::Name);
        let name = validate_new("LAB", "網路服務", &dir).unwrap_err();
        assert!(name[0].message.contains("已被資產說明 NET 使用"));
        let full = validate_new(
            "LAB",
            "ＮＥＴ服務",
            &SystemDirectory::new(vec![sys("NET", "NET服務")]),
        )
        .unwrap_err();
        assert!(
            full[0].message.contains("已被資產說明 NET 使用"),
            "全形視同半形"
        );
    }

    #[test]
    fn names_and_codes_cannot_cross() {
        let dir = SystemDirectory::new(vec![sys("NET", "網路服務")]);
        let e = validate_new("LAB", "net", &dir).unwrap_err();
        assert_eq!(e[0].field, SystemField::Name);
        assert_eq!(e[0].message, "名稱「net」與資產說明 NET 的代碼相同");
        let dir = SystemDirectory::new(vec![sys("NET", "LAB")]);
        let e = validate_new("lab", "實驗室", &dir).unwrap_err();
        assert_eq!(e[0].field, SystemField::Code);
        assert_eq!(e[0].message, "代碼 LAB 與資產說明 NET 的名稱相同");
        // 名稱等於自己的代碼（migration 建立的資產說明）可以
        let own = SystemDirectory::new(vec![sys("SYS", "SYS")]);
        assert!(validate_rename(&SystemCode::parse("SYS").unwrap(), "SYS", &own).is_ok());
    }

    #[test]
    fn rename_allows_own_name() {
        let dir = SystemDirectory::new(vec![sys("NET", "網路服務"), sys("SYS", "核心系統")]);
        let net = SystemCode::parse("NET").unwrap();
        assert!(validate_rename(&net, "網路服務", &dir).is_ok());
        assert!(validate_rename(&net, "核心系統", &dir).is_err());
        let lab = SystemCode::parse("LAB").unwrap();
        assert!(validate_rename(&lab, "實驗室", &dir).is_err());
    }
}
