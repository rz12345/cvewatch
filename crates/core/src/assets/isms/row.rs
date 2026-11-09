//! 單列轉換（17 design D3）：ISMS 編號組成、PE/IF/SE 過濾、組成 [`RecordInput`] 並以
//! [`validate_record`] 驗證；錯誤轉為 [`SkipReason`]，不中止整批。
//!
//! 例外：法規遵循性的 `-`、`－` 等破折號視為未評定；是否含個人資料只接受 Y／N（ISMS 清冊的寫法）。

use std::collections::HashMap;
use std::fmt;

use unicode_normalization::UnicodeNormalization;

use super::describe::display_name;
use super::workbook::IsmsRow;
use crate::assets::classes::{AssetClass, AssetClasses, MajorClass};
use crate::assets::model::AssetRecord;
use crate::assets::record::{validate_record, RecordContext, RecordInput};
use crate::assets::record_admin::isms_system_code;
use crate::assets::systems::SystemCode;

/// 草稿 notes 的前綴（其後為資產描述原文）。
pub const NOTES_PREFIX: &str = "ISMS 描述：";

/// 法規遵循性「未評定」的寫法（NFKC 之後；含 Excel 自動校正產生的各種破折號）。
const COMPLIANCE_DASHES: [char; 9] = ['-', '‐', '‑', '‒', '–', '—', '―', '−', 'ー'];

/// 略過的列與原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipReason {
    pub row: usize,
    pub message: String,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "第 {} 列：{}", self.row, self.message)
    }
}

/// 轉換成功的列（尚未參考既有資產）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedRow {
    pub number: usize,
    /// ISMS 編號（NFKC，保留大小寫寫法）
    pub isms_id: String,
    pub system: SystemCode,
    /// 「資產說明」欄原文
    pub system_name: String,
    /// 驗證後的欄位；`asset_tag` 為 ISMS 編號、曝露度為預設值（草稿寫出時留空）
    pub record: AssetRecord,
    pub description: String,
}

/// 一列的轉換結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowOutcome {
    Converted(Box<ConvertedRow>),
    /// HW、SW 以外的大類（PE、IF、SE…），只計數
    OtherMajor(String),
    Skipped(SkipReason),
}

fn nfkc(s: &str) -> String {
    s.trim().nfkc().collect::<String>().trim().to_string()
}

/// ISMS 編號：第一欄（去除結尾的 `-`）＋ `-` ＋ 第二欄（`NET-` ＋ `HW-001` → `NET-HW-001`）。
pub fn compose_isms_id(system_id: &str, serial: &str) -> String {
    let system = nfkc(system_id);
    let system = system.trim_end_matches('-').trim();
    let serial = nfkc(serial);
    if system.is_empty() {
        serial
    } else {
        format!("{system}-{serial}")
    }
}

/// 資產類別（代碼或名稱）對應的子類別。
fn row_class<'c>(row: &IsmsRow, classes: &'c AssetClasses) -> Option<&'c AssetClass> {
    classes
        .resolve(&nfkc(&row.category), false)
        .and_then(|r| classes.get(&r.code))
}

/// 兩個英文字母開頭、其後全是數字（`PE01`）時為大寫的兩個字母。
fn code_letters(s: &str) -> Option<String> {
    let v = nfkc(s);
    let (head, tail) = v.split_at_checked(2)?;
    let ok = head.bytes().all(|b| b.is_ascii_alphabetic())
        && !tail.is_empty()
        && tail.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| head.to_ascii_uppercase())
}

/// 流水號的大類（`PE-001` → `PE`）。
fn serial_major(serial: &str) -> Option<String> {
    let v: String = nfkc(serial).chars().take(2).collect();
    (v.len() == 2 && v.bytes().all(|b| b.is_ascii_alphabetic())).then(|| v.to_ascii_uppercase())
}

/// 列的大類：已定義子類別（代碼或名稱）的大類；`PE01` 這類代碼寫法取前兩碼；
/// 其他情形（空白、PE／IF／SE 的類別名稱）取流水號的前兩碼。都無法判斷時為 None。
fn row_major(row: &IsmsRow, classes: &AssetClasses) -> Option<String> {
    row_class(row, classes)
        .map(|c| c.code.major().as_str().to_string())
        .or_else(|| code_letters(&row.category))
        .or_else(|| serial_major(&row.serial))
}

