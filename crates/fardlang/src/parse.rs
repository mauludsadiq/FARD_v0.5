use crate::ast::*;

use crate::lex::{Lexer, Tok};
use anyhow::{bail, Result};

pub fn parse_module(bytes: &[u8]) -> Result<Module> {
    let mut lx = Lexer::new(bytes);

    expect(&mut lx, Tok::KwModule)?;
    let name = parse_modpath(&mut lx)?;

    let mut imports = Vec::new();
    let mut fact_imports = Vec::new();
    let mut effects = Vec::new();
    let mut types = Vec::new();
    let mut fns = Vec::new();

    loop {
        let t = lx.next()?;
        match t {
            Tok::Eof => break,
            Tok::KwImport => {
                // either "import path.to.mod as alias" OR "import x: Run("sha256:...")"
                let t = lx.next()?;
                match t {
                    Tok::Ident(n) => {
                        let nxt = lx.next()?;
                        match nxt {
                            Tok::Colon => {
                                expect(&mut lx, Tok::KwRun)?;
                                expect(&mut lx, Tok::LParen)?;
                                let rid = match lx.next()? {
                                    Tok::Text(s) => s,
                                    _ => bail!("ERROR_PARSE expected Run(\"...\")"),
                                };
                                expect(&mut lx, Tok::RParen)?;
                                fact_imports.push(FactImportDecl {
                                    name: n,
                                    run_id: rid,
                                });
                            }
                            Tok::Dot => {
                                // parse rest of modpath
                                let mut parts = vec![n];
                                parts.push(parse_ident_after_dot(&mut lx)?);
                                while peek_is(&mut lx, Tok::Dot)? {
                                    lx.next()?;
                                    parts.push(parse_ident(&mut lx)?);
                                }
                                let path = ModPath(parts);
                                let mut alias = None;
                                if peek_is(&mut lx, Tok::KwAs)? {
                                    lx.next()?;
                                    alias = Some(parse_ident(&mut lx)?);
                                }
                                imports.push(ImportDecl { path, alias });
                            }
                            Tok::KwAs => {
                                let alias = Some(parse_ident(&mut lx)?);
                                imports.push(ImportDecl {
                                    path: ModPath(vec![n]),
                                    alias,
                                });
                            }
                            _ => {
                                // single ident modpath
                                let path = ModPath(vec![n]);
                                // push back isn't supported; treat as no-alias import
                                imports.push(ImportDecl { path, alias: None });
                                // we consumed one extra token; reject for now
                                bail!("ERROR_PARSE malformed import");
                            }
                        }
                    }
                    _ => bail!("ERROR_PARSE expected ident after import"),
                }
            }
            Tok::KwEffect => effects.push(parse_effect(&mut lx)?),
            Tok::KwPub | Tok::KwType | Tok::KwFn => {
                // allow pub prefix
                let mut is_pub = false;
                let head = if t == Tok::KwPub {
                    is_pub = true;
                    lx.next()?
                } else {
                    t
                };
                match head {
                    Tok::KwType => types.push(parse_type_decl(&mut lx, is_pub)?),
                    Tok::KwFn => fns.push(parse_fn_decl(&mut lx, is_pub)?),
                    _ => bail!("ERROR_PARSE expected type or fn after pub"),
                }
            }
            _ => bail!("ERROR_PARSE unexpected token at top-level"),
        }
    }

    Ok(Module {
        name,
        imports,
        fact_imports,
        effects,
        types,
        fns,
    })
}

fn parse_modpath(lx: &mut Lexer<'_>) -> Result<ModPath> {
    let mut parts = vec![parse_ident(lx)?];
    while peek_is(lx, Tok::Dot)? {
        lx.next()?;
        parts.push(parse_ident(lx)?);
    }
    Ok(ModPath(parts))
}

fn parse_ident_after_dot(lx: &mut Lexer<'_>) -> Result<String> {
    // after having consumed a Dot token already
    parse_ident(lx)
}

