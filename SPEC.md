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


## Manifest Contract Spec (Intent Tranche v1.0: g11–g15)

This section specifies the **normative contract** enforced by `tests/intent_tranche_v1_0_g11_g15.rs` over:

- `spec/stdlib_surface.v1_0.ontology.json`

### Checklist (must hold)

#### A) File + parseability
- [ ] File exists at `spec/stdlib_surface.v1_0.ontology.json`.
- [ ] File bytes parse as JSON (UTF-8 JSON text; no trailing garbage).

#### B) Top-level shape + key order
- [ ] Top-level value is an **object**.
- [ ] Top-level keys are **exactly**: `["modules","schema"]` in that order.
- [ ] `schema` is a **string** and equals: `fard.stdlib_surface.ontology.v1_0`.
- [ ] `modules` is an **object** and is **non-empty**.

#### C) Canonical bytes (exact file-bytes constraint)
- [ ] The file bytes are exactly: `canon_compact_bytes(JSON) + "\n"`.
  - Canonicalization rule: recursively sort **all object keys** lexicographically; arrays preserve order; scalars unchanged.
  - Serialization rule: compact JSON (no extra whitespace), then a single trailing newline `\n`.

#### D) Module keys: canonical path + sorted order
For each module key `mname` in `modules`:
- [ ] `mname` is a **canonical module path**:
  - must start with `std/`
  - tail (after `std/`) is non-empty
  - tail characters are only `[a-z0-9_]`
- [ ] Module keys in `modules` are iterated in **lexicographic ascending order**.

#### E) Module value shape
For each module entry `(mname, mval)`:
- [ ] `mval` is an **object** with keyset **exactly**: `{ "exports" }`.
- [ ] `mval.exports` is an **object**.

#### F) Export keys: sorted order
For each module `mname`, within `exports`:
- [ ] Export names `ename` are iterated in **lexicographic ascending order**.

#### G) Export record shape + enums
For each export entry `(ename, eval)` in `mname.exports`:
- [ ] `eval` is an **object** with keyset equal to **one of**:
  - `{ "intent","pipe","return","status" }`
  - `{ "intent","pipe","return","status","notes" }`
- [ ] `intent` is a **string** in: `{ "construct","transform","query","effect" }`
- [ ] `pipe` is a **string** in: `{ "Stage","No" }`
- [ ] `return` is a **string** in: `{ "Value","Option","Result" }`
- [ ] `status` is a **string** in: `{ "implemented","planned" }`
- [ ] If present, `notes` is a **string**.

#### H) Stage constraint
For every export:
- [ ] If `pipe == "Stage"`, then `intent != "construct"`.

#### I) Fully-qualified uniqueness
Across the entire manifest:
- [ ] Fully-qualified export names are unique:
  - `fq := mname + "." + ename`
  - no duplicate `fq` across all modules/exports.

#### J) Minimum surface (v1.0 presence guarantees)
The following modules/exports must exist in `modules` and in each module's `exports`:

- [ ] `std/result` exports: `Ok`, `Err`, `andThen`
- [ ] `std/list` exports: `len`, `hist_int`, `sort_by_int_key`
- [ ] `std/str` exports: `len`, `concat`
- [ ] `std/json` exports: `encode`, `decode`
- [ ] `std/map` exports: `get`, `set`, `keys`, `values`, `has`
- [ ] `std/grow` exports: `append`, `merge`
- [ ] `std/flow` exports: `id`, `tap`

### Implied invariants (derived from the checklist)

1) Canonical JSON bytes ⇒ stable digesting
- Because the file bytes must equal canonicalized compact JSON + newline, identical semantic content implies identical bytes and thus identical digests.

2) Deterministic module/export iteration
- Sorted module keys and sorted export keys imply deterministic iteration order in any consumer that iterates via the JSON object map.

3) Ontology discipline
- Every export is forced into a small, enumerated intent/pipe/return/status space with an optional human note, preventing ad-hoc fields.

4) Pipeline semantics safety
- The Stage constraint forbids “constructors as pipeline stages”, preserving the convention that Stage entries are value-first transforms/queries/effects, not literal constructors.

5) No ambiguous symbol resolution
- FQ uniqueness guarantees that `std/x.y` resolves to exactly one export across the entire manifest.

