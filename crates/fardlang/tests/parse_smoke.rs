use fardlang::parse_module;

#[test]
fn parses_minimal_module() {
    let src = br#"
module main
effect read_file(path: text): bytes
fn main(): int uses [read_file] { 1 }
"#;
    let m = parse_module(src).unwrap();
    assert_eq!(m.name.0, vec!["main".to_string()]);
    assert_eq!(m.effects.len(), 1);
    assert_eq!(m.fns.len(), 1);
}
