//! ISMS 子類別（13 design D1–D3）：子類別代碼、大類、C/I/A 與子類別表。
//!
//! 內建 ISMS 資訊資產清冊 v2.1 的 HW01–HW11、SW01–SW07；設定檔 `[asset_classes.<代碼>]`
//! 可覆寫名稱、描述與範本值，也可新增子類別（[`AssetClasses::from_config`]）。

use std::collections::BTreeMap;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use unicode_normalization::UnicodeNormalization;

use super::columns::StandardField;
use crate::error::CoreError;

/// C/I/A 的允許範圍。
pub const MIN_RATING: u8 = 1;
pub const MAX_RATING: u8 = 4;

/// 大類：由子類別代碼前兩碼推得。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MajorClass {
    Hardware,
    Software,
}

impl MajorClass {
    pub const ALL: [MajorClass; 2] = [MajorClass::Hardware, MajorClass::Software];

    /// 代碼前綴（`HW`、`SW`）。
    pub fn as_str(self) -> &'static str {
        match self {
            MajorClass::Hardware => "HW",
            MajorClass::Software => "SW",
        }
    }

    /// 介面名稱（「實體」「軟體」）。
    pub fn label(self) -> &'static str {
        match self {
            MajorClass::Hardware => "實體",
            MajorClass::Software => "軟體",
        }
    }

    /// `HW`、`SW`（不分大小寫、去除前後空白）。
    pub fn parse(raw: &str) -> Option<MajorClass> {
        let v = raw.trim();
        Self::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(v))
    }
}

/// 已驗證的子類別代碼：`HW` 或 `SW` 加兩位數字，存為大寫。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassCode(String);

impl ClassCode {
    /// 解析代碼（不分大小寫、去除前後空白，全形字先以 NFKC 轉為半形）；格式不符時為 None。
    pub fn parse(raw: &str) -> Option<ClassCode> {
        let v = nfkc(raw).to_ascii_uppercase();
        let (prefix, digits) = v.split_at_checked(2)?;
        let ok = MajorClass::parse(prefix).is_some()
            && digits.len() == 2
            && digits.bytes().all(|b| b.is_ascii_digit());
        ok.then_some(ClassCode(v))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn major(&self) -> MajorClass {
        if self.0.starts_with("SW") {
            MajorClass::Software
        } else {
            MajorClass::Hardware
        }
    }
}

impl fmt::Display for ClassCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 機密性、完整性、可用性（各為 1–4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cia {
    pub confidentiality: u8,
    pub integrity: u8,
    pub availability: u8,
}

impl Cia {
    pub const fn new(confidentiality: u8, integrity: u8, availability: u8) -> Cia {
        Cia {
            confidentiality,
            integrity,
            availability,
        }
    }

    /// 資產價值 = MAX(C, I, A)。
    pub fn value(&self) -> u8 {
        self.confidentiality
            .max(self.integrity)
            .max(self.availability)
    }

    /// `[C, I, A]`（與 `validate::RATING_FIELDS` 同順序）。
    pub fn to_array(&self) -> [u8; 3] {
        [self.confidentiality, self.integrity, self.availability]
    }

    /// 某個 C/I/A 欄位的值；`field` 不是 C/I/A 時為 None。
    pub fn rating(&self, field: StandardField) -> Option<u8> {
        match field {
            StandardField::Confidentiality => Some(self.confidentiality),
            StandardField::Integrity => Some(self.integrity),
            StandardField::Availability => Some(self.availability),
            _ => None,
        }
    }
}

impl fmt::Display for Cia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            self.confidentiality, self.integrity, self.availability
        )
    }
}

/// 一個子類別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetClass {
    pub code: ClassCode,
    pub name: String,
    pub description: String,
    /// C/I/A 範本值；None 表示依個案評定（C/I/A 必填，例如 SW01）
    pub template: Option<Cia>,
    /// 是否含個人資料的預設值（清冊或編修留空時帶入；16 design D6）
    pub personal_data: bool,
}

