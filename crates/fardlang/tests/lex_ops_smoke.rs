use fardlang::lex::{Lexer, Tok};

#[test]
fn lex_ops_smoke() {
    let src = b"1+2*3<=4==5&&6||7-8/2%3";
    let mut lx = Lexer::new(src);

    let mut toks = Vec::new();
    loop {
        let t = lx.next().unwrap();
        toks.push(t.clone());
        if t == Tok::Eof {
            break;
        }
    }

    // We only check the operator sequence is present; ints are kept as strings.
    let ops: Vec<Tok> = toks
        .into_iter()
        .filter(|t| {
            matches!(
                t,
                Tok::Plus
                    | Tok::Minus
                    | Tok::Star
                    | Tok::Slash
                    | Tok::Percent
                    | Tok::EqEq
                    | Tok::Le
                    | Tok::Ge
                    | Tok::AndAnd
                    | Tok::OrOr
            )
        })
        .collect();

    assert_eq!(
        ops,
        vec![
            Tok::Plus,
            Tok::Star,
            Tok::Le,
            Tok::EqEq,
            Tok::AndAnd,
            Tok::OrOr,
            Tok::Minus,
            Tok::Slash,
            Tok::Percent,
        ]
    );
}
