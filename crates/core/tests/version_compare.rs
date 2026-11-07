//! 04 任務 1.5：各版本格式的比較案例、不可解析樣本與全序性質。

use std::cmp::Ordering;
use std::collections::BTreeMap;

use cvewatch_core::catalog::Catalog;
use cvewatch_core::matching::version::esxi::parse_release;
use cvewatch_core::matching::version::{compare, Schemes, VersionScheme};
use proptest::prelude::*;

fn schemes() -> Schemes {
    Schemes::from_catalog(&Catalog::seed().expect("seed"))
}

fn ord(symbol: &str) -> Ordering {
    match symbol {
        "<" => Ordering::Less,
        "=" => Ordering::Equal,
        ">" => Ordering::Greater,
        other => panic!("未知符號 {other}"),
    }
}

/// 每組都以 a?b 與 b?a 兩個方向驗證。
fn check(name: &str, cases: &[(&str, &str, &str)]) {
    let all = schemes();
    let scheme = all.get(name).unwrap();
    for (a, symbol, b) in cases {
        let expected = ord(symbol);
        assert_eq!(
            compare(scheme, a, b),
            Some(expected),
            "{name}：{a} {symbol} {b}"
        );
        assert_eq!(
            compare(scheme, b, a),
            Some(expected.reverse()),
            "{name}：反向 {b} vs {a}"
        );
    }
}

fn unparseable(name: &str, samples: &[&str]) {
    let all = schemes();
    let scheme = all.get(name).unwrap();
    for s in samples {
        assert!(scheme.parse(s).is_none(), "{name} 不應解析 `{s}`");
    }
}

#[test]
fn panos_ordering() {
    check(
        "panos",
        &[
            // spec 範例
            ("11.1.4-h7", "<", "11.1.4-h12"),
            ("11.1.4-h12", "<", "11.1.5"),
            ("11.1.4", "<", "11.1.4-h1"),
            ("11.1.4", "=", "11.1.4-h0"),
            ("10.2.13", "<", "11.0.0"),
            ("10.2.9-h1", ">", "10.2.9"),
            ("10.2.10", ">", "10.2.9-h21"),
            ("11.1", "=", "11.1.0"),
            ("11.1.0", "<", "11.1.0-h3"),
            ("11.2.0", ">", "11.1.16-h3"),
            ("9.1.0", "<", "10.0.0"),
            ("11.1.4-H7", "=", "11.1.4-h7"),
            (" 11.1.4-h7 ", "=", "11.1.4-h7"),
            ("12.1.4-h12", ">", "12.1.4-h9"),
            ("10.1.14-h9", "<", "10.2.0"),
            ("11.0.4-h1", "<", "11.0.4-h2"),
        ],
    );
    unparseable(
        "panos",
        &[
            "",
            "11",
            "11.1.4-h",
            "11.1.4-hx",
            "11.1.4h7",
            "All",
            "n/a",
            "11.1.4-xfr",
        ],
    );
}

#[test]
fn esxi_ordering() {
    check(
        "esxi",
        &[
            // spec 範例
            ("8.0 U3c", ">", "8.0 Update 2"),
            ("8.0 U3c", "=", "8.0 Update 3c"),
            ("8.0 U3", "<", "8.0 U3c"),
            ("8.0.3", ">", "8.0 U2"),
            ("8.0.2", "<", "8.0 U3"),
            ("8.0", "<", "8.0 U1"),
            ("8.0", "<", "8.0c"),
            ("8.0c", "<", "8.0 U1"),
            ("7.0 U3", "<", "8.0"),
            ("8.0u3 p01", ">", "8.0 U3"),
            ("8.0 U3 P01", "<", "8.0 U3 P02"),
            ("7.0 U3 EP 1", ">", "7.0 U3"),
            // build 透過種子檔對照表轉換
            ("24414501", "=", "8.0 U3c"),
            ("build-24022510", "<", "8.0 U3c"),
            ("24280767", ">", "8.0 Update 2"),
            ("24414501", ">", "24022510"),
            // 對照不到的 build 仍可與 build 比較
            ("23000000", "<", "24000000"),
            ("8.0.3 build-24414501", ">", "8.0.3 build-24022510"),
        ],
    );
    unparseable(
        "esxi",
        &["", "8", "eight", "8.0 U", "8.0 Update", "U3c", "12345"],
    );
}

