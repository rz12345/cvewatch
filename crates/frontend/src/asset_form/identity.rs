//! 資產表單的識別欄位與 IP（20 design D10）：IP 草稿列、IP 驗證（完全空白的列忽略）、
//! 財產編號的先行檢查與介面用語。欄位本身以 core 的 `validate_record` 驗證（見 [`super`]）。

use cvewatch_core::assets::ips::{validate_ips, AssetIp, IpError, IpErrorKind, IpInput};
use cvewatch_core::assets::record_identity::MAX_HOSTNAME_LEN;

use super::conflicts::used_by;
use super::AssetForm;
use crate::data::Snapshot;

/// 主機名稱的字元不符。
pub const HOSTNAME_FORMAT: &str = "只能包含英數字、-、. 與 _";
/// 管理介面位址不是 http／https 網址。
pub const MANAGEMENT_URL_FORMAT: &str = "需以 http:// 或 https:// 開頭";
/// 管理介面位址含帳號或密碼。
pub const URL_CREDENTIALS: &str = "帳號與密碼不得寫入管理介面位址";
/// IP 無法解析。
pub const IP_INVALID: &str = "不是有效的 IPv4 或 IPv6 位址";
/// IP 填了網段。
pub const IP_SINGLE_ADDRESS: &str = "只能填單一位址，不能填網段";
/// 有用途但沒有 IP。
pub const IP_REQUIRED: &str = "請填寫 IP";

/// 主機名稱太長。
pub fn hostname_too_long() -> String {
    format!("最長 {MAX_HOSTNAME_LEN} 個字元")
}

/// 一列 IP（輸入框的原始字串）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IpDraft {
    pub address: String,
    pub purpose: String,
}

impl IpDraft {
    pub fn from_ip(ip: &AssetIp) -> IpDraft {
        IpDraft {
            address: ip.address.to_string(),
            purpose: ip.purpose.clone().unwrap_or_default(),
        }
    }

    /// 位址與用途都空白（儲存時忽略）。
    pub fn is_blank(&self) -> bool {
        self.address.trim().is_empty() && self.purpose.trim().is_empty()
    }
}

/// IP 錯誤的介面用語；`rows` 為送去驗證的各筆在表單中的列索引。
fn ip_message(e: &IpError, rows: &[usize]) -> String {
    match e.kind {
        IpErrorKind::Missing => IP_REQUIRED.into(),
        IpErrorKind::Invalid if e.looks_like_network => {
            format!("{IP_INVALID}；{IP_SINGLE_ADDRESS}")
        }
        IpErrorKind::Invalid => IP_INVALID.into(),
        IpErrorKind::Duplicate => match e.first.and_then(|i| rows.get(i)) {
            Some(row) => format!("與第 {} 列重複", row + 1),
            None => "與其他列重複".into(),
        },
    }
}

impl AssetForm {
    /// 非空白的 IP 列：（表單列索引, 輸入）。
    fn ip_rows(&self) -> Vec<(usize, IpInput<'_>)> {
        self.ips
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.is_blank())
            .map(|(i, d)| {
                (
                    i,
                    IpInput {
                        address: &d.address,
                        purpose: &d.purpose,
                    },
                )
            })
            .collect()
    }

    /// 要送出的 IP（略過完全空白的列，依表單順序）。
    pub fn ip_inputs(&self) -> Vec<IpInput<'_>> {
        self.ip_rows().into_iter().map(|(_, input)| input).collect()
    }

    /// 加入一列空白 IP，回傳其索引（畫面把焦點移到該列）。
    pub fn add_ip(&mut self) -> usize {
        self.ips.push(IpDraft::default());
        self.ips.len() - 1
    }

    /// 移除一列 IP（索引超出範圍時不變）。
    pub fn remove_ip(&mut self, index: usize) {
        if index < self.ips.len() {
            self.ips.remove(index);
        }
    }

    /// IP 列的錯誤：（表單列索引, 介面用語）。
    pub(super) fn ip_errors(&self) -> Vec<(usize, String)> {
        let rows = self.ip_rows();
        let indexes: Vec<usize> = rows.iter().map(|(i, _)| *i).collect();
        let inputs: Vec<IpInput<'_>> = rows.into_iter().map(|(_, input)| input).collect();
        match validate_ips(&inputs) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .iter()
                .map(|e| (indexes[e.index], ip_message(e, &indexes)))
                .collect(),
        }
    }

    /// 表單的 IP 是否與 `current` 不同（略過空白列、以解析後的位址比較；無效時視為不同，
    /// 交給 core 回報錯誤）。
    pub fn ips_changed(&self, current: &[AssetIp]) -> bool {
        match validate_ips(&self.ip_inputs()) {
            Ok(ips) => ips != current,
            Err(_) => true,
        }
    }
}

/// 財產編號已被其他資產（含已退役、不分大小寫）使用時，「財產編號」欄位的錯誤
/// （「已被使用（已退役的 NAS-02）」）。core 仍在交易內再檢查一次（20 design D3）。
pub fn property_number_conflict(form: &AssetForm, data: &Snapshot) -> Option<String> {
    let n = form.property_number.trim();
    if n.is_empty() {
        return None;
    }
    used_by(data, form.original_tag(), |r| {
        r.property_number
            .as_deref()
            .is_some_and(|x| x.eq_ignore_ascii_case(n))
    })
}
