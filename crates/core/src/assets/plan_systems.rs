//! 匯入差異中的資產說明與 ISMS 管理欄位（16 design D4、D5）：`systems` 工作表的新增與改名、
//! 清冊缺欄時保留既有值、資產說明歸屬的解析（需要資料庫中的資產說明，因此在規劃階段進行）。

use super::columns::StandardField;
use super::model::AssetRecord;
use super::plan::CurrentAsset;
use super::record_admin::resolve_system;
use super::systems::{AssetSystem, SystemCode, SystemDirectory};
use super::validate::{ImportNotice, IncomingAsset, IncomingSystem, Inventory, RowRef};

/// 一筆資產說明的變更：`from` 為 None 時是新增。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemChange {
    pub row: RowRef,
    pub code: SystemCode,
    pub from: Option<String>,
    pub to: String,
}

impl SystemChange {
    /// 「新增資產說明 NET 網路服務」「資產說明 NET 名稱：網路服務 → 校園網路服務」。
    pub fn describe(&self) -> String {
        match &self.from {
            None => format!("新增資產說明 {} {}", self.code, self.to),
            Some(from) => format!("資產說明 {} 名稱：{from} → {}", self.code, self.to),
        }
    }

    pub fn system(&self) -> AssetSystem {
        AssetSystem {
            code: self.code.clone(),
            name: self.to.clone(),
        }
    }
}

/// `systems` 工作表相對資料庫的變更與合併後的目錄；名稱與其他代碼衝突時記錄錯誤。
pub(super) fn plan_systems(
    incoming: &[IncomingSystem],
    current: &SystemDirectory,
    errors: &mut Vec<String>,
) -> (Vec<SystemChange>, SystemDirectory) {
    let changes: Vec<SystemChange> = incoming
        .iter()
        .filter_map(|s| {
            let from = current.get(&s.system.code).map(|c| c.name.clone());
            (from.as_deref() != Some(s.system.name.as_str())).then(|| SystemChange {
                row: s.row,
                code: s.system.code.clone(),
                from,
                to: s.system.name.clone(),
            })
        })
        .collect();
    let systems: Vec<AssetSystem> = incoming.iter().map(|s| s.system.clone()).collect();
    let merged = current.with(&systems);
    for s in incoming {
        if let Some(e) = merged.conflict(&s.system.code, &s.system.name) {
            errors.push(format!("{}：{}", s.row, e.message));
        }
    }
    (changes, merged)
}

/// 清冊沒有的欄位改用資料庫中的值（回傳新紀錄，原紀錄不變）。
pub(super) fn keep_existing(
    record: &AssetRecord,
    existing: &AssetRecord,
    inventory: &Inventory,
) -> AssetRecord {
    let keep = |f: StandardField| inventory.keeps(f);
    let text = |f: StandardField, new: &Option<String>, old: &Option<String>| {
        if keep(f) {
            old.clone()
        } else {
            new.clone()
        }
    };
    let date =
        |f: StandardField, new: Option<chrono::NaiveDate>, old: Option<chrono::NaiveDate>| {
            if keep(f) {
                old
            } else {
                new
            }
        };
    AssetRecord {
        warranty_end: date(
            StandardField::WarrantyEnd,
            record.warranty_end,
            existing.warranty_end,
        ),
        support_end: date(
            StandardField::SupportEnd,
            record.support_end,
            existing.support_end,
        ),
        license_end: date(
            StandardField::LicenseEnd,
            record.license_end,
            existing.license_end,
        ),
        brand: text(StandardField::Brand, &record.brand, &existing.brand),
        model: text(StandardField::Model, &record.model, &existing.model),
        serial_number: text(
            StandardField::SerialNumber,
            &record.serial_number,
            &existing.serial_number,
        ),
        property_number: text(
            StandardField::PropertyNumber,
            &record.property_number,
            &existing.property_number,
        ),
        hostname: text(
            StandardField::Hostname,
            &record.hostname,
            &existing.hostname,
        ),
        management_url: text(
            StandardField::ManagementUrl,
            &record.management_url,
            &existing.management_url,
        ),
        isms_id: if keep(StandardField::IsmsId) {
            existing.isms_id.clone()
        } else {
            record.isms_id.clone()
        },
        responsible_unit: if keep(StandardField::ResponsibleUnit) {
            existing.responsible_unit.clone()
        } else {
            record.responsible_unit.clone()
        },
        custodian: if keep(StandardField::Custodian) {
            existing.custodian.clone()
        } else {
            record.custodian.clone()
        },
        user_unit: if keep(StandardField::UserUnit) {
            existing.user_unit.clone()
        } else {
            record.user_unit.clone()
        },
        compliance: if keep(StandardField::Compliance) {
            existing.compliance
        } else {
            record.compliance
        },
        personal_data: if keep(StandardField::PersonalData) {
            existing.personal_data
        } else {
            record.personal_data
        },
        ..record.clone()
    }
}