fn is_hw_or_sw(major: &str) -> bool {
    MajorClass::parse(major).is_some()
}

/// 是否含個人資料：只接受 Y／N（不分大小寫、全形視同半形）。
fn personal_data(raw: &str) -> Result<String, String> {
    let v = nfkc(raw).to_uppercase();
    match v.as_str() {
        "" | "Y" | "N" => Ok(v),
        _ => Err(format!("是否含個人資料「{}」只能填 Y 或 N", raw.trim())),
    }
}

fn compliance(raw: &str) -> String {
    let v = nfkc(raw);
    let mut chars = v.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if COMPLIANCE_DASHES.contains(&c) => String::new(),
        _ => v,
    }
}

/// 名稱：描述第一行（去除數量與標籤）；為空時用子類別名稱，再沒有時用 ISMS 編號。
fn asset_name(row: &IsmsRow, isms_id: &str, ctx: &RecordContext<'_>) -> String {
    let name = display_name(&row.description);
    if !name.is_empty() {
        return name;
    }
    row_class(row, ctx.classes).map_or_else(|| isms_id.to_string(), |c| c.name.clone())
}

fn notes(description: &str) -> String {
    let d = description.trim();
    if d.is_empty() {
        String::new()
    } else {
        format!("{NOTES_PREFIX}{d}")
    }
}

/// ISMS 編號的大類是 HW、SW 以外的英文字母（例如 `WEB-SE-003`）而資產類別是 HW／SW 時，
/// 以與資產驗證相同的寫法回報大類不符（資產驗證只認得 HW、SW，會回報成格式錯誤）。
fn other_major_in_id(isms_id: &str, class: Option<&AssetClass>) -> Option<String> {
    let parts: Vec<&str> = isms_id.split('-').collect();
    let [_, major, _] = parts[..] else {
        return None;
    };
    let code = &class?.code;
    let letters = major.len() == 2 && major.bytes().all(|b| b.is_ascii_alphabetic());
    (letters && !is_hw_or_sw(major)).then(|| {
        format!(
            "ISMS 編號「{isms_id}」的大類 {} 與資產類別 {code} 不符",
            major.to_ascii_uppercase()
        )
    })
}

/// 驗證一個 HW／SW 列。
fn convert(row: &IsmsRow, ctx: &RecordContext<'_>) -> Result<ConvertedRow, String> {
    let isms_id = compose_isms_id(&row.system_id, &row.serial);
    if let Some(message) = other_major_in_id(&isms_id, row_class(row, ctx.classes)) {
        return Err(message);
    }
    let personal = personal_data(&row.personal_data)?;
    let name = asset_name(row, &isms_id, ctx);
    let notes = notes(&row.description);
    let compliance = compliance(&row.compliance);
    let fields = [
        nfkc(&row.category),
        nfkc(&row.confidentiality),
        nfkc(&row.integrity),
        nfkc(&row.availability),
    ];
    let input = RecordInput {
        asset_tag: &isms_id,
        name: &name,
        category: &fields[0],
        confidentiality: &fields[1],
        integrity: &fields[2],
        availability: &fields[3],
        isms_id: &isms_id,
        responsible_unit: &row.responsible_unit,
        custodian: &row.custodian,
        user_unit: &row.user_unit,
        compliance: &compliance,
        personal_data: &personal,
        notes: &notes,
        ..RecordInput::default()
    };
    let record = validate_record(&input, ctx).map_err(|errors| {
        // 沒有範本值的子類別：每個空白的 C/I/A 各一筆錯誤但訊息相同，只列一次
        let mut messages: Vec<String> = Vec::new();
        for e in errors {
            if !messages.contains(&e.message) {
                messages.push(e.message);
            }
        }
        messages.join("；")
    })?;
    let system = isms_system_code(&isms_id).expect("ISMS 編號已驗證");
    Ok(ConvertedRow {
        number: row.number,
        isms_id,
        system,
        system_name: row.system_name.trim().to_string(),
        record,
        description: row.description.trim().to_string(),
    })
}