fn parse_ident(lx: &mut Lexer<'_>) -> Result<String> {
    match lx.next()? {
        Tok::Ident(s) => Ok(s),
        _ => bail!("ERROR_PARSE expected ident"),
    }
}

fn parse_effect(lx: &mut Lexer<'_>) -> Result<EffectDecl> {
    let name = parse_ident(lx)?;
    expect(lx, Tok::LParen)?;
    let mut params = Vec::new();
    if !peek_is(lx, Tok::RParen)? {
        loop {
            let p = parse_ident(lx)?;
            expect(lx, Tok::Colon)?;
            let t = parse_type(lx)?;
            params.push((p, t));
            if peek_is(lx, Tok::Comma)? {
                lx.next()?;
                continue;
            }
            break;
        }
    }
    expect(lx, Tok::RParen)?;
    expect(lx, Tok::Colon)?;
    let ret = parse_type(lx)?;
    Ok(EffectDecl { name, params, ret })
}

fn parse_type_decl(lx: &mut Lexer<'_>, is_pub: bool) -> Result<TypeDecl> {
    let name = parse_ident(lx)?;
    let params = parse_type_params(lx)?;
    expect(lx, Tok::Eq)?;

    // record type: { a: int, b: text }
    // sum type:   | None | Some(value: T)
    let body = if peek_is(lx, Tok::LBrace)? {
        lx.next()?;
        let mut fields = Vec::new();
        if !peek_is(lx, Tok::RBrace)? {
            loop {
                let f = parse_ident(lx)?;
                expect(lx, Tok::Colon)?;
                let t = parse_type(lx)?;
                fields.push((f, t));
                if peek_is(lx, Tok::Comma)? {
                    lx.next()?;
                    continue;
                }
                break;
            }
        }
        expect(lx, Tok::RBrace)?;
        TypeBody::Record(fields)
    } else {
        let mut vars = Vec::new();
        loop {
            if peek_is(lx, Tok::Pipe)? {
                lx.next()?;
            }
            let vname = parse_ident(lx)?;
            let mut fields = Vec::new();
            if peek_is(lx, Tok::LParen)? {
                lx.next()?;
                if !peek_is(lx, Tok::RParen)? {
                    loop {
                        let f = parse_ident(lx)?;
                        expect(lx, Tok::Colon)?;
                        let t = parse_type(lx)?;
                        fields.push((f, t));
                        if peek_is(lx, Tok::Comma)? {
                            lx.next()?;
                            continue;
                        }
                        break;
                    }
                }
                expect(lx, Tok::RParen)?;
            }
            vars.push(Variant {
                name: vname,
                fields,
            });
            if peek_is(lx, Tok::Pipe)? {
                continue;
            }
            break;
        }
        TypeBody::Sum(vars)
    };

    Ok(TypeDecl {
        name,
        params,
        body,
        is_pub,
    })
}

fn parse_fn_decl(lx: &mut Lexer<'_>, is_pub: bool) -> Result<FnDecl> {
    let name = parse_ident(lx)?;
    let _tparams = parse_type_params(lx)?; // accepted, ignored here
    expect(lx, Tok::LParen)?;
    let mut params = Vec::new();
    if !peek_is(lx, Tok::RParen)? {
        loop {
            let p = parse_ident(lx)?;
            expect(lx, Tok::Colon)?;
            let t = parse_type(lx)?;
            params.push((p, t));
            if peek_is(lx, Tok::Comma)? {
                lx.next()?;
                continue;
            }
            break;
        }
    }
    expect(lx, Tok::RParen)?;

    let mut ret = None;
    if peek_is(lx, Tok::Colon)? {
        lx.next()?;
        ret = Some(parse_type(lx)?);
    }

    let mut uses = Vec::new();
    if peek_is(lx, Tok::KwUses)? {
        lx.next()?;
        expect(lx, Tok::LBrack)?;
        if !peek_is(lx, Tok::RBrack)? {
            loop {
                uses.push(parse_ident(lx)?);
                if peek_is(lx, Tok::Comma)? {
                    lx.next()?;
                    continue;
                }
                break;
            }
        }
        expect(lx, Tok::RBrack)?;
    }

    let body = parse_block(lx)?;
    Ok(FnDecl {
        name,
        params,
        ret,
        uses,
        body,
        is_pub,
    })
}

