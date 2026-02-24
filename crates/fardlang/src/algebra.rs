use crate::ast::Module;
use crate::lex::{Lexer, Tok};
use crate::canon::{canonical_module_string};
use crate::parse::parse_module;

/// Normalized token for algebra laws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokN {
    pub k: TokKind,
    pub lex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokKind {
    Kw, Ident, IntLit, TextLit, BytesLit, Sym, Eof,
}

fn tok_to_tokn(t: Tok) -> TokN {
    match t {
        Tok::KwModule   => TokN { k: TokKind::Kw,  lex: "module".into() },
        Tok::KwImport   => TokN { k: TokKind::Kw,  lex: "import".into() },
        Tok::KwAs       => TokN { k: TokKind::Kw,  lex: "as".into() },
        Tok::KwPub      => TokN { k: TokKind::Kw,  lex: "pub".into() },
        Tok::KwType     => TokN { k: TokKind::Kw,  lex: "type".into() },
        Tok::KwEffect   => TokN { k: TokKind::Kw,  lex: "effect".into() },
        Tok::KwFn       => TokN { k: TokKind::Kw,  lex: "fn".into() },
        Tok::KwArtifact => TokN { k: TokKind::Kw,  lex: "artifact".into() },
        Tok::KwUses     => TokN { k: TokKind::Kw,  lex: "uses".into() },
        Tok::KwRun      => TokN { k: TokKind::Kw,  lex: "run".into() },
        Tok::KwLet      => TokN { k: TokKind::Kw,  lex: "let".into() },
        Tok::KwIf       => TokN { k: TokKind::Kw,  lex: "if".into() },
        Tok::KwElse     => TokN { k: TokKind::Kw,  lex: "else".into() },
        Tok::KwMatch    => TokN { k: TokKind::Kw,  lex: "match".into() },
        Tok::KwTrue     => TokN { k: TokKind::Kw,  lex: "true".into() },
        Tok::KwFalse    => TokN { k: TokKind::Kw,  lex: "false".into() },
        Tok::KwUnit     => TokN { k: TokKind::Kw,  lex: "unit".into() },
        Tok::Ident(s)   => TokN { k: TokKind::Ident,   lex: s },
        Tok::Text(s)    => TokN { k: TokKind::TextLit,  lex: format!("\"{}\"", s) },
        Tok::BytesHex(s)=> TokN { k: TokKind::BytesLit, lex: format!("b\"{}\"", s) },
        Tok::Int(s)     => TokN { k: TokKind::IntLit,   lex: s },
        Tok::LParen     => TokN { k: TokKind::Sym, lex: "(".into() },
        Tok::RParen     => TokN { k: TokKind::Sym, lex: ")".into() },
        Tok::LBrace     => TokN { k: TokKind::Sym, lex: "{".into() },
        Tok::RBrace     => TokN { k: TokKind::Sym, lex: "}".into() },
        Tok::LBrack     => TokN { k: TokKind::Sym, lex: "[".into() },
        Tok::RBrack     => TokN { k: TokKind::Sym, lex: "]".into() },
        Tok::Lt         => TokN { k: TokKind::Sym, lex: "<".into() },
        Tok::Gt         => TokN { k: TokKind::Sym, lex: ">".into() },
        Tok::Colon      => TokN { k: TokKind::Sym, lex: ":".into() },
        Tok::Comma      => TokN { k: TokKind::Sym, lex: ",".into() },
        Tok::Dot        => TokN { k: TokKind::Sym, lex: ".".into() },
        Tok::Eq         => TokN { k: TokKind::Sym, lex: "=".into() },
        Tok::FatArrow   => TokN { k: TokKind::Sym, lex: "=>".into() },
        Tok::Pipe       => TokN { k: TokKind::Sym, lex: "|".into() },
        Tok::PipeGreater=> TokN { k: TokKind::Sym, lex: "|>".into() },
        Tok::Question   => TokN { k: TokKind::Sym, lex: "?".into() },
        Tok::Plus       => TokN { k: TokKind::Sym, lex: "+".into() },
        Tok::Minus      => TokN { k: TokKind::Sym, lex: "-".into() },
        Tok::Star       => TokN { k: TokKind::Sym, lex: "*".into() },
        Tok::Slash      => TokN { k: TokKind::Sym, lex: "/".into() },
        Tok::Percent    => TokN { k: TokKind::Sym, lex: "%".into() },
        Tok::EqEq       => TokN { k: TokKind::Sym, lex: "==".into() },
        Tok::Le         => TokN { k: TokKind::Sym, lex: "<=".into() },
        Tok::Ge         => TokN { k: TokKind::Sym, lex: ">=".into() },
        Tok::AndAnd     => TokN { k: TokKind::Sym, lex: "&&".into() },
        Tok::OrOr       => TokN { k: TokKind::Sym, lex: "||".into() },
        Tok::PlusPlus   => TokN { k: TokKind::Sym, lex: "++".into() },
        Tok::Eof        => TokN { k: TokKind::Eof, lex: "".into() },
    }
}

