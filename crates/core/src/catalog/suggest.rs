//! 未辨識名稱的相似建議。

use std::collections::HashMap;

pub const SUGGEST_MIN_SCORE: f64 = 0.85;
pub const SUGGEST_MAX: usize = 3;

/// 每個產品取其別名中的最高分，回傳分數 ≥ 門檻的前幾個產品 key（高分在前）。
pub fn best_keys<'a>(
    name_norm: &str,
    candidates: impl Iterator<Item = (&'a str, &'a str)>,
) -> Vec<&'a str> {
    if name_norm.is_empty() {
        return Vec::new();
    }
    let mut best: HashMap<&str, f64> = HashMap::new();
    for (alias, key) in candidates {
        let score = strsim::jaro_winkler(name_norm, alias);
        if score >= SUGGEST_MIN_SCORE {
            let entry = best.entry(key).or_insert(score);
            *entry = entry.max(score);
        }
    }
    let mut ranked: Vec<(&str, f64)> = best.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ranked
        .into_iter()
        .take(SUGGEST_MAX)
        .map(|(k, _)| k)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_and_limits() {
        let candidates = vec![
            ("synologydsm", "synology-dsm"),
            ("synology", "synology-dsm"),
            ("panos", "paloalto-panos"),
        ];
        let keys = best_keys("synolgydsm", candidates.into_iter());
        assert_eq!(keys, vec!["synology-dsm"]);
    }

    #[test]
    fn empty_name_has_no_suggestions() {
        assert!(best_keys("", vec![("a", "b")].into_iter()).is_empty());
    }
}
