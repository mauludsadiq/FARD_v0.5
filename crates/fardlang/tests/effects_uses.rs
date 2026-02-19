use fardlang::check::check_module;
use fardlang::parse_module;

#[test]
fn forbids_undeclared_effect_use() {
    let src = br#"
module main
effect read_file(path: text): bytes
fn main(): int { read_file("x") }
"#;
    let m = parse_module(src).unwrap();
    let err = check_module(&m).unwrap_err().to_string();
    assert!(err.contains("ERROR_EFFECT_NOT_ALLOWED"));
}

#[test]
fn allows_declared_effect_use() {
    let src = br#"
module main
effect read_file(path: text): bytes
fn main(): int uses [read_file] { 1 }
"#;
    let m = parse_module(src).unwrap();
    check_module(&m).unwrap();
}
