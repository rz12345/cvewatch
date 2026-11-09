//! 清冊的 `ips` 工作表（CSV 的 `--ips` 檔；20 design D5）：逐列讀取、依 `asset_tag` 分組，
//! 每個資產的 IP 以 [`validate_ips`] 驗證，錯誤換成工作表與列號。

use std::collections::{BTreeMap, HashMap, HashSet};

use super::columns::{SheetKind, StandardField};
use super::ips::{validate_ips, AssetIp, IpInput};
use super::sheet::Sheet;
use super::validate::{RowReader, RowRef};

/// 清冊中的 IP。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IpsInput {
    /// 不處理 IP，也不提醒（ISMS 匯入使用）
    #[default]
    Untouched,
    /// 清冊沒有 `ips` 工作表（或 CSV 沒有 `--ips`）：保留既有 IP，需要時提醒
    Missing,
    /// asset_tag → 該資產的 IP（依列序）；清冊中沒有列出的資產不在表中（寫入時為空清單）
    Sheet(BTreeMap<String, Vec<AssetIp>>),
}

/// 一筆 IP 列（驗證前）。
struct IpRow<'a> {
    at: RowRef,
    address: &'a str,
    purpose: &'a str,
}

/// 讀取 `ips` 工作表。`incoming` 為 assets 工作表中驗證通過的資產編號，`known` 為 assets
/// 工作表中出現過的所有編號（該資產列本身有錯時不重複提示）。
pub(super) fn parse_ips(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    incoming: &HashSet<String>,
    known: &HashSet<String>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, Vec<AssetIp>> {
    let mut grouped: BTreeMap<String, Vec<IpRow<'_>>> = BTreeMap::new();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Ips,
            number: row.number,
        };
        let reader = RowReader { row, at, columns };
        let tag = reader.get(StandardField::AssetTag);
        let address = reader.get(StandardField::Ip);
        let purpose = reader.get(StandardField::Purpose);
        let before = errors.len();
        if tag.is_empty() {
            errors.push(format!("{at}：缺少 asset_tag"));
        }
        if address.is_empty() {
            errors.push(format!("{at}：缺少 ip"));
        }
        if errors.len() != before {
            continue;
        }
        if incoming.contains(tag) {
            grouped.entry(tag.to_string()).or_default().push(IpRow {
                at,
                address,
                purpose,
            });
        } else if !known.contains(tag) {
            errors.push(format!("{at}：asset_tag「{tag}」不在 assets 工作表中"));
        }
    }
    grouped
        .into_iter()
        .filter_map(|(tag, rows)| {
            let inputs: Vec<IpInput<'_>> = rows
                .iter()
                .map(|r| IpInput {
                    address: r.address,
                    purpose: r.purpose,
                })
                .collect();
            match validate_ips(&inputs) {
                Ok(ips) => Some((tag, ips)),
                Err(ip_errors) => {
                    for e in ip_errors {
                        let at = rows[e.index].at;
                        let message = match e.first {
                            Some(first) => format!(
                                "{at}：{tag} 的 ip「{}」與第 {} 列重複",
                                rows[e.index].address, rows[first].at.number
                            ),
                            None => format!("{at}：{}", e.message),
                        };
                        errors.push(message);
                    }
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::columns::ColumnMap;
    use crate::assets::sheet::SheetRow;

    fn sheet(rows: &[&[&str]]) -> Sheet {
        Sheet {
            kind: SheetKind::Ips,
            headers: ["asset_tag", "ip", "purpose"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, r)| SheetRow {
                    number: i + 2,
                    cells: r.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
        }
    }

    fn parse(rows: &[&[&str]]) -> (BTreeMap<String, Vec<AssetIp>>, Vec<String>) {
        let s = sheet(rows);
        let cols = ColumnMap::default()
            .locate(&s.headers, SheetKind::Ips)
            .unwrap();
        let incoming: HashSet<String> = ["SRV-01", "NAS-01"].map(String::from).into();
        let known: HashSet<String> = ["SRV-01", "NAS-01", "BAD-01"].map(String::from).into();
        let mut errors = Vec::new();
        let out = parse_ips(&s, &cols, &incoming, &known, &mut errors);
        (out, errors)
    }

    #[test]
    fn rows_are_grouped_by_asset_in_order() {
        let (out, errors) = parse(&[
            &["SRV-01", "192.0.2.10", "管理"],
            &["NAS-01", "192.0.2.20", ""],
            &["SRV-01", "2001:DB8::10", ""],
        ]);
        assert!(errors.is_empty(), "{errors:?}");
        let srv: Vec<String> = out["SRV-01"].iter().map(ToString::to_string).collect();
        assert_eq!(srv, ["192.0.2.10（管理）", "2001:db8::10"]);
        assert_eq!(out["NAS-01"].len(), 1);
    }

    #[test]
    fn asset_not_in_the_assets_sheet() {
        let (_, errors) = parse(&[
            &["SRV-01", "192.0.2.10", ""],
            &["BAD-01", "192.0.2.11", ""],
            &["SRV-99", "192.0.2.12", ""],
        ]);
        assert_eq!(
            errors,
            ["ips 工作表第 4 列：asset_tag「SRV-99」不在 assets 工作表中"],
            "資產列本身有錯（BAD-01）時不重複提示"
        );
    }

    #[test]
    fn invalid_and_duplicate_addresses_carry_their_rows() {
        let (out, errors) = parse(&[
            &["SRV-01", "192.0.2.10", ""],
            &["SRV-01", "192.0.2.300", ""],
            &["NAS-01", "2001:db8::1", ""],
            &["NAS-01", "2001:DB8:0:0::1", ""],
            &["NAS-01", "", "管理"],
        ]);
        assert_eq!(
            errors,
            [
                "ips 工作表第 6 列：缺少 ip",
                "ips 工作表第 5 列：NAS-01 的 ip「2001:DB8:0:0::1」與第 4 列重複",
                "ips 工作表第 3 列：ip「192.0.2.300」不是有效的 IPv4 或 IPv6 位址",
            ]
        );
        assert!(out.is_empty());
    }

    #[test]
    fn missing_ip_column_is_a_header_error() {
        let err = ColumnMap::default()
            .locate(
                &["asset_tag".to_string(), "purpose".to_string()],
                SheetKind::Ips,
            )
            .unwrap_err();
        assert!(err.to_string().contains("ips 工作表缺少欄位：ip"), "{err}");
    }
}
