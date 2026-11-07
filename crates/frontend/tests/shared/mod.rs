//! TUI 與 frontend 測試共用的畫面資料片段：PAN-OS 命中與僅關鍵字相符的等待區 CVE。
//! 以 `#[path]` 引入，並要求同一層有 `common`（`crates/core/tests/common`）模組。

use rusqlite::params;
use serde_json::json;

use super::common::matching::{affected, panos, World};

pub const PANOS_CVES: [&str; 3] = ["CVE-2026-0001", "CVE-2026-0002", "CVE-2026-0003"];
pub const PENDING_CVE: &str = "CVE-2099-20001";

/// 種子目錄沒有 Proxmox 的 CNA 寫法，以 catalog.d 補上。
pub const PROXMOX_CNA: &str = r#"
[[product]]
key = "proxmox-ve"
cna = [{ vendor = "Proxmox Server Solutions GmbH", product = "Proxmox VE" }]
"#;

/// 三筆影響 PAN-OS 11.1.0 起的 CVE（修正版本依序為 11.1.3、11.1.4、11.1.5）；
/// `cvss` 為真時依序寫入 9.8、8.8、7.8。
pub fn panos_cves(w: &World, cvss: bool) {
    for (i, cve) in PANOS_CVES.iter().enumerate() {
        let upper = format!("11.1.{}", 3 + i);
        w.cve(
            cve,
            panos(json!([{"version": "11.1.0", "lessThan": upper, "status": "affected"}])),
            json!([]),
        );
        if cvss {
            w.cvss(cve, 9.8 - i as f64);
        }
    }
}

/// [`PENDING_CVE`]：只有關鍵字相符（沒有 affected 版本）的 Proxmox VE CVE，
/// 比對後進等待區、不自動建立 probable 命中。需要 [`PROXMOX_CNA`]。
pub fn keyword_only_pending(w: &World) {
    w.cve(
        PENDING_CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([])
        )]),
        json!([]),
    );
    w.conn
        .execute(
            "DELETE FROM cve_affected WHERE cve_id = ?1",
            params![PENDING_CVE],
        )
        .expect("affected");
    w.conn
        .execute(
            "UPDATE cve SET relevance_reason = 'keyword_only',
                            description_en = 'Proxmox VE privilege escalation' WHERE id = ?1",
            params![PENDING_CVE],
        )
        .expect("desc");
}
