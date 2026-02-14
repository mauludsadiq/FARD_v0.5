# FARD v0.5 / v1.0 Notation Closure (M6)

This document is normative for the notation layer: tokenization, parsing, precedence/associativity, and
the syntactic rewrite contracts that are gate-verified.

Status: behavior is enforced by gate + milestone tranches. This text exists to freeze the contract in
written form and bind it to the already-passing suites.

## 1. Lexical layer

### 1.1 Whitespace
- Space, tab, and newline are separators.
- Newlines may terminate sequences where grammar permits.

### 1.2 Comments
- Line comments: begin with `//` and end at newline.
- Block comments: `/* ... */`.
- Block comments may be nested.
- Unterminated block comments are parse errors.

### 1.3 Identifiers
- Ident rules are as enforced by the lexer gates.
- Reserved tokens are not permitted as identifiers.

### 1.4 Strings
- String literal forms and escape rules are as enforced by the string gates.
- Unterminated strings are parse errors.

### 1.5 Numbers
- Integer and float syntax is as enforced by the number gates.
- Invalid numeric forms are parse errors.

## 2. Expression grammar and precedence

This layer is sealed by the B-series precedence tranches and golden program byte tests.

### 2.1 Precedence lattice (highest to lowest)
The exact operator/construct precedence and associativity are as proven by the B-series tranches:
- primary (literals, identifiers, grouping)
- postfix (if any)
- prefix (if any)
- application/call (if any)
- arithmetic / comparison (as defined by tranches)
- pipeline `|>`
- qmark `?`
- match
- let / seq boundaries

(Exact forms are those accepted/rejected by the parser under the test suite.)

### 2.2 Pipeline rewrite contract
- `a |> f` rewrites to `f(a)` where `f` is a callable expression, as enforced by the pipeline tranches.
- Rewrite associativity and evaluation order follow the C-series tranches.

### 2.3 Qmark binding
- `x ?` is the qmark operator with binding/precedence as enforced by the B-series and M1 equivalence tests.

## 3. Statements / top-level forms

- `use ...` import forms and their parse constraints are sealed by the import tranches.
- `let` binding forms and boundaries are sealed by the let/seq tranches.
- `match` syntax, pattern grammar, and parse errors are sealed by match tranches.

## 4. Canonical examples

The canonical byte-identity examples are in:
- `spec/v1_0/golden_programs/` (if present)
- `spec/v1_0/golden_bundle/v1/` (golden bundle)

