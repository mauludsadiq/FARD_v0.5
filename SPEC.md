# FARD v0.5.0 Language Contract

Version: 0.5.0

## Changelog
Release date: 2026-02-03.
- 0.5.0 (2026-02-03): Grammar extended with -> token and Type productions; parser accepts fn f(x: Int) -> Int { ... } and stores annotations in the AST.

## Tokens
Lexing is deterministic: given identical program bytes, the token stream is identical.

### Keywords
- `let`
- `in`
- `if`
- `then`
- `else`
- `fn`
- `import`
- `as`
- `export`

The lexer produces a deterministic token stream from program bytes; non-semantic whitespace is ignored outside string literals.
- comments: start with `#` and run to end-of-line
- strings: double-quoted; contents are not further tokenized
- identifiers/keywords/integers/symbols: as implemented by the current lexer

## Grammar
The surface grammar below is normative for v0.5 and defines the parseable forms accepted by the parser.

```ebnf
module      ::= { item } EOF ;

item        ::= import_item
              | export_item
              | let_item
              | fn_item
              | expr_item ;

import_item ::= "import" "(" string ")" "as" ident ;
export_item ::= "export" "{" ident { "," ident } "}" ;
let_item    ::= "let" ident "=" expr ;
fn_item     ::= "fn" ident "(" [ param { "," param } ] ")" [ "->" type ] "{" expr "}" ;
param       ::= ident [ ":" type ] ;

expr_item   ::= expr ;

expr        ::= let_expr
              | if_expr
              | fn_call
              | field_access
              | list_lit
              | record_lit
              | atom
              | expr binop expr ;

let_expr    ::= "let" ident "=" expr "in" expr ;
if_expr     ::= "if" expr "then" expr "else" expr ;

fn_call     ::= ident "(" [ expr { "," expr } ] ")" ;
field_access::= expr "." ident ;

list_lit    ::= "[" [ expr { "," expr } ] "]" ;
record_lit  ::= "{" [ ident ":" expr { "," ident ":" expr } ] "}" ;

atom        ::= ident | int | string | "(" expr ")" ;
type        ::= "Int" | "String" | "Bool" | "Unit" | "Dynamic"
              | "List" "<" type ">"
              | "Rec" "{" ident ":" type { "," ident ":" type } "}"
              | "Func" "(" [ type { "," type } ] ")" "->" type
              | ident [ "<" type { "," type } ">" ] ;

binop       ::= "==" | "<=" | ">=" | "&&" | "||" | "+" | "-" | "*" | "/" ;
```

## Eval
Evaluation is deterministic and call-by-value. Functions are lexically scoped and close over the defining environment.

## Values
Runtime values include JSONable scalars, lists, and records; function values exist but are not JSONable.

## Trace
When enabled, trace output is append-only and deterministic for identical inputs (program bytes + stdlib bytes + toolchain).

## Modules
`import("std/x") as Alias` resolves a standard-library module path to a stable module identity for the given stdlib bytes.

## Out-dir
A run writes outputs under the selected out directory and MUST emit the following files:
- `trace.ndjson`
- `result.json`
- `stderr.txt`
- `error.json`

## Errors
Errors use stable string codes and deterministic rendering into `error.json` and `stderr.txt`.

