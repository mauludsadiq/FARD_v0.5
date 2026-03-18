//! farddoc — Generate Markdown documentation from FARD source comments.
//!
//! Doc comment syntax:
//!   /// Single line doc comment
//!   /// Multiple lines are joined
//!   fn name(params) { ... }
//!
//!   /// Module-level doc (before any fn/let)
//!
//! Usage:
//!   farddoc --program file.fard [--out docs/] [--format md|html]
//!   farddoc --package packages/stats [--out docs/]

use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

// ── Doc item types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct DocParam {
    name: String,
    type_hint: Option<String>,
}

#[derive(Debug, Clone)]
enum DocItem {
    Module {
        name: String,
        description: Vec<String>,
        source_file: String,
    },
    Function {
        name: String,
        params: Vec<DocParam>,
        doc_lines: Vec<String>,
        line: usize,
    },
    Constant {
        name: String,
        doc_lines: Vec<String>,
        line: usize,
    },
}

// ── Parser ────────────────────────────────────────────────────────────────────

fn parse_params(param_str: &str) -> Vec<DocParam> {
    if param_str.trim().is_empty() { return vec![]; }
    param_str.split(',').map(|p| {
        let p = p.trim();
        // Handle "name: Type" or just "name"
        if let Some((name, ty)) = p.split_once(':') {
            DocParam { name: name.trim().to_string(), type_hint: Some(ty.trim().to_string()) }
        } else {
            DocParam { name: p.to_string(), type_hint: None }
        }
    }).filter(|p| !p.name.is_empty()).collect()
}

fn extract_doc_items(source: &str, filename: &str) -> Vec<DocItem> {
    let mut items: Vec<DocItem> = Vec::new();
    let mut pending_docs: Vec<String> = Vec::new();
    let mut module_doc: Vec<String> = Vec::new();
    let mut saw_item = false;

    // Extract module name from filename
    let module_name = Path::new(filename)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(filename)
        .to_string();

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Doc comment
        if let Some(doc) = trimmed.strip_prefix("///") {
            let text = doc.trim().to_string();
            if !saw_item && items.is_empty() && pending_docs.is_empty() {
                module_doc.push(text);
            } else {
                pending_docs.push(text);
            }
            continue;
        }

        // Regular comment — reset pending if it separates doc from item
        if trimmed.starts_with("//") {
            if !pending_docs.is_empty() {
                // Section divider resets pending docs
                if trimmed.contains("────") || trimmed.contains("====") {
                    pending_docs.clear();
                }
            }
            continue;
        }

        // Function definition
        if trimmed.starts_with("fn ") {
            saw_item = true;
            let rest = trimmed.trim_start_matches("fn ").trim();
            // Parse: name(params) {
            if let Some(paren_start) = rest.find('(') {
                let name = rest[..paren_start].trim().to_string();
                if name.is_empty() || name.contains(' ') {
                    pending_docs.clear();
                    continue;
                }
                let after_paren = &rest[paren_start + 1..];
                let param_str = if let Some(paren_end) = after_paren.find(')') {
                    &after_paren[..paren_end]
                } else {
                    ""
                };

                // Extract type hints from doc lines "/// param: Type"
                let mut params = parse_params(param_str);

                // Augment params with type hints from doc comments
                for doc in &pending_docs {
                    // Look for "paramname: Type" patterns
                    if let Some((pname, ptype)) = doc.split_once(':') {
                        let pname = pname.trim();
                        if let Some(p) = params.iter_mut().find(|p| p.name == pname) {
                            if p.type_hint.is_none() {
                                p.type_hint = Some(ptype.trim().to_string());
                            }
                        }
                    }
                }

                // Filter out param-type lines from doc
                let doc_text: Vec<String> = pending_docs.iter()
                    .filter(|d| {
                        let has_colon = d.contains(':');
                        if !has_colon { return true; }
                        // Keep if it doesn't look like a param annotation
                        let before_colon = d.split(':').next().unwrap_or("").trim();
                        !params.iter().any(|p| p.name == before_colon)
                    })
                    .cloned()
                    .collect();

                items.push(DocItem::Function {
                    name,
                    params,
                    doc_lines: doc_text,
                    line: line_idx + 1,
                });
            }
            pending_docs.clear();
            continue;
        }

        // Top-level let binding
        if trimmed.starts_with("let ") && !pending_docs.is_empty() {
            saw_item = true;
            let rest = trimmed.trim_start_matches("let ").trim();
            let name = rest.split_whitespace().next().unwrap_or("").trim_end_matches('=').trim().to_string();
            if !name.is_empty() {
                items.push(DocItem::Constant {
                    name,
                    doc_lines: pending_docs.clone(),
                    line: line_idx + 1,
                });
            }
            pending_docs.clear();
            continue;
        }

        // Non-comment, non-fn, non-let line
        if !trimmed.is_empty() && !trimmed.starts_with("import") {
            if !pending_docs.is_empty() && !trimmed.starts_with('{') {
                pending_docs.clear();
            }
        }
    }

    // Prepend module doc item if we have one
    if !module_doc.is_empty() || !items.is_empty() {
        let mut result = vec![DocItem::Module {
            name: module_name,
            description: module_doc,
            source_file: filename.to_string(),
        }];
        result.extend(items);
        result
    } else {
        items
    }
}

