use anyhow::{bail, Result};

use crate::ast::{Expr, Func, Module};
use crate::lex::{Lexer, Tok};

pub fn parse_module(bytes: &[u8]) -> Result<Module> {
    let mut lx = Lexer::new(bytes);
    let mut funcs = Vec::new();

    loop {
        let t = lx.next()?;
        match t {
            Tok::Eof => break,
            Tok::Fn => {
                let name = match lx.next()? {
                    Tok::Ident(s) => s,
                    _ => bail!("ERROR_PARSE expected function name after fn"),
                };
                expect(&mut lx, Tok::LParen)?;
                expect(&mut lx, Tok::RParen)?;
                expect(&mut lx, Tok::LBrace)?;
                let body = parse_expr(&mut lx)?;
                expect(&mut lx, Tok::RBrace)?;
                funcs.push(Func { name, body });
            }
            _ => bail!("ERROR_PARSE expected fn or EOF"),
        }
    }

    if funcs.is_empty() {
        bail!("ERROR_PARSE module must contain at least one fn");
    }

    Ok(Module { funcs })
}

fn parse_expr(lx: &mut Lexer<'_>) -> Result<Expr> {
    match lx.next()? {
        Tok::Unit => Ok(Expr::Unit),
        _ => bail!("ERROR_PARSE only `unit` expression supported in fardc v0 frontend"),
    }
}

fn expect(lx: &mut Lexer<'_>, want: Tok) -> Result<()> {
    let got = lx.next()?;
    if got == want {
        Ok(())
    } else {
        bail!("ERROR_PARSE expected {:?} got {:?}", want, got)
    }
}