impl AssetClass {
    /// 「HW01 伺服器」。
    pub fn label(&self) -> String {
        format!("{} {}", self.code, self.name)
    }
}

/// v2.1 選項工作表（代碼、名稱、描述、C/I/A；SW01 資產價值為 0，無範本值）。
const BUILTIN: [(&str, &str, &str, Option<Cia>); 18] = [
    ("HW01", "伺服器", "大型主機、刀鋒主機、高效能計算主機等", Some(Cia::new(2, 3, 4))),
    ("HW02", "通訊設備", "有線或無線路由器、交換器、閘道器、集線器、無線AP、電話、電報交換機或傳真機等", Some(Cia::new(2, 3, 4))),
    ("HW03", "資安設備", "防火牆、NGFW、IPS或WAF等", Some(Cia::new(2, 3, 4))),
    ("HW04", "週邊設備", "Barcode掃描器、讀卡機、外接燒錄機、隨身碟、外接式硬碟...等", Some(Cia::new(2, 3, 3))),
    ("HW05", "儲存設備", "NAS或Storage", Some(Cia::new(2, 3, 4))),
    ("HW06", "個人電腦", "OA辦公用桌上電腦、筆記型電腦", Some(Cia::new(2, 3, 3))),
    ("HW07", "公共使用電腦", "臨時借用或公用電腦", Some(Cia::new(2, 3, 2))),
    ("HW08", "日常作業事務機器", "印表機、掃瞄器、傳真機、影印機", Some(Cia::new(2, 3, 3))),
    ("HW09", "行動裝置", "公務用平板電腦、智慧型手機、穿戴裝置", Some(Cia::new(2, 3, 3))),
    ("HW10", "物聯網設備", "門禁設備、空拍機、監視器、戶外裝置..等", Some(Cia::new(2, 2, 3))),
    ("HW11", "基礎設施", "UPS、機櫃、發電機、冷氣、消防或環控設備等", Some(Cia::new(3, 3, 4))),
    ("SW01", "應用系統", "自行或委外開發之系統及網站", None),
    ("SW02", "作業系統", "Windows、Linux或VMWare等作業平台(註記版本及名稱)", Some(Cia::new(2, 3, 3))),
    ("SW03", "資料庫管理系統", "MS SQL、MY SQL或Oracle等(註記版本及名稱)", Some(Cia::new(2, 3, 3))),
    ("SW04", "網站服務系統", "IIS、Jboss、Apache等(註記版本及名稱)", Some(Cia::new(2, 3, 3))),
    ("SW05", "應用工具軟體", "開發工具軟體、架設網站工具、磁碟格式化、磁碟重組、虛擬主機軟體或FTP軟體…等(註記版本及名稱)", Some(Cia::new(2, 3, 3))),
    ("SW06", "資通安全系統", "網路封包解析、資產盤點或端點安全等", Some(Cia::new(2, 3, 3))),
    ("SW07", "一般套裝軟體", "OFFICE、PDF、壓縮軟體、遠端遙控、影音撥放軟體、防毒軟體、專案管理或備份軟體等(註記版本及名稱)", Some(Cia::new(2, 2, 2))),
];

/// 舊版 category 值 → 子類別代碼（design D3；只在清冊匯入接受）。
pub const LEGACY_CATEGORIES: [(&str, &str); 6] = [
    ("network", "HW02"),
    ("security", "HW03"),
    ("virtualization", "HW01"),
    ("server", "HW01"),
    ("storage", "HW05"),
    ("host", "HW01"),
];

/// 設定檔 `[asset_classes.<代碼>]` 的一筆（只覆寫有寫的欄位）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct AssetClassConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub confidentiality: Option<u8>,
    pub integrity: Option<u8>,
    pub availability: Option<u8>,
    /// 是否含個人資料的預設值（未寫時沿用內建值；新增的子類別預設為否）
    pub personal_data: Option<bool>,
}

/// [`AssetClasses::resolve`] 的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub code: ClassCode,
    /// 由舊版 category 值轉換時為原值
    pub legacy: Option<String>,
}

fn nfkc(s: &str) -> String {
    s.trim().nfkc().collect()
}

