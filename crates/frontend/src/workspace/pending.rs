//! 等待區處理：套用到資產（建立 probable 命中）或判定不相關（與 `cvewatch pending` 相同）。

use cvewatch_core::assets::ComponentKind;
use cvewatch_core::matching::pending_ops::{self, Action, Resolution};
use cvewatch_core::notify::triage_notes;

use super::reload::Refreshed;
use super::Workspace;
use crate::findings::ListView;
use crate::report::Report;

impl<V> Workspace<V> {
    /// 套用目標：有產品元件的使用中資產（等待區只含產品，套件由 OSV 判定）；
    /// 有多個產品元件時另列 `<資產>:<產品>`。
    pub fn pending_targets(&self) -> Result<Vec<String>, Report> {
        let options: Vec<String> = self
            .data
            .assets
            .iter()
            .flat_map(|a| {
                let tag = a.asset.record.asset_tag.clone();
                let products: Vec<String> = a
                    .components
                    .iter()
                    .filter_map(|c| match &c.kind {
                        ComponentKind::Product { product_key, .. } => {
                            Some(format!("{tag}:{product_key}"))
                        }
                        _ => None,
                    })
                    .collect();
                let head = (!products.is_empty()).then_some(tag);
                let extra = if products.len() > 1 {
                    products
                } else {
                    Vec::new()
                };
                head.into_iter().chain(extra)
            })
            .collect();
        if options.is_empty() {
            return Err(Report::info("沒有含產品元件的使用中資產可套用"));
        }
        Ok(options)
    }
}

impl<V: ListView> Workspace<V> {
    /// 處理等待區的 CVE 後重新載入（`open`：開啟中的命中詳情）。
    /// 與 CLI 相同：一併忽略的命中已有 Redmine 議題時，同一交易排入註記。
    pub fn resolve_pending(
        &mut self,
        cve: &str,
        action: &Action,
        note: &str,
        open: &[i64],
    ) -> Refreshed {
        let request = pending_ops::ResolveRequest {
            catalog: &self.settings.catalog,
            scoring: &self.settings.scoring,
            raw_cve: cve,
            action,
            note,
            now: self.services.now(),
        };
        let result = triage_notes::resolve_pending(&mut self.conn, &self.config.notify, &request);
        match result {
            Ok(resolution) => self.after_write(pending_message(&resolution, action), open),
            Err(e) => Refreshed::unchanged(Report::failure("等待區處理失敗", &e), open),
        }
    }
}

fn pending_message(r: &Resolution, action: &Action) -> String {
    match action {
        Action::Dismiss if r.ignored.is_empty() => {
            format!("{} 已移出等待區（人工判定不相關）", r.cve_id)
        }
        Action::Dismiss => format!(
            "{} 已移出等待區（人工判定不相關），一併忽略 {} 筆 probable 命中",
            r.cve_id,
            r.ignored.len()
        ),
        Action::Apply(target) => {
            let ids: Vec<String> = r
                .applied
                .iter()
                .map(|a| format!("#{}", a.finding_id))
                .collect();
            format!(
                "{} 已移出等待區並套用到 {target}（命中 {}）",
                r.cve_id,
                ids.join("、")
            )
        }
    }
}