pub fn tokenize_shipped(src: &str) -> anyhow::Result<Vec<TokN>> {
    let mut lex = Lexer::new(src.as_bytes());
    let mut out = Vec::new();
    loop {
        let t = lex.next()?;
        let is_eof = t == Tok::Eof;
        out.push(tok_to_tokn(t));
        if is_eof { break; }
    }
    Ok(out)
}

pub fn canon_tokens(ts: Vec<TokN>) -> Vec<TokN> {
    ts.into_iter().map(|mut t| {
        if t.k == TokKind::Sym { t.lex = t.lex.trim().to_string(); }
        t
    }).collect()
}

pub fn parse_shipped(src: &str) -> anyhow::Result<Module> {
    parse_module(src.as_bytes()).map_err(|e| anyhow::anyhow!("{}", e))
}

pub fn print_canon(m: &Module) -> String {
    canonical_module_string(m)
}

pub fn canon_ast(m: Module) -> Module { m }

pub fn law_token_roundtrip(src: &str) -> anyhow::Result<()> {
    let t1 = canon_tokens(tokenize_shipped(src)?);
    let src2 = detokenize_canon(&t1);
    let t2 = canon_tokens(tokenize_shipped(&src2)?);
    if t1 != t2 {
        anyhow::bail!("token roundtrip failed\nSRC2:\n{}\nT1:{:?}\nT2:{:?}", src2, t1, t2);
    }
    Ok(())
}

pub fn law_ast_roundtrip(src: &str) -> anyhow::Result<()> {
    let p1 = canon_ast(parse_shipped(src)?);
    let s1 = print_canon(&p1);
    let p2 = canon_ast(parse_shipped(&s1)?);
    let s2 = print_canon(&p2);
    if s1 != s2 {
        anyhow::bail!("ast roundtrip failed\nS1:\n{}\nS2:\n{}", s1, s2);
    }
    Ok(())
}

pub fn law_print_idempotent(src: &str) -> anyhow::Result<()> {
    let p = canon_ast(parse_shipped(src)?);
    let s1 = print_canon(&p);
    let p2 = canon_ast(parse_shipped(&s1)?);
    let s2 = print_canon(&p2);
    if s1 != s2 {
        anyhow::bail!("print idempotence failed\nS1:\n{}\nS2:\n{}", s1, s2);
    }
    Ok(())
}

pub fn detokenize_canon(ts: &[TokN]) -> String {
    let mut out = String::new();
    let tight = |s: &str| matches!(s, "." | "(" | ")" | "[" | "]" | "{" | "}" | "," | ":");
    let binop = |s: &str| matches!(s, "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" |
        "+" | "-" | "*" | "/" | "%" | "&&" | "||" | "|>" | "=>" | "|");
    let is_word = |k: &TokKind| matches!(k,
        TokKind::Kw | TokKind::Ident | TokKind::IntLit |
        TokKind::TextLit | TokKind::BytesLit);

    let mut prev: Option<&TokN> = None;
    for t in ts.iter() {
        if t.k == TokKind::Eof { break; }
        let space = match prev {
            None => false,
            Some(p) => {
                let p_tight = p.k == TokKind::Sym && tight(&p.lex);
                let t_tight = t.k == TokKind::Sym && tight(&t.lex);
                if p_tight || t_tight { false }
                else if is_word(&p.k) && is_word(&t.k) { true }
                else if p.k == TokKind::Sym && binop(&p.lex) && is_word(&t.k) { true }
                else if is_word(&p.k) && t.k == TokKind::Sym && binop(&t.lex) { true }
                else { false }
            }
        };
        if space { out.push(' '); }
        out.push_str(&t.lex);
        prev = Some(t);
    }
    out.push('\n');
    out
}
