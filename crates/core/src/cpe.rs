//! CPE 2.3 formatted string 的欄位切分（NVD 條件、CSAF、關聯性篩選共用）。

/// 依未跳脫的 `:` 切開並移除跳脫字元（版本與 update 欄位的比較形式）。
pub fn fields(cpe: &str) -> Vec<String> {
    split(cpe, false)
}

/// 依未跳脫的 `:` 切開並保留跳脫字元（vendor／product 與目錄、NVD 查詢字串的形式一致）。
pub fn raw_fields(cpe: &str) -> Vec<String> {
    split(cpe, true)
}

fn split(cpe: &str, keep_escapes: bool) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = cpe.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if keep_escapes {
                    current.push(c);
                }
                current.extend(chars.next());
            }
            ':' => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_colon_does_not_split() {
        let cpe = r"cpe:2.3:a:foo:bar\:baz:1.0:*";
        assert_eq!(fields(cpe)[4], "bar:baz");
        assert_eq!(raw_fields(cpe)[4], r"bar\:baz");
        assert_eq!(fields(cpe)[5], "1.0");
    }
}