/// 目前生效的子類別表（依代碼排序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetClasses {
    classes: Vec<AssetClass>,
}

impl Default for AssetClasses {
    fn default() -> Self {
        AssetClasses::builtin()
    }
}

impl AssetClasses {
    /// 內建的 18 個子類別。
    pub fn builtin() -> AssetClasses {
        let classes = BUILTIN
            .iter()
            .map(|(code, name, description, template)| AssetClass {
                code: ClassCode(code.to_string()),
                name: name.to_string(),
                description: description.to_string(),
                template: *template,
                // v2.1 選項工作表的 HW／SW 列皆為 N
                personal_data: false,
            })
            .collect();
        AssetClasses { classes }
    }

    /// 內建子類別套用設定檔的覆寫與新增（design D2）；錯誤訊息指出代碼。
    pub fn from_config(
        overrides: &BTreeMap<String, AssetClassConfig>,
    ) -> Result<AssetClasses, CoreError> {
        let mut by_code: BTreeMap<ClassCode, AssetClass> = AssetClasses::builtin()
            .classes
            .into_iter()
            .map(|c| (c.code.clone(), c))
            .collect();
        let mut seen: BTreeMap<ClassCode, &str> = BTreeMap::new();
        for (raw, cfg) in overrides {
            let code = ClassCode::parse(raw).ok_or_else(|| {
                invalid(format!(
                    "[asset_classes] 的代碼 `{}` 必須以 HW 或 SW 開頭並接兩位數字（例如 HW12）",
                    raw.trim()
                ))
            })?;
            if let Some(first) = seen.insert(code.clone(), raw) {
                return Err(invalid(format!(
                    "[asset_classes] 的 `{first}` 與 `{raw}` 是同一個代碼 {code}"
                )));
            }
            let merged = merge(&code, by_code.get(&code), cfg)?;
            by_code.insert(code, merged);
        }
        let classes: Vec<AssetClass> = by_code.into_values().collect();
        ensure_unique_names(&classes)?;
        Ok(AssetClasses { classes })
    }

    pub fn all(&self) -> &[AssetClass] {
        &self.classes
    }

    pub fn get(&self, code: &ClassCode) -> Option<&AssetClass> {
        self.classes.iter().find(|c| &c.code == code)
    }

    /// 「HW01 伺服器」；已不再定義的代碼（例如設定檔刪除了自訂子類別）為「HW12（未定義）」。
    pub fn label(&self, code: &ClassCode) -> String {
        self.get(code)
            .map_or_else(|| format!("{code}（未定義）"), AssetClass::label)
    }

    /// 依代碼（不分大小寫）、名稱（NFKC 後相同）解析；`allow_legacy` 時再試舊版 category 值。
    /// 不屬於已定義子類別時為 None。
    pub fn resolve(&self, raw: &str, allow_legacy: bool) -> Option<Resolved> {
        let v = raw.trim();
        if v.is_empty() {
            return None;
        }
        let found = |code: &ClassCode| self.get(code).map(|c| c.code.clone());
        let direct = ClassCode::parse(v)
            .and_then(|code| found(&code))
            .or_else(|| {
                let name = nfkc(v);
                self.classes
                    .iter()
                    .find(|c| nfkc(&c.name) == name)
                    .map(|c| c.code.clone())
            });
        if let Some(code) = direct {
            return Some(Resolved { code, legacy: None });
        }
        if !allow_legacy {
            return None;
        }
        let (old, code) = LEGACY_CATEGORIES
            .iter()
            .find(|(old, _)| old.eq_ignore_ascii_case(v))?;
        let code = found(&ClassCode(code.to_string()))?;
        Some(Resolved {
            code,
            legacy: Some(old.to_string()),
        })
    }

    /// 錯誤訊息用的已定義代碼清單（「HW01、HW02…」）。
    pub fn allowed(&self) -> String {
        self.classes
            .iter()
            .map(|c| c.code.as_str())
            .collect::<Vec<_>>()
            .join("、")
    }
}

