//! [`super`] 的單元測試（以 `tests/fixtures/endoflife/` 解析回應）。

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/endoflife")
        .join(name);
    std::fs::read(path).unwrap()
}

#[test]
fn parses_release_names_and_dates() {
    let p = parse_product(&fixture("panos.json")).unwrap();
    let names: Vec<&str> = p.releases.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["11.2", "11.1", "10.2"]);
    let r102 = &p.releases[2];
    assert_eq!(r102.eol_from.as_deref(), Some("2027-03-01"));
    assert!(!r102.is_eol);
    assert!(r102.latest_name.is_some());
    assert_eq!(p.html_link.as_deref(), Some("https://endoflife.date/panos"));
}

#[test]
fn optional_fields_and_null_dates() {
    let esxi = parse_product(&fixture("esxi.json")).unwrap();
    assert_eq!(
        esxi.releases[0].eoas_from, None,
        "產品沒有該階段時欄位不出現"
    );
    assert_eq!(esxi.releases[0].eoes_from, None);
    let win = parse_product(&fixture("windows-server.json")).unwrap();
    let r2022 = win.releases.iter().find(|r| r.name == "2022").unwrap();
    assert_eq!(r2022.latest_name.as_deref(), Some("10.0.20348"));
    assert!(r2022.eoas_from.is_some());
    let undated = parse_product(&fixture("no-eol-date.json")).unwrap();
    let eol = undated.releases.iter().find(|r| r.name == "8").unwrap();
    assert!(eol.is_eol);
    assert_eq!(eol.eol_from, None);
    let open = undated.releases.iter().find(|r| r.name == "9").unwrap();
    assert!(!open.is_eol);
    assert_eq!(open.eol_from, None);
}

#[test]
fn html_and_bad_dates_are_errors() {
    assert!(parse_product(&fixture("not-found.html")).is_err());
    let bad = br#"{"result":{"releases":[{"name":"1","isEol":false,"eolFrom":"2027/01/01"}]}}"#;
    let err = parse_product(bad).unwrap_err();
    assert!(err.contains("eolFrom"), "{err}");
    let missing = br#"{"result":{"releases":[{"name":"1"}]}}"#;
    assert!(parse_product(missing).is_err(), "缺必有欄位 isEol");
}
