//! 版本比較器：依產品目錄宣告的 `version_scheme` 解析並比較版本。
//!
//! 解析失敗回傳 None，比較無法判定（例如 ESXi 對照不到的 build 與 Update 名稱）也回傳 None；
//! 呼叫端一律視為「未知」，不猜測。

pub mod cisco;
pub mod dotted;
pub mod dsm;
pub mod esxi;
pub mod junos;
pub mod numeric;
pub mod panos;
pub mod zyxel;

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::catalog::Catalog;

/// 可比較的正規化版本。只有同一 scheme 解析出的版本才能互相比較。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    Dotted(dotted::Dotted),
    Panos(panos::Panos),
    Esxi(esxi::Esxi),
    Dsm(dsm::Dsm),
    Cisco(cisco::Cisco),
    Junos(junos::Junos),
    Numeric(numeric::Numeric),
    Zyxel(zyxel::Zyxel),
}

pub trait VersionScheme: Send + Sync {
    fn name(&self) -> &'static str;
    /// 無法以此格式解析時為 None。
    fn parse(&self, raw: &str) -> Option<Version>;
    /// 兩者無法比較時為 None（不同 scheme 的版本、或 ESXi 對照不到的 build）。
    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering>;

    /// `bound` 是否涵蓋 `v`：用於「版本 = bound」與「≤ bound」的邊界。
    /// 預設為相等；DSM 中較不具體的版本（沒寫 build 或 Update）涵蓋整條版本線。
    fn covers(&self, bound: &Version, v: &Version) -> Option<bool> {
        Some(self.cmp(v, bound)? == Ordering::Equal)
    }

    /// 狀態變更點 `at`（CVE 5.x `changes[].at`、CSAF 只有下界的區間）是否適用於 `v`。
    /// 預設為線性：`at ≤ v`。PAN-OS 的變更點只適用同一功能版本線（`>=11.1.0` 不涵蓋 11.2），
    /// hotfix 變更點只適用同一維護版本線（`11.1.0-h3` 修正不代表 `11.1.1` 已修正）。
    fn change_applies(&self, at: &Version, v: &Version) -> Option<bool> {
        Some(self.cmp(at, v)? != Ordering::Greater)
    }

    /// `v` 是否屬於 `base` 的版本線、任何 update／hotfix 皆可（NVD CPE 的 update 欄位為 `*`）。
    /// 預設同 [`VersionScheme::covers`]；PAN-OS 忽略 hotfix，ESXi 忽略未寫明的 Update，dotted 忽略尾碼。
    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        self.covers(base, v)
    }
}

/// 數字段逐段比較，較短者補 0（dotted 與 numeric 共用）。
pub(crate) fn cmp_padded(a: &[u64], b: &[u64]) -> Ordering {
    let len = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..len)
        .map(|i| at(a, i).cmp(&at(b, i)))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// `v ≤ bound`（含 bound 涵蓋的版本線）。
pub fn at_most(scheme: &dyn VersionScheme, v: &Version, bound: &Version) -> Option<bool> {
    match scheme.cmp(v, bound)? {
        Ordering::Less => Some(true),
        _ => scheme.covers(bound, v),
    }
}

pub(crate) use crate::version_text::canonical;

/// 所有內建 scheme；ESXi 需要目錄中的 build 對照表。
pub struct Schemes {
    dotted: dotted::DottedScheme,
    ubuntu: dotted::DottedScheme,
    panos: panos::PanosScheme,
    esxi: esxi::EsxiScheme,
    dsm: dsm::DsmScheme,
    cisco: cisco::CiscoScheme,
    junos: junos::JunosScheme,
    numeric: numeric::NumericScheme,
    zyxel: zyxel::ZyxelScheme,
}

impl Schemes {
    pub fn new(esxi_builds: BTreeMap<u64, esxi::Release>) -> Self {
        Schemes {
            dotted: dotted::DottedScheme::dotted(),
            ubuntu: dotted::DottedScheme::ubuntu(),
            panos: panos::PanosScheme,
            esxi: esxi::EsxiScheme::new(esxi_builds),
            dsm: dsm::DsmScheme,
            cisco: cisco::CiscoScheme,
            junos: junos::JunosScheme,
            numeric: numeric::NumericScheme,
            zyxel: zyxel::ZyxelScheme,
        }
    }