fn invalid(message: String) -> CoreError {
    CoreError::Invalid(message)
}

fn rating(code: &ClassCode, label: &str, v: Option<u8>) -> Result<Option<u8>, CoreError> {
    match v {
        Some(n) if !(MIN_RATING..=MAX_RATING).contains(&n) => Err(invalid(format!(
            "[asset_classes.{code}] 的{label} {n} 必須是 {MIN_RATING}–{MAX_RATING}"
        ))),
        other => Ok(other),
    }
}

/// 套用一筆設定：覆寫內建子類別時沒寫的欄位沿用內建值；新增時 name 必填。
/// C/I/A 在沒有基準範本值時必須三項全寫或全不寫。
fn merge(
    code: &ClassCode,
    base: Option<&AssetClass>,
    cfg: &AssetClassConfig,
) -> Result<AssetClass, CoreError> {
    let c = rating(code, "機密性", cfg.confidentiality)?;
    let i = rating(code, "完整性", cfg.integrity)?;
    let a = rating(code, "可用性", cfg.availability)?;
    let base_template = base.and_then(|b| b.template);
    let template = match (base_template, c, i, a) {
        (Some(t), ..) => Some(Cia::new(
            c.unwrap_or(t.confidentiality),
            i.unwrap_or(t.integrity),
            a.unwrap_or(t.availability),
        )),
        (None, Some(c), Some(i), Some(a)) => Some(Cia::new(c, i, a)),
        (None, None, None, None) => None,
        (None, ..) => {
            return Err(invalid(format!(
                "[asset_classes.{code}] 的 confidentiality、integrity、availability 必須三項全寫或全不寫"
            )))
        }
    };
    let name = match (&cfg.name, base) {
        (Some(n), _) if ClassCode::parse(n).is_some() => {
            // 解析時先比對代碼，代碼形式的名稱永遠選不到這個子類別
            return Err(invalid(format!(
                "[asset_classes.{code}] 的 name「{}」不可是子類別代碼的形式",
                n.trim()
            )));
        }
        (Some(n), _) if !n.trim().is_empty() => n.trim().to_string(),
        (Some(_), _) => return Err(invalid(format!("[asset_classes.{code}] 的 name 不可空白"))),
        (None, Some(b)) => b.name.clone(),
        (None, None) => {
            return Err(invalid(format!(
                "[asset_classes.{code}] 是新增的子類別，必須填寫 name"
            )))
        }
    };
    let description = cfg
        .description
        .as_ref()
        .map(|d| d.trim().to_string())
        .or_else(|| base.map(|b| b.description.clone()))
        .unwrap_or_default();
    let personal_data = cfg
        .personal_data
        .or_else(|| base.map(|b| b.personal_data))
        .unwrap_or(false);
    Ok(AssetClass {
        code: code.clone(),
        name,
        description,
        template,
        personal_data,
    })
}

/// 設定檔 `[asset_classes]` 的反序列化：載入設定時即驗證（代碼格式、新增時 name 必填、
/// C/I/A 全寫或全不寫、名稱不重複），錯誤訊息含欄位路徑與代碼。
pub fn deserialize_config<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, AssetClassConfig>, D::Error> {
    let map = BTreeMap::<String, AssetClassConfig>::deserialize(d)?;
    AssetClasses::from_config(&map).map_err(|e| D::Error::custom(e.to_string()))?;
    Ok(map)
}