fn parse_block(lx: &mut Lexer<'_>) -> Result<Block> {
    expect(lx, Tok::LBrace)?;
    let mut stmts = Vec::new();
    let mut tail = None;

    loop {
        if peek_is(lx, Tok::RBrace)? {
            lx.next()?;
            break;
        }
        if peek_is(lx, Tok::KwLet)? {
            lx.next()?;
            let name = parse_ident(lx)?;
            expect(lx, Tok::Eq)?;
            let expr = parse_expr(lx)?;
            stmts.push(Stmt::Let { name, expr });
            continue;
        }

        // either stmt expr or tail expr; we treat last expr before '}' as tail if next is '}'
        let e = parse_expr(lx)?;
        if peek_is(lx, Tok::RBrace)? {
            tail = Some(Box::new(e));
            lx.next()?;
            break;
        } else {
            stmts.push(Stmt::Expr(e));
        }
    }

    Ok(Block { stmts, tail })
}
fn parse_expr(lx: &mut Lexer<'_>) -> Result<Expr> {
    parse_or(lx)
}

fn parse_or(lx: &mut Lexer<'_>) -> Result<Expr> {
    let mut lhs = parse_and(lx)?;
    while peek_is(lx, Tok::OrOr)? {
        expect(lx, Tok::OrOr)?;
        let rhs = parse_and(lx)?;
        lhs = Expr::BinOp {
            op: BinOp::Or,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_and(lx: &mut Lexer<'_>) -> Result<Expr> {
    let mut lhs = parse_cmp(lx)?;
    while peek_is(lx, Tok::AndAnd)? {
        expect(lx, Tok::AndAnd)?;
        let rhs = parse_cmp(lx)?;
        lhs = Expr::BinOp {
            op: BinOp::And,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_cmp(lx: &mut Lexer<'_>) -> Result<Expr> {
    let mut lhs = parse_add(lx)?;
    loop {
        let op = if peek_is(lx, Tok::EqEq)? {
            Some(BinOp::Eq)
        } else if peek_is(lx, Tok::Le)? {
            Some(BinOp::Le)
        } else if peek_is(lx, Tok::Ge)? {
            Some(BinOp::Ge)
        } else if peek_is(lx, Tok::Lt)? {
            Some(BinOp::Lt)
        } else if peek_is(lx, Tok::Gt)? {
            Some(BinOp::Gt)
        } else {
            None
        };

        let Some(op) = op else {
            break;
        };

        match op {
            BinOp::Eq => expect(lx, Tok::EqEq)?,
            BinOp::Le => expect(lx, Tok::Le)?,
            BinOp::Ge => expect(lx, Tok::Ge)?,
            BinOp::Lt => expect(lx, Tok::Lt)?,
            BinOp::Gt => expect(lx, Tok::Gt)?,
            _ => unreachable!(),
        }

        let rhs = parse_add(lx)?;
        lhs = Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_add(lx: &mut Lexer<'_>) -> Result<Expr> {
    let mut lhs = parse_mul(lx)?;
    loop {
        let op = if peek_is(lx, Tok::PlusPlus)? {
            Some(BinOp::Concat)
        } else if peek_is(lx, Tok::Plus)? {
            Some(BinOp::Add)
        } else if peek_is(lx, Tok::Minus)? {
            Some(BinOp::Sub)
        } else {
            None
        };

        let Some(op) = op else {
            break;
        };

        match op {
            BinOp::Add => expect(lx, Tok::Plus)?,
            BinOp::Concat => expect(lx, Tok::PlusPlus)?,
            BinOp::Sub => expect(lx, Tok::Minus)?,
            _ => unreachable!(),
        }

        let rhs = parse_mul(lx)?;
        lhs = Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_mul(lx: &mut Lexer<'_>) -> Result<Expr> {
    let mut lhs = parse_unary(lx)?;
    loop {
        let op = if peek_is(lx, Tok::Star)? {
            Some(BinOp::Mul)
        } else if peek_is(lx, Tok::Slash)? {
            Some(BinOp::Div)
        } else if peek_is(lx, Tok::Percent)? {
            Some(BinOp::Rem)
        } else {
            None
        };

        let Some(op) = op else {
            break;
        };

        match op {
            BinOp::Mul => expect(lx, Tok::Star)?,
            BinOp::Div => expect(lx, Tok::Slash)?,
            BinOp::Rem => expect(lx, Tok::Percent)?,
            _ => unreachable!(),
        }

        let rhs = parse_unary(lx)?;
        lhs = Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_unary(lx: &mut Lexer<'_>) -> Result<Expr> {
    if peek_is(lx, Tok::Minus)? {
        expect(lx, Tok::Minus)?;
        let e = parse_unary(lx)?;
        return Ok(Expr::UnaryMinus(Box::new(e)));
    }
    parse_primary(lx)
}

fn parse_primary(lx: &mut Lexer<'_>) -> Result<Expr> {
    // minimal expression set for v1 bootstrap: literals, ident, call, if, list, record, field-get
    if peek_is(lx, Tok::KwIf)? {
        lx.next()?;
        let c = Box::new(parse_expr(lx)?);
        let t = parse_block(lx)?;
        expect(lx, Tok::KwElse)?;
        let e = parse_block(lx)?;
        return Ok(Expr::If {
            c,
            t: Box::new(t),
            e: Box::new(e),
        });
    }

    if peek_is(lx, Tok::KwMatch)? {
        lx.next()?;
        return parse_match_expr(lx);
    }

    let e = match lx.next()? {
        Tok::KwUnit => Expr::Unit,
        Tok::KwTrue => Expr::Bool(true),
        Tok::KwFalse => Expr::Bool(false),
        Tok::Int(s) => Expr::Int(s),
        Tok::Text(s) => Expr::Text(s),
        Tok::BytesHex(h) => Expr::BytesHex(h),

        Tok::LParen => {
            let e = parse_expr(lx)?;
            expect(lx, Tok::RParen)?;
            e
        }

        Tok::LBrack => {
            let mut items = Vec::new();

            // []
            if peek_is(lx, Tok::RBrack)? {
                lx.next()?; // consume ']'
                return Ok(Expr::List(items));
            }

            // [a, b, c] with optional trailing comma
            loop {
                items.push(parse_expr(lx)?);

                if peek_is(lx, Tok::Comma)? {
                    lx.next()?; // consume ','

                    // allow trailing comma: [a,]
                    if peek_is(lx, Tok::RBrack)? {
                        break;
                    }
                    continue;
                }

                break;
            }

            expect(lx, Tok::RBrack)?;
            Expr::List(items)
        }

        Tok::LBrace => {
            let mut fields: Vec<(String, Expr)> = vec![];

            // {}
            if peek_is(lx, Tok::RBrace)? {
                lx.next()?; // consume '}'
                Expr::RecordLit(fields)
            } else {
                // {a: 1, b: 2,} trailing comma allowed
                loop {
                    let k = parse_ident(lx)?;
                    expect(lx, Tok::Colon)?;
                    let v = parse_expr(lx)?;
                    fields.push((k, v));

                    if peek_is(lx, Tok::Comma)? {
                        lx.next()?; // consume ','
                        if peek_is(lx, Tok::RBrace)? {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                expect(lx, Tok::RBrace)?;
                Expr::RecordLit(fields)
            }
        }

        Tok::Ident(id) => {
            if peek_is(lx, Tok::LParen)? {
                lx.next()?;
                let mut args = Vec::new();
                if !peek_is(lx, Tok::RParen)? {
                    loop {
                        args.push(parse_expr(lx)?);
                        if peek_is(lx, Tok::Comma)? {
                            lx.next()?;
                            continue;
                        }
                        break;
                    }
                }
                expect(lx, Tok::RParen)?;
                Expr::Call { f: id, args }
            } else {
                Expr::Ident(id)
            }
        }

        tok => bail!("ERROR_PARSE unsupported expression {:?}", tok),
    };

    let mut out = e;
    while peek_is(lx, Tok::Dot)? {
        lx.next()?; // consume '.'
        let f = parse_ident(lx)?;
        out = Expr::FieldGet {
            base: Box::new(out),
            field: f,
        };
    }
    Ok(out)
}

fn parse_match_expr(lx: &mut Lexer<'_>) -> Result<Expr> {
    let scrut = parse_expr(lx)?;
    expect(lx, Tok::LBrace)?;

    let mut arms: Vec<MatchArm> = vec![];
    if !peek_is(lx, Tok::RBrace)? {
        loop {
            let pat = parse_pattern(lx)?;
            expect(lx, Tok::FatArrow)?;
            let body = parse_expr(lx)?;
            arms.push(MatchArm { pat, body });

            if peek_is(lx, Tok::Comma)? {
                lx.next()?;
                if peek_is(lx, Tok::RBrace)? {
                    break;
                }
                continue;
            }
            break;
        }
    }

    expect(lx, Tok::RBrace)?;
    Ok(Expr::Match {
        scrut: Box::new(scrut),
        arms,
    })
}

fn parse_pattern(lx: &mut Lexer<'_>) -> Result<Pattern> {
    let t = lx.next()?;
    Ok(match t {
        Tok::Ident(s) if s == "_" => Pattern::Wild,
        Tok::KwUnit => Pattern::Unit,
        Tok::KwTrue => Pattern::Bool(true),
        Tok::KwFalse => Pattern::Bool(false),
        Tok::Int(z) => Pattern::Int(z),
        Tok::Text(s) => Pattern::Text(s),
        Tok::BytesHex(h) => Pattern::BytesHex(h),
        Tok::Ident(s) => Pattern::Ident(s),
        _ => bail!("ERROR_PARSE expected pattern"),
    })
}

fn parse_type_params(lx: &mut Lexer<'_>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if !peek_is(lx, Tok::Lt)? {
        return Ok(out);
    }
    lx.next()?;
    loop {
        out.push(parse_ident(lx)?);
        if peek_is(lx, Tok::Comma)? {
            lx.next()?;
            continue;
        }
        break;
    }
    expect(lx, Tok::Gt)?;
    Ok(out)
}

fn parse_type(lx: &mut Lexer<'_>) -> Result<Type> {
    match lx.next()? {
        Tok::Ident(s) => Ok(match s.as_str() {
            "unit" => Type::Unit,
            "bool" => Type::Bool,
            "int" => Type::Int,
            "bytes" => Type::Bytes,
            "text" => Type::Text,
            "Value" => Type::Value,
            "List" => {
                expect(lx, Tok::Lt)?;
                let t = parse_type(lx)?;
                expect(lx, Tok::Gt)?;
                Type::List(Box::new(t))
            }
            "Map" => {
                expect(lx, Tok::Lt)?;
                let k = parse_type(lx)?;
                expect(lx, Tok::Comma)?;
                let v = parse_type(lx)?;
                expect(lx, Tok::Gt)?;
                Type::Map(Box::new(k), Box::new(v))
            }
            _ => {
                let args = if peek_is(lx, Tok::Lt)? {
                    lx.next()?;
                    let mut a = Vec::new();
                    loop {
                        a.push(parse_type(lx)?);
                        if peek_is(lx, Tok::Comma)? {
                            lx.next()?;
                            continue;
                        }
                        break;
                    }
                    expect(lx, Tok::Gt)?;
                    a
                } else {
                    Vec::new()
                };
                Type::Named { name: s, args }
            }
        }),
        _ => bail!("ERROR_PARSE expected type"),
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
fn peek_is(lx: &mut Lexer<'_>, want: Tok) -> Result<bool> {
    let m = lx.mark();
    let t = lx.next()?;
    lx.reset(m);
    Ok(t == want)
}
