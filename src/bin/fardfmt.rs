//! fardfmt — FARD code formatter
//! Usage: fardfmt [--check] [--stdin] [file.fard ...]
//!
//! Formatting rules:
//!   - 2-space indent inside fn/match/test blocks
//!   - One space around binary operators
//!   - Record literals: { k: v, k: v } with spaces inside braces
//!   - let chains: one per line
//!   - Max line length: 100 chars (soft)
//!   - Trailing whitespace removed
//!   - Single trailing newline

use std::process;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: fardfmt [--check] [--stdin] [file.fard ...]");
        eprintln!("       fardfmt --check main.fard   # exit 1 if not formatted");
        eprintln!("       fardfmt --stdin             # read from stdin");
        process::exit(0);
    }

    let check_mode = args.iter().any(|a| a == "--check");
    let stdin_mode = args.iter().any(|a| a == "--stdin");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let mut any_changed = false;

    if stdin_mode {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).unwrap();
        let formatted = format_src(&input);
        print!("{}", formatted);
        return;
    }

    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => { eprintln!("fardfmt: {}: {}", file, e); process::exit(2); }
        };
        let formatted = format_src(&src);
        if formatted != src {
            any_changed = true;
            if check_mode {
                eprintln!("fardfmt: {} is not formatted", file);
            } else {
                std::fs::write(file, &formatted).unwrap();
                eprintln!("fardfmt: formatted {}", file);
            }
        }
    }

    if check_mode && any_changed {
        process::exit(1);
    }
}

fn format_src(src: &str) -> String {
    let mut out = String::new();
    let mut indent: usize = 0;
    let mut lines = src.lines().peekable();
    let mut prev_blank = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Preserve intentional blank lines (max 1 consecutive)
        if trimmed.is_empty() {
            if !prev_blank && !out.is_empty() {
                out.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        // Adjust indent based on closing braces at start
        let close_count = trimmed.chars().take_while(|&c| c == '}').count();
        if close_count > 0 && indent >= close_count * 2 {
            indent -= close_count * 2;
        }

        // Format the line
        let formatted_line = format_line(trimmed, indent);

        // Count net brace change for next line indent
        let opens = count_unquoted(trimmed, '{');
        let closes = count_unquoted(trimmed, '}');
        let net = opens as isize - closes as isize;
        if net > 0 {
            indent += (net as usize) * 2;
        } else if net < 0 {
            let decrease = (-net as usize) * 2;
            // Already adjusted for leading closes above
            // Only adjust for non-leading closes
            let leading_closes = trimmed.chars().take_while(|&c| c == '}').count();
            let trailing_net = opens as isize - (closes as isize - leading_closes as isize);
            if trailing_net < 0 {
                let dec = (-trailing_net as usize) * 2;
                if indent >= dec { indent -= dec; }
            }
        }

        out.push_str(&formatted_line);
        out.push('\n');
    }

    // Ensure single trailing newline
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn format_line(line: &str, indent: usize) -> String {
    let prefix = " ".repeat(indent);
    let body = format_tokens(line);
    format!("{}{}", prefix, body)
}

fn format_tokens(line: &str) -> String {
    // Tokenize and reformat spacing around operators and punctuation
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // String literals — copy verbatim
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                let sc = chars[i];
                out.push(sc);
                i += 1;
                if sc == '\\' && i < chars.len() {
                    out.push(chars[i]);
                    i += 1;
                } else if sc == '"' {
                    break;
                }
            }
            continue;
        }

        // Comments — copy rest of line verbatim
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // Add space before comment if needed
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            while i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // Binary operators: ensure single space around them
        if is_op_char(c) {
            // Collect full operator
            let op_start = i;
            while i < chars.len() && is_op_char(chars[i]) {
                i += 1;
            }
            let op: String = chars[op_start..i].iter().collect();

            // Don't add spaces around -> in type annotations or => in match
            let needs_space = !matches!(op.as_str(), "." | "::" | "@");

            if needs_space {
                // Ensure space before
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('(') {
                    out.push(' ');
                }
                out.push_str(&op);
                // Ensure space after (if not at end or followed by space)
                if i < chars.len() && chars[i] != ' ' && chars[i] != '\n' {
                    out.push(' ');
                }
            } else {
                out.push_str(&op);
            }
            continue;
        }

        // Opening brace: ensure space inside
        if c == '{' {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push('{');
            i += 1;
            // Skip existing spaces
            while i < chars.len() && chars[i] == ' ' { i += 1; }
            if i < chars.len() && chars[i] != '}' {
                out.push(' ');
            }
            continue;
        }

        // Closing brace: ensure space before
        if c == '}' {
            if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('{') {
                out.push(' ');
            }
            out.push('}');
            i += 1;
            continue;
        }

        // Comma: no space before, one space after
        if c == ',' {
            // Remove trailing space before comma
            while out.ends_with(' ') { out.pop(); }
            out.push(',');
            i += 1;
            while i < chars.len() && chars[i] == ' ' { i += 1; }
            if i < chars.len() && chars[i] != ')' && chars[i] != ']' && chars[i] != '}' {
                out.push(' ');
            }
            continue;
        }

        // Colon in record field: "key: value"
        if c == ':' && i + 1 < chars.len() && chars[i + 1] != ':' {
            while out.ends_with(' ') { out.pop(); }
            out.push(':');
            i += 1;
            while i < chars.len() && chars[i] == ' ' { i += 1; }
            out.push(' ');
            continue;
        }

        // Whitespace: collapse multiple spaces to one
        if c == ' ' || c == '\t' {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }

    out.trim_end().to_string()
}

fn is_op_char(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~')
}

fn count_unquoted(s: &str, target: char) -> usize {
    let mut count = 0;
    let mut in_str = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' && !in_str { in_str = true; continue; }
        if c == '"' && in_str { in_str = false; continue; }
        if c == '\\' && in_str { chars.next(); continue; }
        if !in_str && c == target { count += 1; }
    }
    count
}
