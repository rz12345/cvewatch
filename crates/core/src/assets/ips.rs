//! 資產的 IP 位址（20 design D4）：型別、驗證與 `asset_ip` 表的讀寫。
//!
//! 每筆是一個 IPv4 或 IPv6 位址（不接受網段、範圍、zone ID 與 IPv4 前導零），以
//! [`IpAddr`] 的標準寫法儲存與顯示。同一資產不可重複（以解析後的位址比較）；不同資產可相同。

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;

/// 一筆 IP。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetIp {
    pub address: IpAddr,
    /// 用途／備註（已去除前後空白；空白為 None）
    pub purpose: Option<String>,
}

impl fmt::Display for AssetIp {
    /// 「192.0.2.1（管理）」「2001:db8::1」。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.purpose {
            Some(p) => write!(f, "{}（{p}）", self.address),
            None => write!(f, "{}", self.address),
        }
    }
}

/// 畫面或清冊輸入的原文。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpInput<'a> {
    pub address: &'a str,
    pub purpose: &'a str,
}

/// IP 錯誤的種類（讓畫面以自己的用語呈現）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpErrorKind {
    /// 位址空白（用途有填）
    Missing,
    /// 不是單一的 IPv4 或 IPv6 位址
    Invalid,
    /// 與同一資產的另一筆相同
    Duplicate,
}

/// 一筆 IP 的錯誤；`index` 為輸入清單中的位置（從 0 起）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpError {
    pub index: usize,
    pub kind: IpErrorKind,
    pub message: String,
    /// [`IpErrorKind::Duplicate`] 時為先出現那筆的位置
    pub first: Option<usize>,
    /// 原文含 `/`（看起來像網段）
    pub looks_like_network: bool,
}

/// 解析一個位址（已去除前後空白）。
pub fn parse_ip(value: &str) -> Option<IpAddr> {
    IpAddr::from_str(value.trim()).ok()
}

/// 驗證整份 IP 清單；任何一筆有錯時回傳全部錯誤。順序保留。
pub fn validate_ips(inputs: &[IpInput<'_>]) -> Result<Vec<AssetIp>, Vec<IpError>> {
    let mut errors = Vec::new();
    let mut out = Vec::new();
    let mut seen: HashMap<IpAddr, usize> = HashMap::new();
    for (index, input) in inputs.iter().enumerate() {
        let raw = input.address.trim();
        let error = |kind, message, first| IpError {
            index,
            kind,
            message,
            first,
            looks_like_network: raw.contains('/'),
        };
        if raw.is_empty() {
            errors.push(error(IpErrorKind::Missing, "缺少 ip".to_string(), None));
            continue;
        }
        let Some(address) = parse_ip(raw) else {
            let message = if raw.contains('/') {
                format!("ip「{raw}」只能填單一 IPv4 或 IPv6 位址，不能填網段")
            } else {
                format!("ip「{raw}」不是有效的 IPv4 或 IPv6 位址")
            };
            errors.push(error(IpErrorKind::Invalid, message, None));
            continue;
        };
        if let Some(&first) = seen.get(&address) {
            let message = format!("ip「{raw}」與 {address} 重複");
            errors.push(error(IpErrorKind::Duplicate, message, Some(first)));
            continue;
        }
        seen.insert(address, index);
        let purpose = input.purpose.trim();
        out.push(AssetIp {
            address,
            purpose: (!purpose.is_empty()).then(|| purpose.to_string()),
        });
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn ip_from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<AssetIp> {
    let text: String = row.get(at)?;
    let address = parse_ip(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            at,
            rusqlite::types::Type::Text,
            format!("資料庫中的 IP 不合法：{text}").into(),
        )
    })?;
    Ok(AssetIp {
        address,
        purpose: row.get(at + 1)?,
    })
}

/// `asset_ip` 表的讀寫。
pub struct AssetIpRepo<'c> {
    conn: &'c Connection,
}

