use crate::ast::*;

pub fn canonical_module_bytes(m: &Module) -> Vec<u8> {
    canonical_module_string(m).into_bytes()
}

pub fn canonical_module_string(m: &Module) -> String {
    let mut out = String::new();

    out.push_str("module ");
    out.push_str(&m.name.0.join("."));
    out.push('\n');

    // sort deterministically
    let mut imports = m.imports.clone();
    imports
        .sort_by(|a, b| (a.path.clone(), a.alias.clone()).cmp(&(b.path.clone(), b.alias.clone())));

    let mut fact_imports = m.fact_imports.clone();
    fact_imports.sort_by(|a, b| a.name.cmp(&b.name));

    let mut effects = m.effects.clone();
    effects.sort_by(|a, b| a.name.cmp(&b.name));

    let mut types = m.types.clone();
    types.sort_by(|a, b| a.name.cmp(&b.name));

    let mut fns = m.fns.clone();
    fns.sort_by(|a, b| a.name.cmp(&b.name));

    for i in imports {
        out.push_str("import ");
        out.push_str(&i.path.0.join("."));
        if let Some(a) = i.alias {
            out.push_str(" as ");
            out.push_str(&a);
        }
        out.push('\n');
    }

    for fi in fact_imports {
        out.push_str("import ");
        out.push_str(&fi.name);
        out.push_str(": Run(\"");
        out.push_str(&fi.run_id);
        out.push_str("\")\n");
    }

    for e in effects {
        out.push_str("effect ");
        out.push_str(&e.name);
        out.push('(');
        for (idx, (p, t)) in e.params.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(p);
            out.push_str(": ");
            out.push_str(&print_type(t));
        }
        out.push_str("): ");
        out.push_str(&print_type(&e.ret));
        out.push('\n');
    }

    for t in types {
        if t.is_pub {
            out.push_str("pub ");
        }
        out.push_str("type ");
        out.push_str(&t.name);
        if !t.params.is_empty() {
            out.push('<');
            for (i, p) in t.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(p);
            }
            out.push('>');
        }
        out.push_str(" = ");
        match &t.body {
            TypeBody::Record(fields) => {
                out.push_str("{ ");
                for (i, (k, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(k);
                    out.push_str(": ");
                    out.push_str(&print_type(ty));
                }
                out.push_str(" }");
            }
            TypeBody::Sum(vars) => {
                for (i, v) in vars.iter().enumerate() {
                    if i == 0 {
                        out.push_str("| ");
                    } else {
                        out.push_str(" | ");
                    }
                    out.push_str(&v.name);
                    if !v.fields.is_empty() {
                        out.push('(');
                        for (j, (k, ty)) in v.fields.iter().enumerate() {
                            if j > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(k);
                            out.push_str(": ");
                            out.push_str(&print_type(ty));
                        }
                        out.push(')');
                    }
                }
            }
        }
        out.push('\n');
    }

    for f in fns {
        if f.is_pub {
            out.push_str("pub ");
        }
        out.push_str("fn ");
        out.push_str(&f.name);
        out.push('(');
        for (i, (p, ty)) in f.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(p);
            out.push_str(": ");
            out.push_str(&print_type(ty));
        }
        out.push(')');
        if let Some(r) = &f.ret {
            out.push_str(": ");
            out.push_str(&print_type(r));
        }
        if !f.uses.is_empty() {
            out.push_str(" uses [");
            for (i, u) in f.uses.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(u);
            }
            out.push(']');
        }
        out.push_str(" { ");
        out.push_str(&print_block(&f.body));
        out.push_str(" }\n");
    }

    out
}

fn print_type(t: &Type) -> String {
    match t {
        Type::Unit => "unit".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Bytes => "bytes".to_string(),
        Type::Text => "text".to_string(),
        Type::Value => "Value".to_string(),
        Type::List(x) => format!("List<{}>", print_type(x)),
        Type::Map(k, v) => format!("Map<{}, {}>", print_type(k), print_type(v)),
        Type::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let mut s = String::new();
                s.push_str(name);
                s.push('<');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&print_type(a));
                }
                s.push('>');
                s
            }
        }
    }
}