#[test]
fn esxi_unknown_build_cannot_be_compared_with_names() {
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(compare(esxi, "23999999", "8.0 U3"), None);
    // 有名稱與只有 build 的版本互不可比較（維持可遞移，避免排序循環）
    assert_eq!(compare(esxi, "8.0.3 build-24414501", "23999999"), None);
}

#[test]
fn dsm_ordering() {
    check(
        "dsm",
        &[
            ("7.2.2-72806 Update 3", ">", "7.2.2-72806 Update 2"),
            ("7.2.2-72806 Update 3", "=", "7.2.2-72806-3"),
            ("7.2.2-72806", "<", "7.2.2-72806 Update 1"),
            ("7.2.1-69057", "<", "7.2.2-72806"),
            ("7.2.1-69057 Update 5", "<", "7.2.2-72806"),
            ("7.2-64570", "<", "7.2.1-69057"),
            ("7.2-64570", "=", "7.2.0-64570"),
            ("7.1.1-42962", "<", "7.2-64570"),
            ("6.2.4-25556", "<", "7.0-41890"),
            ("7.2.2", "<", "7.2.2-72806"),
            ("7.2.2", ">", "7.2.1-69057 Update 6"),
            ("7.2.2-72806update3", "=", "7.2.2-72806 Update 3"),
            ("7.2.2-72806 update_3", "=", "7.2.2-72806 Update 3"),
            ("7.2.2-72806 Update 0", "=", "7.2.2-72806"),
            ("7.2.1-69057-2", "<", "7.2.1-69057-10"),
            ("7.3-81180", ">", "7.2.2-72806 Update 4"),
        ],
    );
    unparseable(
        "dsm",
        &["", "7", "DSM 7.2", "7.2.2-", "7.2.2-72806 Update", "seven"],
    );
}

#[test]
fn dotted_ordering() {
    check(
        "dotted",
        &[
            ("7.16.2", "<", "7.16.10"),
            ("2.16.0", "=", "2.16"),
            ("2.16.0", "<", "2.16.0.1"),
            ("7.10.30.00", ">", "7.10.25.00"),
            ("8.2.2", "<", "8.2.4"),
            ("10.0.20348.2700", ">", "10.0.20348.2655"),
            ("1.0", "<", "1.0a"),
            ("1.0a", "<", "1.0b"),
            ("1.2.3-rc1", "<", "1.2.3"),
            ("1.2.3-rc1", "<", "1.2.4"),
            ("v2.0", "=", "2.0"),
            ("9", "<", "10"),
            ("12.7", ">", "12.6.1"),
            ("0", "<", "0.0.1"),
            ("3.0.11", "<", "3.0.13"),
            ("1.0-beta", "<", "1.0-rc"),
        ],
    );
    unparseable(
        "dotted",
        &[
            "",
            ".1",
            "1..2",
            "abc",
            "-1",
            "n/a",
            "1.2.",
            "1.2-3",
            "1.99999999999999999999",
        ],
    );
}