impl<'c> AssetIpRepo<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        AssetIpRepo { conn }
    }

    /// 一個資產的 IP（依儲存順序）。
    pub fn for_asset(&self, asset_id: i64) -> Result<Vec<AssetIp>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT address, purpose FROM asset_ip WHERE asset_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![asset_id], |r| ip_from_row(r, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 所有資產的 IP，依資產 id 分組（一次查詢）。
    pub fn all_by_asset(&self) -> Result<HashMap<i64, Vec<AssetIp>>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT asset_id, address, purpose FROM asset_ip ORDER BY asset_id, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, ip_from_row(r, 1)?)))?;
        let mut out: HashMap<i64, Vec<AssetIp>> = HashMap::new();
        for row in rows {
            let (id, ip) = row?;
            out.entry(id).or_default().push(ip);
        }
        Ok(out)
    }

    /// 以新清單取代該資產的 IP（依輸入順序寫入；呼叫端負責驗證）。
    pub fn replace(&self, asset_id: i64, ips: &[AssetIp]) -> Result<(), CoreError> {
        self.conn.execute(
            "DELETE FROM asset_ip WHERE asset_id = ?1",
            params![asset_id],
        )?;
        let mut stmt = self
            .conn
            .prepare("INSERT INTO asset_ip (asset_id, address, purpose) VALUES (?1, ?2, ?3)")?;
        for ip in ips {
            stmt.execute(params![asset_id, ip.address.to_string(), ip.purpose])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(address: &'a str, purpose: &'a str) -> IpInput<'a> {
        IpInput { address, purpose }
    }

    #[test]
    fn several_addresses_keep_order_and_canonical_form() {
        let ips = validate_ips(&[
            input("192.0.2.10", " 管理 "),
            input(" 2001:DB8:0:0::10 ", ""),
        ])
        .unwrap();
        let text: Vec<String> = ips.iter().map(ToString::to_string).collect();
        assert_eq!(text, ["192.0.2.10（管理）", "2001:db8::10"]);
        assert_eq!(ips[0].purpose.as_deref(), Some("管理"));
        assert_eq!(ips[1].purpose, None);
    }

    #[test]
    fn ipv6_written_differently_is_a_duplicate() {
        let errors =
            validate_ips(&[input("2001:db8::1", ""), input("2001:DB8:0:0::1", "")]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].index, 1);
        assert_eq!(errors[0].kind, IpErrorKind::Duplicate);
        assert_eq!(errors[0].first, Some(0));
        assert!(
            errors[0].message.contains("與 2001:db8::1 重複"),
            "{errors:?}"
        );
    }

    #[test]
    fn network_instead_of_an_address() {
        let errors = validate_ips(&[input("192.0.2.0/24", "")]).unwrap_err();
        assert_eq!(errors[0].kind, IpErrorKind::Invalid);
        assert!(errors[0].looks_like_network);
        assert!(
            errors[0].message.contains("只能填單一 IPv4 或 IPv6 位址"),
            "{errors:?}"
        );
    }

    #[test]
    fn malformed_addresses_are_rejected() {
        for bad in [
            "192.000.002.010",
            "fe80::1%eth0",
            "192.0.2.300",
            "[2001:db8::1]",
            "192.0.2.1-192.0.2.9",
            "fw01",
        ] {
            let errors = validate_ips(&[input(bad, "")]).unwrap_err();
            assert_eq!(errors[0].kind, IpErrorKind::Invalid, "{bad}");
        }
        let errors = validate_ips(&[input(" ", "管理")]).unwrap_err();
        assert_eq!(errors[0].kind, IpErrorKind::Missing);
        assert_eq!(validate_ips(&[]), Ok(Vec::new()));
    }

    #[test]
    fn ipv4_mapped_ipv6_is_a_different_address() {
        let ips = validate_ips(&[input("192.0.2.1", ""), input("::ffff:192.0.2.1", "")]).unwrap();
        assert_eq!(ips.len(), 2);
    }
}