    /// 以目錄的 ESXi build 對照表建立（載入目錄時已驗證格式）。
    pub fn from_catalog(catalog: &Catalog) -> Self {
        let builds = catalog
            .esxi_builds()
            .iter()
            .map(|(build, version)| {
                // 目錄載入時已以同一套文法驗證（version_text::esxi::is_release_name）
                let release =
                    esxi::parse_release(version).expect("目錄已驗證 ESXi build 對照的版本名稱");
                (*build, release)
            })
            .collect();
        Self::new(builds)
    }

    pub fn get(&self, name: &str) -> Option<&dyn VersionScheme> {
        let all: [&dyn VersionScheme; 9] = [
            &self.dotted,
            &self.ubuntu,
            &self.panos,
            &self.esxi,
            &self.dsm,
            &self.cisco,
            &self.junos,
            &self.numeric,
            &self.zyxel,
        ];
        all.into_iter().find(|s| s.name() == name)
    }
}

/// 以 scheme 比較兩個版本字串；任一方無法解析或無法比較時為 None。
pub fn compare(scheme: &dyn VersionScheme, a: &str, b: &str) -> Option<Ordering> {
    scheme.cmp(&scheme.parse(a)?, &scheme.parse(b)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_collapses_and_lowercases() {
        assert_eq!(canonical("  8.0   Update_3C "), "8.0 update 3c");
    }

    #[test]
    fn every_catalog_scheme_has_a_comparator() {
        let schemes = Schemes::new(BTreeMap::new());
        for name in crate::catalog::scheme::scheme_names() {
            assert!(schemes.get(name).is_some(), "缺少 {name} 比較器");
        }
        assert!(schemes.get("nope").is_none());
    }

    #[test]
    fn versions_of_different_schemes_are_not_comparable() {
        let schemes = Schemes::new(BTreeMap::new());
        let panos = schemes.get("panos").unwrap();
        let dotted = schemes.get("dotted").unwrap();
        let a = panos.parse("11.1.4").unwrap();
        let b = dotted.parse("11.1.4").unwrap();
        assert_eq!(panos.cmp(&a, &b), None);
        assert_eq!(dotted.cmp(&a, &b), None);
    }

    /// 02 匯入時以正規表示式判斷「可解析」；這些樣本比較器也必須能解析，
    /// 否則清冊接受的版本在比對時會一律變成未知。
    #[test]
    fn comparators_accept_what_catalog_validation_accepts() {
        let samples = [
            (
                "dotted",
                vec!["7.10.30.00", "2.16.0", "10.0.20348.2700", "8"],
            ),
            ("panos", vec!["11.1.4", "11.1.4-h7", "10.2.13"]),
            (
                "dsm",
                vec![
                    "7.2.1-69057",
                    "7.2.2-72806 update 3",
                    "7.2-64570",
                    "7.2.2-72806update3",
                ],
            ),
            (
                "esxi",
                vec![
                    "8.0 u3c",
                    "8.0 update 3c",
                    "8.0.3",
                    "8.0u3 p01",
                    "24414501",
                    "build-24414501",
                    "8.0.3 build-24414501",
                    "8.0 (build 24414501)",
                    "7.0 u3 ep 1",
                    "7.0u3e p01",
                ],
            ),
            ("ubuntu", vec!["22.04", "22.04.4", "22.04.4 lts"]),
            (
                "cisco",
                vec![
                    "15.2(7)e8",
                    "9.18(4)22",
                    "10.2(3t)",
                    "12.4(25)",
                    "version 15.2(7)e8",
                ],
            ),
            (
                "junos",
                vec!["21.4r3-s5", "22.7r2.4", "21.4", "15.1x49-d200"],
            ),
            (
                "numeric",
                vec![
                    "7.0.1-5145",
                    "14.1-29.72",
                    "5.1.7.2770 build 20240520",
                    "r81.20",
                    "h5.1.7",
                    "15 build 41800",
                    "5.1.x",
                ],
            ),
            ("zyxel", vec!["v2.80(abcd.1)c0", "2.80(abcd.1)c0"]),
        ];
        let schemes = Schemes::new(BTreeMap::new());
        for (name, versions) in samples {
            let validator = crate::catalog::scheme::scheme_by_name(name).unwrap();
            let comparator = schemes.get(name).unwrap();
            for v in versions {
                assert!(validator.parse(v).is_some(), "{name} 驗證應接受 {v}");
                assert!(comparator.parse(v).is_some(), "{name} 比較器應能解析 {v}");
            }
        }
    }
}
