use fardlang::ast::{Block, Expr, FnDecl, Stmt, Type};
use fardlang::eval::{eval_block, Env};
use std::collections::BTreeMap;
use valuecore::v0::V;

fn empty_block_tail(e: Expr) -> Block {
    Block {
        stmts: vec![],
        tail: Some(Box::new(e)),
    }
}

#[test]
fn eval_let_and_add() {
    let block = Block {
        stmts: vec![
            Stmt::Let {
                name: "x".to_string(),
                expr: Expr::Int("3".to_string()),
            },
            Stmt::Let {
                name: "y".to_string(),
                expr: Expr::Int("4".to_string()),
            },
        ],
        tail: Some(Box::new(Expr::Call {
            f: "add".to_string(),
            args: vec![Expr::Ident("x".to_string()), Expr::Ident("y".to_string())],
        })),
    };

    let mut env = Env::new();
    let v = eval_block(&block, &mut env).unwrap();
    assert_eq!(v, V::Int(7));
}

#[test]
fn eval_if_selects_branch() {
    let t = empty_block_tail(Expr::Int("10".to_string()));
    let e = empty_block_tail(Expr::Int("20".to_string()));

    let block = empty_block_tail(Expr::If {
        c: Box::new(Expr::Bool(true)),
        t: Box::new(t),
        e: Box::new(e),
    });

    let mut env = Env::new();
    let v = eval_block(&block, &mut env).unwrap();
    assert_eq!(v, V::Int(10));
}

#[test]
fn eval_user_function_call() {
    // fn inc(a:int) { add(a, 1) }
    let inc = FnDecl {
        name: "inc".to_string(),
        params: vec![("a".to_string(), Type::Int)],
        ret: Some(Type::Int),
        uses: vec![],
        body: Block {
            stmts: vec![],
            tail: Some(Box::new(Expr::Call {
                f: "add".to_string(),
                args: vec![Expr::Ident("a".to_string()), Expr::Int("1".to_string())],
            })),
        },
        is_pub: false,
    };

    let mut fns = BTreeMap::new();
    fns.insert("inc".to_string(), inc);

    let main = empty_block_tail(Expr::Call {
        f: "inc".to_string(),
        args: vec![Expr::Int("41".to_string())],
    });

    let mut env = Env::with_fns(fns);
    let v = eval_block(&main, &mut env).unwrap();
    assert_eq!(v, V::Int(42));
}