fn ensure_unique_names(classes: &[AssetClass]) -> Result<(), CoreError> {
    let mut seen: BTreeMap<String, &ClassCode> = BTreeMap::new();
    for c in classes {
        if let Some(other) = seen.insert(nfkc(&c.name), &c.code) {
            return Err(invalid(format!(
                "[asset_classes] 的子類別名稱「{}」重複（{other} 與 {}）",
                c.name, c.code
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(s: &str) -> ClassCode {
        ClassCode::parse(s).unwrap()
    }

    fn config(entries: &[(&str, AssetClassConfig)]) -> BTreeMap<String, AssetClassConfig> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn builtin_has_eighteen_classes_with_templates() {
        let classes = AssetClasses::builtin();
        assert_eq!(classes.all().len(), 18);
        let hw01 = classes.get(&code("HW01")).unwrap();
        assert_eq!(hw01.template, Some(Cia::new(2, 3, 4)));
        assert_eq!(hw01.label(), "HW01 伺服器");
        assert_eq!(classes.get(&code("SW01")).unwrap().template, None);
        assert_eq!(classes.get(&code("HW11")).unwrap().name, "基礎設施");
        assert_eq!(Cia::new(2, 3, 4).value(), 4);
        assert_eq!(Cia::new(2, 2, 2).value(), 2);
    }

    #[test]
    fn class_code_parsing() {
        assert_eq!(ClassCode::parse("hw05").unwrap().as_str(), "HW05");
        assert_eq!(code(" sw01 ").major(), MajorClass::Software);
        assert_eq!(code("HW03").major(), MajorClass::Hardware);
        for bad in ["IF01", "PE06", "HW1", "HW001", "HWAB", "", "伺服器"] {
            assert_eq!(ClassCode::parse(bad), None, "{bad}");
        }
        assert_eq!(MajorClass::parse("sw"), Some(MajorClass::Software));
        assert_eq!(code("ＨＷ０１").as_str(), "HW01", "全形代碼");
        assert_eq!(
            AssetClasses::builtin()
                .resolve("ｓｗ０２", false)
                .unwrap()
                .code
                .as_str(),
            "SW02"
        );
    }

    #[test]
    fn label_marks_undefined_codes() {
        let classes = AssetClasses::builtin();
        assert_eq!(classes.label(&code("HW03")), "HW03 資安設備");
        assert_eq!(classes.label(&code("HW12")), "HW12（未定義）");
        let cia = Cia::new(2, 3, 4);
        assert_eq!(cia.to_array(), [2, 3, 4]);
        assert_eq!(cia.rating(StandardField::Integrity), Some(3));
        assert_eq!(cia.rating(StandardField::Name), None);
    }

    #[test]
    fn asset_classes_section_is_validated_when_config_loads() {
        use crate::config::{parse, Config};
        let p = std::path::Path::new("test.toml");
        let src = "[asset_classes.HW05]\nconfidentiality = 3\n\
                   [asset_classes.hw12]\nname = \"實驗儀器\"\nconfidentiality = 2\n\
                   integrity = 3\navailability = 3\n";
        let (cfg, unknown) = parse(src, p).unwrap();
        assert!(unknown.is_empty(), "{unknown:?}");
        let classes = cfg.asset_classes().unwrap();
        let hw05 = classes.get(&code("HW05")).unwrap();
        assert_eq!(hw05.template, Some(Cia::new(3, 3, 4)));
        assert!(classes.resolve("實驗儀器", false).is_some());
        for (src, needle) in [
            ("[asset_classes.PE06]\nname = \"x\"\n", "PE06"),
            (
                "[asset_classes.HW12]\nname = \"x\"\nintegrity = 3\n",
                "HW12",
            ),
        ] {
            let msg = parse(src, p).unwrap_err().to_string();
            assert!(msg.contains(needle), "{src}：{msg}");
            assert!(msg.contains("asset_classes"), "{msg}");
        }
        assert_eq!(
            Config::default().asset_classes().unwrap(),
            AssetClasses::builtin()
        );
    }

    #[test]
    fn resolve_by_code_name_and_legacy() {
        let classes = AssetClasses::builtin();
        let r = classes.resolve("資安設備", false).unwrap();
        assert_eq!((r.code.as_str(), r.legacy), ("HW03", None));
        let r = classes.resolve("hw05", false).unwrap();
        assert_eq!(r.code.as_str(), "HW05");
        let r = classes.resolve(" Security ", true).unwrap();
        assert_eq!(r.code.as_str(), "HW03");
        assert_eq!(r.legacy.as_deref(), Some("security"));
        assert_eq!(classes.resolve("security", false), None, "編修不接受舊值");
        for bad in ["IF01", "printer", "HW12", ""] {
            assert_eq!(classes.resolve(bad, true), None, "{bad}");
        }
    }

    #[test]
    fn config_overrides_only_written_fields() {
        let classes = AssetClasses::from_config(&config(&[(
            "hw05",
            AssetClassConfig {
                confidentiality: Some(3),
                ..Default::default()
            },
        )]))
        .unwrap();
        let hw05 = classes.get(&code("HW05")).unwrap();
        assert_eq!(hw05.template, Some(Cia::new(3, 3, 4)));
        assert_eq!(hw05.name, "儲存設備");
        assert_eq!(hw05.description, "NAS或Storage");
    }

    #[test]
    fn config_adds_a_class() {
        let classes = AssetClasses::from_config(&config(&[(
            "HW12",
            AssetClassConfig {
                name: Some("實驗儀器".into()),
                confidentiality: Some(2),
                integrity: Some(3),
                availability: Some(3),
                ..Default::default()
            },
        )]))
        .unwrap();
        assert_eq!(classes.all().len(), 19);
        assert_eq!(
            classes.resolve("HW12", false).unwrap().code.as_str(),
            "HW12"
        );
        assert_eq!(
            classes.resolve("實驗儀器", false).unwrap().code.as_str(),
            "HW12"
        );
        assert_eq!(
            classes.all().last().unwrap().code.as_str(),
            "SW07",
            "依代碼排序"
        );
    }

    #[test]
    fn personal_data_default_for_a_class() {
        let classes = AssetClasses::from_config(&config(&[
            (
                "HW06",
                AssetClassConfig {
                    personal_data: Some(true),
                    ..Default::default()
                },
            ),
            (
                "HW12",
                AssetClassConfig {
                    name: Some("實驗儀器".into()),
                    ..Default::default()
                },
            ),
        ]))
        .unwrap();
        assert!(classes.get(&code("HW06")).unwrap().personal_data);
        assert!(
            !classes.get(&code("HW05")).unwrap().personal_data,
            "內建為否"
        );
        assert!(
            !classes.get(&code("HW12")).unwrap().personal_data,
            "新增子類別未指定時為否"
        );
        assert!(AssetClasses::builtin()
            .all()
            .iter()
            .all(|c| !c.personal_data));
    }

    #[test]
    fn invalid_config_names_the_code() {
        let err = |entries: &[(&str, AssetClassConfig)]| {
            AssetClasses::from_config(&config(entries))
                .unwrap_err()
                .to_string()
        };
        let named = AssetClassConfig {
            name: Some("人員".into()),
            ..Default::default()
        };
        let msg = err(&[("PE06", named.clone())]);
        assert!(msg.contains("PE06") && msg.contains("HW 或 SW"), "{msg}");
        let partial = AssetClassConfig {
            name: Some("實驗儀器".into()),
            confidentiality: Some(2),
            ..Default::default()
        };
        let msg = err(&[("HW12", partial)]);
        assert!(msg.contains("HW12") && msg.contains("三項全寫"), "{msg}");
        let msg = err(&[("HW12", AssetClassConfig::default())]);
        assert!(msg.contains("HW12") && msg.contains("name"), "{msg}");
        let dup = AssetClassConfig {
            name: Some("伺服器".into()),
            ..Default::default()
        };
        let msg = err(&[("HW12", dup)]);
        assert!(msg.contains("伺服器") && msg.contains("HW12"), "{msg}");
        let out_of_range = AssetClassConfig {
            availability: Some(5),
            ..Default::default()
        };
        let msg = err(&[("HW01", out_of_range)]);
        assert!(msg.contains("HW01") && msg.contains("1–4"), "{msg}");
        let msg = err(&[
            ("hw01", named.clone()),
            ("HW01", AssetClassConfig::default()),
        ]);
        assert!(msg.contains("HW01"), "{msg}");
        let code_like = AssetClassConfig {
            name: Some("HW03".into()),
            ..Default::default()
        };
        let msg = err(&[("HW12", code_like)]);
        assert!(msg.contains("HW12") && msg.contains("代碼的形式"), "{msg}");
    }
}
