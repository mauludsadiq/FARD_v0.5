use fardlang::canon::canonical_module_string;
use fardlang::parse_module;

#[test]
fn canon_is_stable_and_sorts() {
    let src = br#"
module a.b
effect z(): int
effect a(): int
fn main(): int { 1 }
"#;
    let m = parse_module(src).unwrap();
    let c = canonical_module_string(&m);
    let lines: Vec<&str> = c.lines().collect();
    assert_eq!(lines[0], "module a.b");
    // effects sorted
    assert!(c.contains("effect a()"));
    assert!(c.contains("effect z()"));
}