fn print_block(b: &Block) -> String {
    let mut s = String::new();
    for st in &b.stmts {
        match st {
            Stmt::Let { name, expr } => {
                s.push_str("let ");
                s.push_str(name);
                s.push_str(" = ");
                s.push_str(&print_expr(expr));
                s.push_str("; ");
            }
            Stmt::Expr(e) => {
                s.push_str(&print_expr(e));
                s.push_str("; ");
            }
        }
    }
    if let Some(t) = &b.tail {
        s.push_str(&print_expr(t));
    } else {
        s.push_str("unit");
    }
    s
}

pub fn print_expr_public(e: &Expr) -> String {
    print_expr(e)
}
// canon_binop_print_v1 begin
fn binop_str(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Concat => "++",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}
// canon_binop_print_v1 end

fn print_pattern(p: &Pattern) -> String {
    match p {
        Pattern::Wild => "_".to_string(),
        Pattern::Unit => "unit".to_string(),
        Pattern::Bool(true) => "true".to_string(),
        Pattern::Bool(false) => "false".to_string(),
        Pattern::Int(z) => z.clone(),
        Pattern::Text(s) => format!("{:?}", s),
        Pattern::BytesHex(h) => format!("b{:?}", h),
        Pattern::Ident(s) => s.clone(),
        Pattern::List(pats) => {
            let inner = pats.iter().map(|p| print_pattern(p)).collect::<Vec<_>>().join(", ");
            format!("[{}]", inner)
        }
    }
}

fn print_match_arm(a: &MatchArm) -> String {
    let mut out = String::new();
    out.push_str(&print_pattern(&a.pat));
    out.push_str(" => ");
    out.push_str(&print_expr(&a.body));
    out
}



    fn print_expr(e: &Expr) -> String {
    match e {
        Expr::Unit => "unit".to_string(),
        Expr::Bool(true) => "true".to_string(),
        Expr::Bool(false) => "false".to_string(),
        Expr::Int(z) => z.clone(),
        Expr::Text(s) => format!("{:?}", s), // bootstrap
        Expr::BytesHex(h) => format!("b{:?}", h),

        Expr::List(items) => {
            let mut out = String::from("[");
            for (i, it) in items.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&print_expr(it));
            }
            out.push(']');
            out
        }

        Expr::Ident(x) => x.clone(),
        Expr::UnaryMinus(x) => format!("(- {})", print_expr(x)),

        Expr::BinOp { op, lhs, rhs } => {
            format!("({} {} {})", print_expr(lhs), binop_str(op), print_expr(rhs))
        }

        Expr::Call { f, args } => {
            let mut s = String::new();
            s.push_str(f);
            s.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                s.push_str(&print_expr(a));
            }
            s.push(')');
            s
        }

        Expr::If { c, t, e } => format!(
            "if {} {{ {} }} else {{ {} }}",
            print_expr(c),
            print_block(t),
            print_block(e)
        ),

        Expr::RecordLit(fields) => {
            let mut out = String::new();
            out.push_str("{");
            for (j, (k, v)) in fields.iter().enumerate() {
                if j != 0 { out.push_str(","); }
                out.push_str(k);
                out.push_str(":");
                out.push_str(&print_expr(v));
            }
            out.push_str("}");
            out
        }

        Expr::FieldGet { base, field } => {
            let mut out = String::new();
            out.push_str(&print_expr(base));
            out.push_str(".");
            out.push_str(field);
            out
        }

        Expr::Match { scrut, arms } => {
            let mut out = String::new();
            out.push_str("match ");
            out.push_str(&print_expr(scrut));
            out.push_str(" {");
            for (i, a) in arms.iter().enumerate() {
                if i != 0 { out.push_str(","); }
                out.push_str(" ");
                out.push_str(&print_match_arm(a));
            }
            out.push_str(" }");
            out
        }
    }
}