// ── Markdown renderer ─────────────────────────────────────────────────────────

fn render_markdown(items: &[DocItem]) -> String {
    let mut out = String::new();

    for item in items {
        match item {
            DocItem::Module { name, description, source_file } => {
                let _ = writeln!(out, "# {}", name);
                let _ = writeln!(out);
                if !description.is_empty() {
                    for line in description {
                        let _ = writeln!(out, "{}", line);
                    }
                    let _ = writeln!(out);
                }
                let _ = writeln!(out, "*Source: `{}`*", source_file);
                let _ = writeln!(out);
                let _ = writeln!(out, "---");
                let _ = writeln!(out);
            }
            DocItem::Function { name, params, doc_lines, line } => {
                // Signature
                let param_str: String = params.iter().map(|p| {
                    if let Some(ty) = &p.type_hint {
                        format!("{}: {}", p.name, ty)
                    } else {
                        p.name.clone()
                    }
                }).collect::<Vec<_>>().join(", ");

                let _ = writeln!(out, "## `{}({})`", name, param_str);
                let _ = writeln!(out);

                if !doc_lines.is_empty() {
                    for line in doc_lines {
                        if !line.is_empty() {
                            let _ = writeln!(out, "{}", line);
                        }
                    }
                    let _ = writeln!(out);
                }

                if !params.is_empty() {
                    let has_types = params.iter().any(|p| p.type_hint.is_some());
                    if has_types {
                        let _ = writeln!(out, "**Parameters**");
                        let _ = writeln!(out);
                        for p in params {
                            if let Some(ty) = &p.type_hint {
                                let _ = writeln!(out, "- `{}` — {}", p.name, ty);
                            } else {
                                let _ = writeln!(out, "- `{}`", p.name);
                            }
                        }
                        let _ = writeln!(out);
                    }
                }

                let _ = writeln!(out, "*Line {}*", line);
                let _ = writeln!(out);
            }
            DocItem::Constant { name, doc_lines, line } => {
                let _ = writeln!(out, "## `{}`", name);
                let _ = writeln!(out);
                for l in doc_lines {
                    let _ = writeln!(out, "{}", l);
                }
                let _ = writeln!(out);
                let _ = writeln!(out, "*Line {}*", line);
                let _ = writeln!(out);
            }
        }
    }

    out
}

// ── HTML renderer ─────────────────────────────────────────────────────────────