/// code review #1：預覽版早於正式版、尾碼內數字依數值比較、其他尾碼晚於正式版。
#[test]
fn dotted_pre_release_and_post_release_suffixes() {
    check(
        "dotted",
        &[
            ("3.0.0-beta1", "<", "3.0.0"),
            ("3.0.0-rc1", "<", "3.0.0"),
            ("3.0.0-rc2", "<", "3.0.0-rc10"),
            ("3.0.0-dev", "<", "3.0.0-alpha1"),
            ("3.0.0-alpha2", "<", "3.0.0-beta1"),
            ("3.0.0-beta.2", "<", "3.0.0-preview1"),
            ("3.0.0-pre1", "<", "3.0.0-rc1"),
            ("3.0.0-rc1", ">", "2.9.9"),
            ("1.0.2", "<", "1.0.2a"),
            ("1.0.2a", "<", "1.0.2b"),
            ("1.0.2p1", "<", "1.0.2p10"),
            ("3.0.0rc1", "=", "3.0.0-rc1"),
            ("3.0.0-rc01", "=", "3.0.0-rc1"),
            ("1.0-alphabet", ">", "1.0"),
        ],
    );
}

#[test]
fn ubuntu_ignores_lts_suffix() {
    check(
        "ubuntu",
        &[
            ("22.04.4 LTS", "=", "22.04.4"),
            ("22.04", "<", "22.04.4 LTS"),
            ("24.04", ">", "22.04.4"),
        ],
    );
}

#[test]
fn seed_esxi_build_table_is_valid() {
    let catalog = Catalog::seed().unwrap();
    assert!(!catalog.esxi_builds().is_empty());
    for (build, version) in catalog.esxi_builds() {
        assert!(parse_release(version).is_some(), "{build} → {version}");
    }
}

