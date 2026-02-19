# FARD v0.5

FARD is a programming language with traceable execution. Every program
that runs produces a cryptographic witness proving what code ran, what
it received, and what it returned. The witness is a value, encoded as
canonical bytes, identified by its hash.

Traceability is not a feature. It is an invariant of every execution.

## Architecture

```
Layer 5  Execution ABI v0        bundle → ENC(W*) on stdout
Layer 4  Registry Semantics v0   content-addressed witness storage
Layer 3  Composition Semantics   executions link by verified RunID
Layer 2  Artifact Semantics      same (program, input, deps) → same RunID
Layer 1  Value Core v0           same value → same bytes → same hash
```

Each layer depends only on the one below. The entire system reduces to
one primitive: `CID(bytes) = "sha256:" || hex(SHA256(bytes))`.

## Crates

|Crate        |Purpose                                                             |
|-------------|--------------------------------------------------------------------|
|`valuecore`  |Value encoding, decoding, hashing (Layer 1)                         |
|`witnesscore`|Witness construction and identity projection (Layer 2/3)            |
|`abirunner`  |Bundle runner — pure function from bundle to witness bytes (Layer 5)|
|`registry`   |Content-addressed storage by CID (Layer 4)                          |
|`fardc`      |Compiler — .fard source to bundle                                   |
|`fardlang`   |Parser, canonical printer, effect checker, evaluator                |
|`fardcli`    |Developer CLI (`fard run`)                                          |

## Quick Start

Build everything:

```bash
cargo build
```

Write a program:

```fard
module main

fn main(): int {
  let x = 10
  let y = mul(x, 3)
  add(y, rem(x, 3))
}
```

Run it directly:

```bash
target/debug/fard run program.fard
```

Output:

```json
{"t":"int","v":"31"}
```

Compile to bundle and run through the trust stack:

```bash
target/debug/fardc --src program.fard --out _bundle
target/debug/abirun _bundle
```

Output is `ENC(W*)` — the canonical witness bytes. The SHA-256 of those
bytes is the RunID, the unique identity of this execution.

## Gates

The trust stack is verified by 12 gates. Each gate is a test that
asserts a specific property of the system by checking exact bytes
and frozen cryptographic hashes.

Run all gates:

```bash
bash tools/gates_stack_v0.sh
```

### Gate Summary

|Gate|Property                                                        |
|----|----------------------------------------------------------------|
|1   |ABI Vector 0 — minimal program produces frozen RunID            |
|2   |Satisfied effect — bundle value becomes witness digest          |
|3   |Missing fact precedence — import errors before effect errors    |
|4   |Registry round-trip — PUT/GET preserves CID identity            |
|5   |Compiler to runner — fardc → bundle → abirun → frozen RunID     |
|6   |Canonical module bytes — formatting variance, identical CID     |
|7   |Parse smoke — full language frontend parses correctly           |
|8   |Canonical printer stability — parse/print/parse round-trip      |
|9   |Effect checker — undeclared effect use rejected                 |
|10  |Effect checker — declared effect use accepted                   |
|11  |Evaluator smoke — let, arithmetic, conditionals, functions      |
|12  |Evaluator integration — computed result through full trust chain|

All 12 gates pass. All frozen hashes are machine-verified.

## Language

FARD v0.5 is a pure functional language. Programs are organized into
modules with typed function declarations, effect declarations, and
algebraic type definitions.

### What you can write today

```fard
module main

fn factorial(n: int, acc: int): int {
  if eq(n, 0) { acc }
  else { factorial(sub(n, 1), mul(n, acc)) }
}

fn main(): int {
  factorial(10, 1)
}
```

```fard
module main

fn sum_list(xs: list, i: int, acc: int): int {
  if eq(i, list_len(xs)) { acc }
  else { sum_list(xs, add(i, 1), add(acc, list_get(xs, i))) }
}

fn main(): int {
  let xs = [10, 20, 30, 40]
  sum_list(xs, 0, 0)
}
```

### Language Features

- Module declarations with `module name`
- Function declarations with typed parameters, return types, and effect clauses
- Let bindings in blocks
- Conditionals: `if expr { block } else { block }`
- List literals: `[a, b, c]`
- User-defined recursive functions with depth limit (1024)
- Type declarations: records and sum types
- Effect declarations with `uses` enforcement

### Builtin Functions

|Function           |Signature         |
|-------------------|------------------|
|`add(a, b)`        |int × int → int   |
|`sub(a, b)`        |int × int → int   |
|`mul(a, b)`        |int × int → int   |
|`div(a, b)`        |int × int → int   |
|`rem(a, b)`        |int × int → int   |
|`neg(a)`           |int → int         |
|`eq(a, b)`         |V × V → bool      |
|`lt(a, b)`         |V × V → bool      |
|`not(a)`           |bool → bool       |
|`text_concat(a, b)`|text × text → text|
|`int_to_text(a)`   |int → text        |
|`list_len(xs)`     |list → int        |
|`list_get(xs, i)`  |list × int → V    |
|`map_get(m, k)`    |map × text → V    |

### Grammar

See `spec/fardlang_grammar_v0.5.txt` for the complete grammar
matching the current parser.

## Specifications

|Document                          |Contents                          |
|----------------------------------|----------------------------------|
|`spec/fard_spec_stack_v0_final.md`|Trust stack specification (frozen)|
|`spec/fardlang_grammar_v0.5.txt`  |Surface language grammar          |
|`spec/fard_formal_description.txt`|Formal description of FARD        |

## Determinism

Identical source + identical input + identical effects = identical RunID.

This holds across machines, across implementations, across time. The
RunID is a pure function of the program, its input, and its environment.
Change any of these, the RunID changes. Keep them identical, the RunID
is identical.

## License

MIT