/// HW／SW 列中出現的系統代碼（驗證時視為已定義：草稿的 `systems` 工作表會列出它們）。
pub fn system_codes(rows: &[IsmsRow], classes: &AssetClasses) -> Vec<SystemCode> {
    let mut codes: Vec<SystemCode> = rows
        .iter()
        .filter(|r| row_major(r, classes).is_some_and(|m| is_hw_or_sw(&m)))
        .filter_map(|r| isms_system_code(&compose_isms_id(&r.system_id, &r.serial)))
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

/// 逐列轉換；ISMS 編號與前面的 HW／SW 列重複（不分大小寫）時略過後者。
pub fn convert_rows(rows: &[IsmsRow], ctx: &RecordContext<'_>) -> Vec<RowOutcome> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    rows.iter()
        .map(|row| {
            let major = row_major(row, ctx.classes);
            if let Some(m) = major.as_ref().filter(|m| !is_hw_or_sw(m)) {
                return RowOutcome::OtherMajor(m.clone());
            }
            let skip = |message: String| {
                RowOutcome::Skipped(SkipReason {
                    row: row.number,
                    message,
                })
            };
            let id = compose_isms_id(&row.system_id, &row.serial);
            if let Some(first) = seen.get(&id.to_lowercase()) {
                return skip(format!("ISMS 編號「{id}」與第 {first} 列重複"));
            }
            seen.insert(id.to_lowercase(), row.number);
            match convert(row, ctx) {
                Ok(c) => RowOutcome::Converted(Box::new(c)),
                Err(message) => skip(message),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::classes::{AssetClasses, Cia};
    use crate::assets::systems::{AssetSystem, SystemDirectory};

    fn row(number: usize, id: (&str, &str), category: &str, cia: [&str; 3]) -> IsmsRow {
        IsmsRow {
            number,
            system_id: id.0.into(),
            serial: id.1.into(),
            category: category.into(),
            system_name: "網路服務".into(),
            description: "Synology XYZ-200 *1".into(),
            confidentiality: cia[0].into(),
            integrity: cia[1].into(),
            availability: cia[2].into(),
            ..IsmsRow::default()
        }
    }

    fn run(rows: &[IsmsRow]) -> Vec<RowOutcome> {
        let classes = AssetClasses::builtin();
        let systems = SystemDirectory::new(
            system_codes(rows, &classes)
                .into_iter()
                .map(|code| AssetSystem {
                    name: code.to_string(),
                    code,
                })
                .collect(),
        );
        convert_rows(
            rows,
            &RecordContext {
                classes: &classes,
                systems: &systems,
            },
        )
    }

    fn converted(o: &RowOutcome) -> &ConvertedRow {
        match o {
            RowOutcome::Converted(c) => c,
            other => panic!("{other:?}"),
        }
    }

    fn skipped(o: &RowOutcome) -> String {
        match o {
            RowOutcome::Skipped(s) => s.to_string(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_id_columns() {
        assert_eq!(compose_isms_id("NET-", "HW-001"), "NET-HW-001");
        assert_eq!(compose_isms_id("ＮＥＴ－", " HW-001 "), "NET-HW-001");
        assert_eq!(compose_isms_id("NET", "HW-001"), "NET-HW-001");
        let out = run(&[row(5, ("NET-", "HW-001"), "HW05", ["2", "3", "4"])]);
        let c = converted(&out[0]);
        assert_eq!(c.isms_id, "NET-HW-001");
        assert_eq!(c.system.as_str(), "NET");
        assert_eq!(c.record.asset_tag, "NET-HW-001");
        assert_eq!(c.record.system_code.as_deref(), Some("NET"));
        assert_eq!(c.record.name, "Synology XYZ-200");
        assert_eq!(c.record.cia, Cia::new(2, 3, 4));
        assert_eq!(
            c.record.notes.as_deref(),
            Some("ISMS 描述：Synology XYZ-200 *1")
        );
    }

    #[test]
    fn personnel_and_information_rows() {
        let mut rows = Vec::new();
        for (i, (serial, cat)) in [
            ("PE-001", "PE01"),
            ("IF-001", "IF01"),
            ("SE-001", "SE01"),
            ("HW-001", "HW05"),
            ("IF-002", ""),
        ]
        .iter()
        .enumerate()
        {
            rows.push(row(i + 5, ("NET-", serial), cat, ["", "", ""]));
        }
        let out = run(&rows);
        let majors: Vec<_> = out
            .iter()
            .filter_map(|o| match o {
                RowOutcome::OtherMajor(m) => Some(m.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(majors, ["PE", "IF", "SE", "IF"], "類別空白時依流水號");
        converted(&out[3]);
    }

    #[test]
    fn class_names_decide_the_major_class() {
        let out = run(&[
            row(5, ("NET-", "PE-001"), "人員", ["", "", ""]),
            row(6, ("NET-", "HW-001"), "通訊設備", ["", "", ""]),
        ]);
        assert_eq!(
            out[0],
            RowOutcome::OtherMajor("PE".into()),
            "PE 類別名稱依流水號"
        );
        let c = converted(&out[1]);
        assert_eq!(c.record.category.as_str(), "HW02", "子類別名稱");
        assert_eq!(c.record.system_code.as_deref(), Some("NET"));
    }

    #[test]
    fn major_class_mismatch() {
        let out = run(&[row(44, ("WEB-", "SE-003"), "SW01", ["3", "3", "3"])]);
        assert_eq!(
            skipped(&out[0]),
            "第 44 列：ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符"
        );
    }

    #[test]
    fn compliance_dash() {
        for dash in ["－", "-", "–", "−", "‐", ""] {
            let mut r = row(5, ("NET-", "HW-001"), "HW05", ["", "", ""]);
            r.compliance = dash.into();
            assert_eq!(converted(&run(&[r])[0]).record.compliance, None, "{dash}");
        }
        let mut r = row(5, ("NET-", "HW-001"), "HW05", ["", "", ""]);
        r.compliance = "普".into();
        assert_eq!(
            skipped(&run(&[r])[0]),
            "第 5 列：法規遵循性「普」必須是 1–4"
        );
    }

    #[test]
    fn personal_data_only_y_or_n() {
        let mut r = row(5, ("NET-", "HW-001"), "HW05", ["", "", ""]);
        r.personal_data = "y".into();
        assert!(
            converted(&run(std::slice::from_ref(&r))[0])
                .record
                .personal_data
        );
        r.personal_data = "是".into();
        assert_eq!(
            skipped(&run(&[r])[0]),
            "第 5 列：是否含個人資料「是」只能填 Y 或 N"
        );
    }

    #[test]
    fn duplicate_isms_id() {
        let out = run(&[
            row(5, ("NET-", "HW-001"), "HW05", ["", "", ""]),
            row(9, ("net-", "hw-001"), "HW05", ["", "", ""]),
        ]);
        converted(&out[0]);
        assert_eq!(
            skipped(&out[1]),
            "第 9 列：ISMS 編號「net-hw-001」與第 5 列重複"
        );
    }

    #[test]
    fn application_system_without_ratings() {
        let out = run(&[row(7, ("WEB-", "SW-001"), "SW01", ["", "", ""])]);
        assert_eq!(
            skipped(&out[0]),
            "第 7 列：SW01 應用系統必須填寫機密性、完整性與可用性"
        );
    }

    #[test]
    fn malformed_id_and_unknown_class() {
        let out = run(&[
            row(5, ("NE-", "HW-001"), "HW05", ["", "", ""]),
            row(6, ("NET-", "HW-002"), "HW99", ["", "", ""]),
        ]);
        assert!(skipped(&out[0]).contains("格式應為"), "{out:?}");
        assert!(skipped(&out[1]).contains("HW99"), "{out:?}");
    }

    #[test]
    fn name_falls_back_to_the_class_name() {
        let mut r = row(5, ("NET-", "HW-001"), "HW05", ["", "", ""]);
        r.description = String::new();
        let out = run(&[r]);
        let c = converted(&out[0]);
        assert_eq!(c.record.name, "儲存設備");
        assert_eq!(c.record.notes, None);
    }
}