/// 解析資產說明歸屬（spec Asset system membership）。清冊沒有 `system` 欄時以資料庫中的
/// 資產說明代入，與最終的 ISMS 編號一併檢查。錯誤訊息含列位置。
pub(super) fn assign_system(
    incoming: &IncomingAsset,
    record: AssetRecord,
    existing: Option<&CurrentAsset>,
    inventory: &Inventory,
    systems: &SystemDirectory,
) -> Result<AssetRecord, String> {
    // 清冊沒有 system 欄：有新的 ISMS 編號時由它重新帶出（資料庫中的資產說明可能只是由舊的
    // ISMS 編號帶出，例如 migration 0008）；否則沿用資料庫中的資產說明（16 code review）
    let rederive = !inventory.keeps(StandardField::IsmsId) && record.isms_id.is_some();
    let kept = existing
        .filter(|_| !rederive)
        .and_then(|e| e.record.system_code.clone())
        .unwrap_or_default();
    let raw = if inventory.keeps(StandardField::System) {
        kept.as_str()
    } else {
        incoming.system_raw.as_str()
    };
    match resolve_system(raw, record.isms_id.as_deref(), systems) {
        Ok(code) => Ok(AssetRecord {
            system_code: code.map(|c| c.as_str().to_string()),
            ..record
        }),
        Err(e) => Err(format!("{}：{}", incoming.row, e.message)),
    }
}

/// 清冊缺少 16、20 新增的欄位、且有既有資產時的提醒（isms_id 另有原本的提醒）。
fn is_lifecycle_date(f: StandardField) -> bool {
    matches!(
        f,
        StandardField::WarrantyEnd | StandardField::SupportEnd | StandardField::LicenseEnd
    )
}

pub(super) fn kept_columns_notice(
    inventory: &Inventory,
    any_existing: bool,
) -> Option<ImportNotice> {
    let missing: Vec<&str> = inventory
        .kept_fields
        .iter()
        .filter(|f| **f != StandardField::IsmsId)
        // ISMS 匯入不處理生命週期日期（保留既有值、不提醒；22 design D7）
        .filter(|f| !(inventory.lifecycle_untouched() && is_lifecycle_date(**f)))
        .map(|f| f.as_str())
        .collect();
    (any_existing && !missing.is_empty()).then(|| ImportNotice {
        row: None,
        asset_tag: None,
        message: format!(
            "清冊沒有 {} 欄，保留資料庫中既有資產的值",
            missing.join("、")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::columns::SheetKind;

    fn row(n: usize) -> RowRef {
        RowRef {
            sheet: SheetKind::Systems,
            number: n,
        }
    }

    fn sys(code: &str, name: &str) -> AssetSystem {
        AssetSystem {
            code: SystemCode::parse(code).unwrap(),
            name: name.into(),
        }
    }

    fn incoming(n: usize, code: &str, name: &str) -> IncomingSystem {
        IncomingSystem {
            row: row(n),
            system: sys(code, name),
        }
    }

    #[test]
    fn added_and_renamed_systems() {
        let current = SystemDirectory::new(vec![sys("NET", "網路服務"), sys("SYS", "核心系統")]);
        let mut errors = Vec::new();
        let (changes, merged) = plan_systems(
            &[
                incoming(2, "NET", "校園網路服務"),
                incoming(3, "SYS", "核心系統"),
                incoming(4, "LAB", "實驗室"),
            ],
            &current,
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let text: Vec<String> = changes.iter().map(SystemChange::describe).collect();
        assert_eq!(
            text,
            vec![
                "資產說明 NET 名稱：網路服務 → 校園網路服務".to_string(),
                "新增資產說明 LAB 實驗室".to_string(),
            ]
        );
        assert_eq!(merged.all().len(), 3);
    }

    #[test]
    fn name_taken_by_another_code_in_the_database() {
        let current = SystemDirectory::new(vec![sys("NET", "網路服務")]);
        let mut errors = Vec::new();
        plan_systems(&[incoming(2, "LAB", "網路服務")], &current, &mut errors);
        assert_eq!(
            errors,
            vec!["systems 工作表第 2 列：名稱「網路服務」已被資產說明 NET 使用".to_string()]
        );
    }

    #[test]
    fn swapping_names_through_the_sheet_is_allowed() {
        let current = SystemDirectory::new(vec![sys("NET", "甲"), sys("SYS", "乙")]);
        let mut errors = Vec::new();
        plan_systems(
            &[incoming(2, "NET", "乙"), incoming(3, "SYS", "甲")],
            &current,
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
    }
}
