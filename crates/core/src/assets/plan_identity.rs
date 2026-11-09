//! 匯入差異中的識別欄位與 IP（20 design D3、D5、D6）：財產編號的訊息、IP 的變更與
//! 「清冊沒有 ips 工作表」的提醒。財產編號與清冊以外資產的衝突見 [`super::unique`]。

use super::ips::AssetIp;
use super::plan::CurrentAsset;
use super::validate::{Inventory, IpsInput};

/// 清冊沒有 `ips` 工作表（CSV 沒有 `--ips`）、且有既有資產原本就有 IP 時的提醒（整批一次）。
pub const IPS_KEPT_NOTICE: &str = "清冊沒有 ips 工作表，保留資料庫中既有資產的 IP";

/// 一個資產的 IP 變更：新舊清單（「位址（用途）」，依儲存順序）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IpChange {
    pub from: Vec<String>,
    pub to: Vec<String>,
}

impl IpChange {
    /// 「192.0.2.10 → 192.0.2.10、2001:db8::10」；沒有 IP 時為「-」。
    pub fn describe(&self) -> String {
        let list = |v: &[String]| {
            if v.is_empty() {
                super::plan_fields::BLANK_VALUE.to_string()
            } else {
                v.join("、")
            }
        };
        format!("{} → {}", list(&self.from), list(&self.to))
    }
}

fn ip_texts(ips: &[AssetIp]) -> Vec<String> {
    ips.iter().map(ToString::to_string).collect()
}

/// 這次要寫入的 IP：清冊有 `ips` 工作表時為該資產的清單（沒有列出為空清單），否則為 None。
pub(super) fn planned_ips(inventory: &Inventory, tag: &str) -> Option<Vec<AssetIp>> {
    match &inventory.ips {
        IpsInput::Sheet(by_tag) => Some(by_tag.get(tag).cloned().unwrap_or_default()),
        IpsInput::Missing | IpsInput::Untouched => None,
    }
}

/// IP 有變更時的新舊清單（順序也算在內；不寫入 IP 時為 None）。
pub(super) fn ip_change(existing: &CurrentAsset, planned: Option<&[AssetIp]>) -> Option<IpChange> {
    let planned = planned?;
    (existing.ips != planned).then(|| IpChange {
        from: ip_texts(&existing.ips),
        to: ip_texts(planned),
    })
}

/// 清冊沒有 `ips` 工作表，而既有資產原本就有 IP（需要 [`IPS_KEPT_NOTICE`]）。
pub(super) fn keeps_ips(inventory: &Inventory, existing: &CurrentAsset) -> bool {
    matches!(inventory.ips, IpsInput::Missing) && !existing.ips.is_empty()
}

/// 「財產編號 P-0002 已被 NAS-02 使用（該資產已退役）」（匯入與畫面編修共用）。
pub fn property_in_use_message(property_number: &str, owner: &str, retired: bool) -> String {
    let status = if retired {
        "（該資產已退役）"
    } else {
        ""
    };
    format!("財產編號 {property_number} 已被 {owner} 使用{status}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::model::{AssetRecord, AssetStatus};
    use std::str::FromStr;

    #[test]
    fn ip_change_reads_old_and_new_lists() {
        let change = IpChange {
            from: vec![],
            to: vec!["192.0.2.10（管理）".into(), "2001:db8::10".into()],
        };
        assert_eq!(change.describe(), "- → 192.0.2.10（管理）、2001:db8::10");
    }

    #[test]
    fn property_number_message_mentions_retired_owner() {
        assert_eq!(
            property_in_use_message("P-0002", "NAS-02", true),
            "財產編號 P-0002 已被 NAS-02 使用（該資產已退役）"
        );
        assert_eq!(
            property_in_use_message("P-0002", "NAS-01", false),
            "財產編號 P-0002 已被 NAS-01 使用"
        );
    }

    #[test]
    fn unchanged_addresses_are_not_a_change() {
        let ip = AssetIp {
            address: std::net::IpAddr::from_str("192.0.2.10").unwrap(),
            purpose: None,
        };
        let existing = CurrentAsset {
            id: 1,
            record: AssetRecord {
                asset_tag: "SRV-01".into(),
                name: "srv01".into(),
                category: crate::assets::ClassCode::parse("HW01").unwrap(),
                location: None,
                exposure: crate::assets::Exposure::Internal,
                cia: crate::assets::Cia::new(2, 3, 4),
                isms_id: None,
                system_code: None,
                responsible_unit: None,
                custodian: None,
                user_unit: None,
                compliance: None,
                personal_data: false,
                brand: None,
                model: None,
                serial_number: None,
                property_number: None,
                hostname: None,
                management_url: None,
                notes: None,
            },
            status: AssetStatus::Active,
            components: Default::default(),
            ips: vec![ip.clone()],
        };
        assert_eq!(ip_change(&existing, Some(std::slice::from_ref(&ip))), None);
        assert_eq!(ip_change(&existing, None), None);
        assert!(ip_change(&existing, Some(&[])).is_some());
    }
}
