//! 原廠版本寫法的文法：Cisco、Juniper（含 Ivanti）、數字分段、Zyxel 韌體。
//!
//! 目錄匯入時的寫法驗證（`catalog::scheme`）與比較器（`matching::version`）共用同一套文法，
//! 以免清冊接受的版本在比對時變成無法解析。輸入一律先經 [`super::canonical`]（小寫、`_` 視為空白）。

use std::sync::LazyLock;

use regex::Regex;

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("固定的正規表示式必定可編譯")
}

/// Cisco：`主.次(rebuild[字母])train[數字]`，可帶前綴 `version`。
/// IOS `15.2(7)e8`、ASA `9.18(4)22`、NX-OS `10.2(3t)`、`12.4(25)`。
/// 群組：1 主、2 次、3 rebuild、4 rebuild 字母、5 train、6 train 數字。
pub static CISCO: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(?:version ?)?(\d+)\.(\d+)\((\d+)([a-z]*)\)([a-z]*)(\d*)$"));

/// 點分寫法：ASA CNA 記錄的 `9.18.4.22`（等同 `9.18(4)22`）、Ivanti 25.x 的 `25.1.1.3`。
/// 群組：1 主、2 次、3 第三段、4 第四段。
pub static DOTTED_FOUR: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(?:version ?)?(\d+)\.(\d+)\.(\d+)(?:\.(\d+))?$"));

/// Juniper／Ivanti：`主.次[類型 release[.小數][-服務字母 數字[.數字]]]`。
/// Junos `21.4r3-s5`、`15.1x49-d200`、`21.4`（版本線起點）、Ivanti `22.7r2.4`。
/// 群組：1 主、2 次、3 類型、4 release、5 小數、6 服務字母、7 服務數字、8 服務小數。
pub static JUNOS: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"^(\d+)\.(\d+)(?:([a-z])(\d+)(?:\.(\d+))?(?:-([a-z])(\d+)(?:\.(\d+))?)?)?$")
});

/// 數字分段：可有前綴 `v`（忽略）與一個字母前綴，數字段以 `.`、`-`、空白（含 `_`）或 `build` 分隔，
/// 結尾可有 `.x`（版本線起點，CNA 常見的 `5.1.x`）。
/// SonicOS `7.0.1-5145`、NetScaler `14.1-29.72`、QNAP `5.1.7.2770 build 20240520`、`h5.1.x`、
/// Check Point `r81.20`、Acronis `15 build 41800`。
/// 群組：1 字母前綴、2 數字段（含分隔字）。
pub static NUMERIC: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^v?([a-z])?(\d+(?:(?:[.\- ]| ?build ?)\d+)*)(?:\.[x*])?$"));

/// Zyxel 韌體：`v主.次(型號代碼.修補)c數字`，`v` 可省略。`v2.80(abcd.1)c0`。
/// 群組：1 主、2 次、3 型號代碼、4 修補、5 C 數字。
pub static ZYXEL: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^v?(\d+)\.(\d+)\(([a-z0-9]+)\.(\d+)\)c(\d+)$"));
