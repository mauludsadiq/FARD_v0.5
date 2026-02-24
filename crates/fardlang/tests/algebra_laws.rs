use std::fs;

#[test]
fn algebra_laws_corpus() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus");

    let mut paths: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "fard").unwrap_or(false))
        .collect();
    paths.sort();

    for p in &paths {
        let src = fs::read_to_string(p).unwrap();
        let label = p.to_string_lossy();

        if let Err(e) = fardlang::algebra::law_ast_roundtrip(&src) {
            panic!("ast_roundtrip failed: {}\n{}", label, e);
        }
        if let Err(e) = fardlang::algebra::law_print_idempotent(&src) {
            panic!("print_idempotent failed: {}\n{}", label, e);
        }
        if let Err(e) = fardlang::algebra::law_token_roundtrip(&src) {
            panic!("token_roundtrip failed: {}\n{}", label, e);
        }
        println!("PASS: {}", label);
    }
}