#[test]
fn user_catalog_overrides_build_and_rejects_bad_names() {
    use cvewatch_core::catalog::{CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
    let seed = CatalogSource {
        name: SEED_SOURCE_NAME.into(),
        text: SEED_TOML.into(),
    };
    let extra = CatalogSource {
        name: "esxi.toml".into(),
        text: "[[esxi_build]]\nbuild = 24585383\nversion = \"8.0 U3d\"\n".into(),
    };
    let catalog = Catalog::from_sources(&[seed, extra]).unwrap();
    let schemes = Schemes::from_catalog(&catalog);
    let esxi = schemes.get("esxi").unwrap();
    assert_eq!(
        compare(esxi, "24585383", "8.0 U3c"),
        Some(Ordering::Greater)
    );

    let bad = CatalogSource {
        name: "bad.toml".into(),
        text: "[[esxi_build]]\nbuild = 1234567\nversion = \"latest\"\n".into(),
    };
    let err = Catalog::from_sources(&[bad]).unwrap_err().to_string();
    assert!(err.contains("bad.toml") && err.contains("latest"), "{err}");
}

// ── 全序性質 ─────────────────────────────────────────────────

fn assert_total_order(scheme: &dyn VersionScheme, a: &str, b: &str, c: &str) {
    let parse = |s: &str| scheme.parse(s).unwrap_or_else(|| panic!("無法解析 {s}"));
    let (x, y, z) = (parse(a), parse(b), parse(c));
    let cmp = |p, q| scheme.cmp(p, q).expect("同格式版本必可比較");
    assert_eq!(cmp(&x, &x), Ordering::Equal, "自反：{a}");
    assert_eq!(cmp(&x, &y), cmp(&y, &x).reverse(), "反對稱：{a} / {b}");
    if cmp(&x, &y) != Ordering::Greater && cmp(&y, &z) != Ordering::Greater {
        assert_ne!(cmp(&x, &z), Ordering::Greater, "遞移：{a} ≤ {b} ≤ {c}");
    }
}

fn panos_version() -> impl Strategy<Value = String> {
    (
        0u8..15,
        0u8..5,
        proptest::option::of(0u8..20),
        proptest::option::of(0u8..40),
    )
        .prop_map(|(ma, mi, mt, hf)| {
            let mut s = format!("{ma}.{mi}");
            if let Some(m) = mt {
                s.push_str(&format!(".{m}"));
            }
            if let Some(h) = hf {
                s.push_str(&format!("-h{h}"));
            }
            s
        })
}

fn dsm_version() -> impl Strategy<Value = String> {
    (
        6u8..8,
        0u8..4,
        proptest::option::of(0u8..4),
        proptest::option::of(40000u32..90000),
        proptest::option::of(0u8..8),
    )
        .prop_map(|(ma, mi, mc, build, upd)| {
            let mut s = format!("{ma}.{mi}");
            if let Some(m) = mc {
                s.push_str(&format!(".{m}"));
            }
            if let Some(b) = build {
                s.push_str(&format!("-{b}"));
            }
            if let Some(u) = upd {
                s.push_str(&format!(" Update {u}"));
            }
            s
        })
}

/// ESXi：名稱形式或種子對照表中的 build（全部可比較）。
fn esxi_version() -> impl Strategy<Value = String> {
    let named = (
        6u8..9,
        0u8..3,
        proptest::option::of(0u8..4),
        proptest::option::of(proptest::char::range('a', 'e')),
        proptest::option::of(1u8..4),
    )
        .prop_map(|(ma, mi, up, letter, patch)| {
            let mut s = format!("{ma}.{mi}");
            if let Some(u) = up {
                s.push_str(&format!(" U{u}"));
                if let Some(l) = letter {
                    s.push(l);
                }
            }
            if let Some(p) = patch {
                s.push_str(&format!(" P0{p}"));
            }
            s
        });
    let builds: Vec<String> = Catalog::seed()
        .unwrap()
        .esxi_builds()
        .keys()
        .map(u64::to_string)
        .collect();
    prop_oneof![named, proptest::sample::select(builds)]
}

fn dotted_version() -> impl Strategy<Value = String> {
    (
        proptest::collection::vec(0u16..30, 1..5),
        proptest::option::of("[a-z]{1,3}[0-9]?"),
    )
        .prop_map(|(nums, suffix)| {
            let base = nums
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(".");
            match suffix {
                Some(s) => format!("{base}-{s}"),
                None => base,
            }
        })
}

proptest! {
    #[test]
    fn panos_is_total_order(a in panos_version(), b in panos_version(), c in panos_version()) {
        assert_total_order(schemes().get("panos").unwrap(), &a, &b, &c);
    }

    #[test]
    fn dsm_is_total_order(a in dsm_version(), b in dsm_version(), c in dsm_version()) {
        assert_total_order(schemes().get("dsm").unwrap(), &a, &b, &c);
    }

    #[test]
    fn esxi_is_total_order(a in esxi_version(), b in esxi_version(), c in esxi_version()) {
        assert_total_order(schemes().get("esxi").unwrap(), &a, &b, &c);
    }

    #[test]
    fn dotted_is_total_order(a in dotted_version(), b in dotted_version(), c in dotted_version()) {
        assert_total_order(schemes().get("dotted").unwrap(), &a, &b, &c);
    }

    /// 任意字串不會讓解析 panic。
    #[test]
    fn parsing_never_panics(s in "\\PC{0,24}") {
        let all = schemes();
        for name in ["dotted", "ubuntu", "panos", "esxi", "dsm"] {
            let _ = all.get(name).unwrap().parse(&s);
        }
    }
}

#[test]
fn empty_build_table_still_compares_names() {
    let schemes = Schemes::new(BTreeMap::new());
    let esxi = schemes.get("esxi").unwrap();
    assert_eq!(compare(esxi, "24414501", "8.0 U3c"), None);
    assert_eq!(compare(esxi, "8.0 U3c", "8.0 U2"), Some(Ordering::Greater));
}

/// code review 第二輪 #2：名稱 + build 時以對照表為準（含 Update 字母）。
#[test]
fn esxi_named_version_with_build_uses_build_table() {
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(
        compare(esxi, "8.0.3 build-24414501", "8.0 U3c"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare(esxi, "8.0.3 build-24414501", "8.0 U3b"),
        Some(Ordering::Greater)
    );
    // 對照不到的 build：Update 字母未知 → 同一 Update 無法判定，不同 Update 仍可比較
    assert_eq!(compare(esxi, "8.0.3 build-23999999", "8.0 U3"), None);
    assert_eq!(compare(esxi, "8.0.3 build-23999999", "8.0 U3d"), None);
    assert_eq!(
        compare(esxi, "8.0.3 build-23999999", "8.0 U2"),
        Some(Ordering::Greater)
    );
    assert_eq!(
        compare(esxi, "8.0.3 build-23999999", "8.0 U4"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare(esxi, "8.0.3 build-23999999", "8.0.3 build-23999999"),
        Some(Ordering::Equal)
    );
    // 名稱已寫明字母時照名稱比較
    assert_eq!(
        compare(esxi, "8.0 U3c build-23999999", "8.0 U3b"),
        Some(Ordering::Greater)
    );
}

/// code review 第十八輪 #6：數字後緊接 `ep`（`7.0u3ep01`）可讀成 Express Patch 或 Update 字母 e + Patch，
/// 不猜 → 無法解析（判定為未知）；有空白分隔時照常解析。
#[test]
fn esxi_ambiguous_ep_without_space_is_unparseable() {
    unparseable("esxi", &["7.0u3ep1", "7.0u3ep01", "8.0ep1", "7.0U3EP 2"]);
    // 第二十輪 #1：清冊驗證也不接受（否則匯入時不警告、比對時永遠未知）
    let validator = cvewatch_core::catalog::scheme::scheme_by_name("esxi").unwrap();
    for text in ["8.0u3ep1", "8.0ep1", "7.0 u3ep 1"] {
        assert!(validator.parse(text).is_none(), "{text}");
    }
    assert!(validator.parse("7.0 u3 ep 1").is_some());
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(
        compare(esxi, "7.0 U3 EP 2", "7.0 U3 EP 1"),
        Some(Ordering::Greater)
    );
    assert_eq!(
        compare(esxi, "7.0 U3 EP 1", "7.0 U3d"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare(esxi, "7.0u3e p01", "7.0 U3e"),
        Some(Ordering::Greater)
    );
    assert_eq!(compare(esxi, "8.0 EP 1", "8.0"), Some(Ordering::Greater));
    // 真正的 Update 字母不受影響
    assert_eq!(compare(esxi, "7.0u3e", "7.0 U3d"), Some(Ordering::Greater));
    assert_eq!(
        compare(esxi, "7.0u3c p01", "7.0 U3c"),
        Some(Ordering::Greater)
    );
}

/// code review 第六輪 #1：Broadcom CVE 紀錄的精簡寫法 `ESXi80U3d-24585383`。
#[test]
fn esxi_compact_vendor_notation() {
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(
        compare(esxi, "ESXi80U3d-24585383", "8.0 U3c"),
        Some(Ordering::Greater)
    );
    assert_eq!(
        compare(esxi, "ESXi80U3c-24414501", "8.0 U3c"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare(esxi, "ESXi80U2", "8.0 Update 2"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare(esxi, "ESXi70U3sP01", "7.0 U3s"),
        Some(Ordering::Greater)
    );
    // build 在對照表中時以對照表為準
    assert_eq!(
        compare(esxi, "ESXi80U3-24414501", "8.0 U3c"),
        Some(Ordering::Equal)
    );
}

/// code review 第十一輪 #2：`8.0.3` 數字寫法沒有表達 Update 字母 → 與同一 Update 的名稱比較時未知。
#[test]
fn esxi_numeric_update_has_unknown_letter() {
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(compare(esxi, "8.0.3", "8.0 U3c"), None);
    assert_eq!(compare(esxi, "8.0.3", "8.0 U3"), None);
    assert_eq!(compare(esxi, "8.0.3", "8.0 U2"), Some(Ordering::Greater));
    assert_eq!(compare(esxi, "8.0.3", "8.0.3"), Some(Ordering::Equal));
    // 寫明字母或 build 在對照表中時照常比較
    assert_eq!(compare(esxi, "8.0.3c", "8.0 U3c"), Some(Ordering::Equal));
    assert_eq!(
        compare(esxi, "8.0.3 build-24414501", "8.0 U3c"),
        Some(Ordering::Equal)
    );
}

/// code review 第十七輪 #1：沒寫 build 但寫了 Update 的 DSM 版本（NVD CPE `7.2.2:update3`）以 Update 排序。
#[test]
fn dsm_update_without_build_orders_by_update() {
    check(
        "dsm",
        &[
            ("7.2.2-72806 Update 1", "<", "7.2.2 Update 3"),
            ("7.2.2-72806 Update 4", ">", "7.2.2 Update 3"),
            ("7.2.2 Update 3", "<", "7.2.2-72806 Update 3"),
            ("7.2.2", "<", "7.2.2-72806"),
        ],
    );
}

/// code review 第十七輪 #2：PAN-OS 只有下界的變更點只適用同一功能版本線。
#[test]
fn panos_change_points_stay_within_feature_line() {
    let all = schemes();
    let panos = all.get("panos").unwrap();
    let applies = |at: &str, v: &str| {
        let (a, x) = (panos.parse(at).unwrap(), panos.parse(v).unwrap());
        panos.change_applies(&a, &x)
    };
    assert_eq!(applies("11.1.0", "11.1.6"), Some(true));
    assert_eq!(applies("11.1.0", "11.2.3"), Some(false));
    assert_eq!(applies("11.1.4-h7", "11.1.4-h9"), Some(true));
    assert_eq!(applies("11.1.4-h7", "11.1.5"), Some(false));
    assert_eq!(applies("11.1.4", "11.1.3"), Some(false));
}

/// code review 第十七輪 #3：同一 Update、字母都未知的 ESXi 版本以 build 比較。
#[test]
fn esxi_unknown_letters_on_same_update_compare_builds() {
    let all = schemes();
    let esxi = all.get("esxi").unwrap();
    assert_eq!(
        compare(esxi, "8.0.3 build-30000001", "8.0.3 build-30000002"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare(esxi, "8.0.3 build-30000002", "8.0.3 build-30000002"),
        Some(Ordering::Equal)
    );
    // 一邊沒有 build 時仍無法判斷
    assert_eq!(compare(esxi, "8.0.3", "8.0.3 build-30000002"), None);
}

/// code review 第十一輪 #3：目錄的 build 對照表接受 `8.0c` 這類 GA 修補名稱。
#[test]
fn catalog_accepts_ga_patch_letter_names() {
    use cvewatch_core::catalog::{CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
    let catalog = Catalog::from_sources(&[
        CatalogSource {
            name: SEED_SOURCE_NAME.into(),
            text: SEED_TOML.into(),
        },
        CatalogSource {
            name: "ga.toml".into(),
            text: "[[esxi_build]]
build = 21493926
version = \"8.0c\"
"
            .into(),
        },
    ])
    .unwrap();
    let schemes = Schemes::from_catalog(&catalog);
    let esxi = schemes.get("esxi").unwrap();
    assert_eq!(compare(esxi, "21493926", "8.0 U1"), Some(Ordering::Less));
}

/// code review 第十二輪 #3：目錄的 build 對照表不接受 `8.0.3`（無法表達 Update 字母）。
#[test]
fn catalog_rejects_numeric_update_build_names() {
    use cvewatch_core::catalog::CatalogSource;
    let err = Catalog::from_sources(&[CatalogSource {
        name: "num.toml".into(),
        text: "[[esxi_build]]\nbuild = 24414501\nversion = \"8.0.3\"\n".into(),
    }])
    .unwrap_err()
    .to_string();
    assert!(err.contains("8.0.3") && err.contains("8.0 U3c"), "{err}");
}
