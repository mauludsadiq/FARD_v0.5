#[test]
fn show_full_corpus() {
    let src = include_bytes!("corpus/features.fard");
    match fardlang::parse::parse_module(src) {
        Ok(m) => {
            let canon = fardlang::canon::canonical_module_string(&m);
            println!("CANON:\n---\n{}\n---", canon);
            match fardlang::parse::parse_module(canon.as_bytes()) {
                Ok(m2) => {
                    let canon2 = fardlang::canon::canonical_module_string(&m2);
                    if canon == canon2 {
                        println!("RE-PARSE + REPRINT: IDENTICAL");
                    } else {
                        println!("RE-PARSE OK but canon differs:\n---\n{}\n---", canon2);
                    }
                }
                Err(e) => println!("RE-PARSE ERROR: {}", e),
            }
        }
        Err(e) => println!("INITIAL PARSE ERROR: {}", e),
    }
}
