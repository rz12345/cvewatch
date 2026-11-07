//! `vers:` 版本區間字串（https://github.com/package-url/vers-spec）的解析與標準化。
//!
//! 只做語法層級：拆出比較子與版本，比較邏輯在 04 依目錄的 version scheme 實作。
//! 容忍 Palo Alto 在版本前加產品名稱的寫法：`vers:generic/PAN-OS>=11.1.0` → `vers:generic/>=11.1.0`。

const PREFIX: &str = "vers:";
/// 依長度排序，先比對兩字元的比較子。
const OPERATORS: [&str; 6] = [">=", "<=", "!=", ">", "<", "="];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    /// `>=`、`<=`、`!=`、`>`、`<`、`=`；`*` 代表全部版本
    pub op: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersRange {
    pub scheme: String,
    pub constraints: Vec<Constraint>,
}

impl VersRange {
    /// 標準化字串，例如 `vers:generic/>=11.1.0|<11.1.6`。
    pub fn canonical(&self) -> String {
        let parts: Vec<String> = self
            .constraints
            .iter()
            .map(|c| match c.op.as_str() {
                "*" => "*".to_string(),
                "=" => c.version.clone(),
                op => format!("{op}{}", c.version),
            })
            .collect();
        format!("{PREFIX}{}/{}", self.scheme, parts.join("|"))
    }
}

/// 解析失敗（不是 `vers:`、沒有 scheme 或版本為空）時為 None。
pub fn parse(text: &str) -> Option<VersRange> {
    let rest = text.trim().strip_prefix(PREFIX)?;
    let (scheme, constraints) = rest.split_once('/')?;
    let scheme = scheme.trim().to_lowercase();
    if scheme.is_empty() {
        return None;
    }
    let constraints = constraints
        .split('|')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(constraint)
        .collect::<Option<Vec<_>>>()?;
    (!constraints.is_empty()).then_some(VersRange {
        scheme,
        constraints,
    })
}

fn constraint(text: &str) -> Option<Constraint> {
    if text == "*" {
        return Some(Constraint {
            op: "*".into(),
            version: String::new(),
        });
    }
    // 第一個比較子之前的文字視為產品名稱前綴（Palo Alto 寫法）並捨棄
    let found = OPERATORS
        .iter()
        .filter_map(|op| text.find(op).map(|i| (i, *op)))
        .min_by_key(|(i, op)| (*i, std::cmp::Reverse(op.len())));
    let (op, version) = match found {
        Some((i, op)) => (op, &text[i + op.len()..]),
        None => ("=", text),
    };
    let version = version.trim();
    (!version.is_empty()).then(|| Constraint {
        op: op.to_string(),
        version: version.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_range() {
        let r = parse("vers:generic/>=11.1.0|<11.1.6").unwrap();
        assert_eq!(r.scheme, "generic");
        assert_eq!(r.constraints.len(), 2);
        assert_eq!(r.constraints[1].op, "<");
        assert_eq!(r.canonical(), "vers:generic/>=11.1.0|<11.1.6");
    }

    #[test]
    fn palo_alto_product_prefix_is_removed() {
        assert_eq!(
            parse("vers:generic/PAN-OS>=11.1.4-h37")
                .unwrap()
                .canonical(),
            "vers:generic/>=11.1.4-h37"
        );
        assert_eq!(
            parse("vers:generic/PAN-OS<12.2.4").unwrap().canonical(),
            "vers:generic/<12.2.4"
        );
    }

    #[test]
    fn equality_and_wildcard() {
        let r = parse("vers:semver/1.2.3|!=1.2.4|*").unwrap();
        let ops: Vec<&str> = r.constraints.iter().map(|c| c.op.as_str()).collect();
        assert_eq!(ops, vec!["=", "!=", "*"]);
        assert_eq!(r.canonical(), "vers:semver/1.2.3|!=1.2.4|*");
    }

    #[test]
    fn invalid_inputs() {
        assert!(parse(">=1.0").is_none());
        assert!(parse("vers:/>=1.0").is_none());
        assert!(parse("vers:generic/").is_none());
        assert!(parse("vers:generic/>=").is_none());
    }
}