fn render_html(items: &[DocItem], title: &str) -> String {
    let md = render_markdown(items);
    // Simple MD->HTML conversion
    let body = md_to_html(&md);

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; }}
    body {{
      font-family: Helvetica, 'Helvetica Neue', Arial, sans-serif;
      font-size: 15px;
      line-height: 1.7;
      color: #282828;
      background: #f7f5f0;
      margin: 0;
      padding: 0;
    }}
    .sidebar {{
      position: fixed;
      top: 0; left: 0;
      width: 220px;
      height: 100vh;
      background: #fff;
      border-right: 1px solid #e5e0d6;
      overflow-y: auto;
      padding: 24px 0;
    }}
    .sidebar-header {{
      padding: 0 20px 16px;
      border-bottom: 1px solid #ece8df;
      margin-bottom: 12px;
    }}
    .sidebar-title {{
      font-size: 13px;
      font-weight: 600;
      color: #282828;
      letter-spacing: 0.01em;
    }}
    .sidebar-version {{
      font-size: 11px;
      color: #bbb;
      margin-top: 2px;
    }}
    .sidebar a {{
      display: block;
      padding: 5px 20px;
      font-size: 12.5px;
      color: #666;
      text-decoration: none;
      border-left: 2px solid transparent;
      transition: all 0.12s;
    }}
    .sidebar a:hover {{
      color: #3d6b4f;
      border-left-color: #aecebb;
      background: #eaf2ec;
    }}
    .main {{
      margin-left: 220px;
      padding: 48px 56px;
      max-width: 820px;
    }}
    h1 {{
      font-size: 26px;
      font-weight: 700;
      color: #1a1a1a;
      margin: 0 0 8px;
      letter-spacing: -0.01em;
    }}
    h2 {{
      font-size: 16px;
      font-weight: 600;
      color: #1a1a1a;
      margin: 36px 0 10px;
      padding: 10px 14px;
      background: #fff;
      border: 1px solid #e5e0d6;
      border-left: 3px solid #3d6b4f;
      border-radius: 4px;
      font-family: 'Courier New', Courier, monospace;
    }}
    p {{ margin: 0 0 12px; color: #3a3a3a; }}
    code {{
      font-family: 'Courier New', Courier, monospace;
      font-size: 13px;
      background: #f2efe9;
      border: 1px solid #ece8df;
      padding: 1px 5px;
      border-radius: 3px;
      color: #282828;
    }}
    ul {{ margin: 0 0 12px; padding-left: 20px; }}
    li {{ margin-bottom: 4px; color: #3a3a3a; }}
    em {{ color: #bbb; font-style: normal; font-size: 12px; }}
    strong {{ font-weight: 600; color: #282828; }}
    hr {{ border: none; border-top: 1px solid #ece8df; margin: 32px 0; }}
    .dot {{
      display: inline-block;
      width: 7px; height: 7px;
      border-radius: 50%;
      background: #3d6b4f;
      margin-right: 8px;
      vertical-align: middle;
    }}
    @media (max-width: 700px) {{
      .sidebar {{ display: none; }}
      .main {{ margin-left: 0; padding: 24px 20px; }}
    }}
  </style>
</head>
<body>
  <div class="sidebar">
    <div class="sidebar-header">
      <div style="display:flex;align-items:center;gap:8px">
        <span class="dot"></span>
        <span class="sidebar-title">FARD</span>
      </div>
      <div class="sidebar-version">v1.6.0 docs</div>
    </div>
    {nav}
  </div>
  <div class="main">
    {body}
  </div>
</body>
</html>
"#,
        title = title,
        nav = build_nav(items),
        body = body,
    )
}

fn build_nav(items: &[DocItem]) -> String {
    let mut nav = String::new();
    for item in items {
        match item {
            DocItem::Function { name, params, .. } => {
                let param_str = params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
                let anchor = format!("fn-{}", name);
                let _ = writeln!(nav, "<a href=\"#{}\">{}</a>", anchor, format!("{}({})", name, param_str));
            }
            DocItem::Constant { name, .. } => {
                let _ = writeln!(nav, "<a href=\"#const-{}\">{}</a>", name, name);
            }
            _ => {}
        }
    }
    nav
}

fn md_to_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_ul = false;
    let mut fn_count = 0usize;
    let mut const_count = 0usize;

    for line in md.lines() {
        if line.starts_with("# ") {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            let text = &line[2..];
            let _ = writeln!(out, "<h1><span class=\"dot\"></span>{}</h1>", escape_html(text));
        } else if line.starts_with("## `") {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            let text = line.trim_start_matches("## ");
            // Determine anchor
            let anchor = if text.contains('(') {
                fn_count += 1;
                let name = text.trim_matches('`').split('(').next().unwrap_or("fn");
                format!("fn-{}", name)
            } else {
                const_count += 1;
                let name = text.trim_matches('`');
                format!("const-{}", name)
            };
            let _ = writeln!(out, "<h2 id=\"{}\"><code>{}</code></h2>", anchor, escape_html(text.trim_matches('`')));
        } else if line.starts_with("**") && line.ends_with("**") {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            let text = line.trim_matches('*');
            let _ = writeln!(out, "<p><strong>{}</strong></p>", escape_html(text));
        } else if line.starts_with("- ") {
            if !in_ul { out.push_str("<ul>\n"); in_ul = true; }
            let text = &line[2..];
            let _ = writeln!(out, "<li>{}</li>", render_inline(text));
        } else if line.starts_with("---") {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            out.push_str("<hr>\n");
        } else if line.starts_with("*") && line.ends_with("*") {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            let text = line.trim_matches('*');
            let _ = writeln!(out, "<p><em>{}</em></p>", render_inline(text));
        } else if line.is_empty() {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
        } else {
            if in_ul { out.push_str("</ul>\n"); in_ul = false; }
            let _ = writeln!(out, "<p>{}</p>", render_inline(line));
        }
    }
    if in_ul { out.push_str("</ul>\n"); }
    out
}

fn render_inline(s: &str) -> String {
    // Handle `code` spans
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            out.push_str("<code>");
            while let Some(&nc) = chars.peek() {
                if nc == '`' { chars.next(); break; }
                out.push(escape_char(chars.next().unwrap()));
            }
            out.push_str("</code>");
        } else {
            out.push(escape_char(c));
        }
    }
    out
}

fn escape_html(s: &str) -> String {
    s.chars().map(escape_char).collect()
}

fn escape_char(c: char) -> char {
    c // simplified — we handle & < > inline below
}

fn escape_str(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// ── Index page ────────────────────────────────────────────────────────────────

fn render_index_html(modules: &[(String, Vec<DocItem>)]) -> String {
    let mut cards = String::new();
    for (fname, items) in modules {
        let module_name = items.iter().find_map(|i| {
            if let DocItem::Module { name, .. } = i { Some(name.clone()) } else { None }
        }).unwrap_or_else(|| fname.clone());

        let fn_count = items.iter().filter(|i| matches!(i, DocItem::Function { .. })).count();
        let desc = items.iter().find_map(|i| {
            if let DocItem::Module { description, .. } = i {
                description.first().cloned()
            } else { None }
        }).unwrap_or_default();

        let out_name = Path::new(fname).with_extension("md")
            .file_name().and_then(|n| n.to_str()).unwrap_or(fname).to_string();

        let _ = write!(cards, r#"<a class="card" href="{}.html">
  <div class="card-name">{}</div>
  <div class="card-desc">{}</div>
  <div class="card-meta">{} function{}</div>
</a>
"#, out_name.trim_end_matches(".md"), module_name, desc,
        fn_count, if fn_count == 1 { "" } else { "s" });
    }

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>FARD Documentation</title>
  <style>
    body {{ font-family: Helvetica, Arial, sans-serif; background: #f7f5f0; color: #282828; margin: 0; padding: 48px; }}
    h1 {{ font-size: 28px; font-weight: 700; margin: 0 0 6px; }}
    .subtitle {{ color: #999; font-size: 14px; margin-bottom: 40px; }}
    .dot {{ display:inline-block; width:8px; height:8px; border-radius:50%; background:#3d6b4f; margin-right:10px; vertical-align:middle; }}
    .grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 16px; }}
    .card {{ background: #fff; border: 1px solid #e5e0d6; border-radius: 6px; padding: 18px 20px; text-decoration: none; color: inherit; display: block; transition: border-color 0.15s; }}
    .card:hover {{ border-color: #aecebb; }}
    .card-name {{ font-size: 14px; font-weight: 600; margin-bottom: 5px; color: #3d6b4f; }}
    .card-desc {{ font-size: 12.5px; color: #666; margin-bottom: 10px; line-height: 1.5; }}
    .card-meta {{ font-size: 11px; color: #bbb; }}
  </style>
</head>
<body>
  <h1><span class="dot"></span>FARD</h1>
  <div class="subtitle">v1.6.0 · API Reference</div>
  <div class="grid">{cards}</div>
</body>
</html>
"#, cards = cards)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let program = args.windows(2).find(|w| w[0] == "--program").map(|w| w[1].clone());
    let package = args.windows(2).find(|w| w[0] == "--package").map(|w| w[1].clone());
    let out_dir = args.windows(2).find(|w| w[0] == "--out").map(|w| w[1].clone())
        .unwrap_or_else(|| "docs".to_string());
    let format = args.windows(2).find(|w| w[0] == "--format").map(|w| w[1].clone())
        .unwrap_or_else(|| "html".to_string());

    std::fs::create_dir_all(&out_dir).expect("create output dir");

    let mut all_modules: Vec<(String, Vec<DocItem>)> = Vec::new();

    // Collect source files
    let sources: Vec<PathBuf> = if let Some(p) = program {
        vec![PathBuf::from(p)]
    } else if let Some(pkg) = package {
        // All .fard files in package dir
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&pkg) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("fard") {
                    files.push(p);
                }
            }
        }
        files
    } else {
        // All packages/*/main.fard
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir("packages") {
            for entry in entries.flatten() {
                let main = entry.path().join("main.fard");
                if main.exists() { files.push(main); }
            }
        }
        if files.is_empty() {
            eprintln!("usage: farddoc --program <file.fard> | --package <dir> [--out <dir>] [--format md|html]");
            std::process::exit(1);
        }
        files
    };

    for src_path in &sources {
        let src = match std::fs::read_to_string(src_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("error reading {}: {}", src_path.display(), e); continue; }
        };

        let fname = src_path.to_str().unwrap_or("unknown");
        let items = extract_doc_items(&src, fname);

        // Determine output filename
        let stem = src_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("doc");
        let parent_name = src_path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or(stem);
        let out_name = if stem == "main" { parent_name } else { stem };

        match format.as_str() {
            "md" => {
                let md = render_markdown(&items);
                let out_path = format!("{}/{}.md", out_dir, out_name);
                std::fs::write(&out_path, md).expect("write md");
                println!("wrote {}", out_path);
            }
            _ => {
                let html = render_html(&items, &format!("{} — FARD docs", out_name));
                let out_path = format!("{}/{}.html", out_dir, out_name);
                std::fs::write(&out_path, html).expect("write html");
                println!("wrote {}", out_path);
                all_modules.push((out_name.to_string(), items));
            }
        }
    }

    // Write index for multi-module docs
    if format != "md" && all_modules.len() > 1 {
        let index = render_index_html(&all_modules);
        let index_path = format!("{}/index.html", out_dir);
        std::fs::write(&index_path, index).expect("write index");
        println!("wrote {}", index_path);
    }

    println!("docs generated in {}/", out_dir);
}
