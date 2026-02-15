use std::fs;

fn read_bytes(path: &str) -> Vec<u8> {
    fs::read(path).unwrap()
}

fn has_any(bytes: &[u8], needles: &[u8]) -> bool {
    bytes.iter().any(|b| needles.contains(b))
}

#[test]
fn g22_anka_allowlist_json_is_single_line_minified_no_trailing_newline() {
    let path = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";
    let b = read_bytes(path);

    assert!(!b.is_empty(), "g22: empty file: {}", path);

    assert!(
        !has_any(&b, b"\n\r\t"),
        "g22: file must be single-line JSON (no \\n/\\r/\\t): {}",
        path
    );

    assert!(
        !b.ends_with(b"\n"),
        "g22: file must not end with newline: {}",
        path
    );

    let s = std::str::from_utf8(&b).unwrap();
    assert!(
        !s.contains("  "),
        "g22: file must be minified (no double spaces): {}",
        path
    );
}

#[test]
fn g22_anka_allowlist_json_top_level_key_order_is_frozen() {
    let path = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";
    let b = read_bytes(path);
    let s = std::str::from_utf8(&b).unwrap();

    let i_schema = s.find("\"schema\"").unwrap();
    let i_source = s.find("\"source\"").unwrap();
    let i_modules = s.find("\"modules\"").unwrap();

    assert!(
        i_schema < i_source && i_source < i_modules,
        "g22: top-level key order must be schema, source, modules: {}",
        path
    );

    assert!(
        s.starts_with("{\"schema\":"),
        "g22: file must start with {{\"schema\": ... }}: {}",
        path
    );

    assert!(
        s.contains(",\"source\":") && s.contains(",\"modules\":"),
        "g22: required top-level keys missing or wrong separators: {}",
        path
    );
}
