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

Write a program (braces optional):

```fard
module main

fn main() : Value
  let x = 10
  let y = mul(x, 3)
  add(y, rem(x, 3))
```

Run it:

```bash
target/debug/fard run program.fard
```

Output:

```json
{"t":"int","v":31}
```

Compile to bundle and run through the trust stack:

```bash
target/debug/fardc --src program.fard --out _bundle
target/debug/abirun _bundle
```

Output is `ENC(W*)` — the canonical witness bytes. The SHA-256 of those
bytes is the RunID, the unique identity of this execution.

## Gates

The trust stack is verified by 12 gates. Each gate asserts a specific
property of the system by checking exact bytes and frozen cryptographic
hashes.

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

FARD v0.5 is a pure functional language with gradual type enforcement,
first-class functions, pattern matching, and a pure-FARD standard
library. All runtime failures return `V::Err` values — nothing crashes
the interpreter.

### Syntax

Function bodies use either braces or indentation:

```fard
module example

// brace style
fn double(n: Int) : Int {
  mul(n, 2)
}

// indent style
fn triple(n: Int) : Int
  mul(n, 3)

fn main() : Value
  let a = double(5)
  let b = triple(5)
  {a: a, b: b}
```

### Type Enforcement

Type annotations on function parameters are enforced at call time.
`Value` means any type — no check. Specific types are checked and
return `V::Err` on mismatch:

```fard
fn add_ints(a: Int, b: Int) : Int
  add(a, b)

// add_ints(3, "oops") → {"t":"err","e":"ERROR_TYPE add_ints:b expected Int got Text"}
```

### Pattern Matching

```fard
fn describe(v: Value) : Value
  match v {
    ok(x) => text_concat("ok: ", int_to_text(x)),
    err(msg) => text_concat("err: ", msg),
    1 => "one",
    _ => "other"
  }
```

Constructor patterns `ok(x)` and `err(msg)` destructure result values.

### First-Class Functions

```fard
import std.list as list

fn main() : Value
  let nums = list.range(1, 6)
  let squares = list.map(nums, fn(x) { mul(x, x) })
  let sum = list.fold(squares, 0, fn(acc, x) { add(acc, x) })
  {squares: squares, sum: sum}
```

### Effects

Effects are declared and checked at parse time. Undeclared effects are
rejected. Effect call failures return `V::Err` — no hard crash:

```fard
module fetcher

import std.io as io
import std.http as http

fn main() : Value uses [io.clock_now, http.http_get]
  let t = io.clock_now()
  let r = http.http_get("https://example.com")
  match r {
    err(msg) => {error: msg, time: t},
    ok(body) => {body: body, time: t}
  }
```

### Fact Imports

A program can import the verified output of a prior run:

```fard
module derived

import prev: Run("sha256:7b28df5e...")

fn main() : Value
  map_get(prev, "result")
```

The receipt for the prior run is loaded, its `run_id` verified, and its
output bound to `prev`.

### Artifact Derivation

`artifact` declares that this run’s output derives from a prior run.
The provenance edge is recorded in the receipt under `derived_from`:

```fard
module pipeline_step2

artifact upstream: Run("sha256:1e4c15a...")

fn main() : Value
  let data = map_get(upstream, "processed")
  {result: data, stage: 2}
```

The receipt will contain `"derived_from": ["sha256:1e4c15a..."]`.

### Multi-File Source Imports

Import an external `.fard` file by path at runtime:

```fard
module my_program

import "lib/utils.fard" as utils

fn main() : Value
  utils.compute(42)
```

### Robustness

All runtime failure conditions return `V::Err` rather than crashing.
Programs can handle them with `match` or `?`:

|Condition                  |Error code                         |
|---------------------------|-----------------------------------|
|Division by zero           |`ERROR_DIV_ZERO`                   |
|Integer overflow           |`ERROR_OVERFLOW`                   |
|List out of bounds         |`ERROR_OOB`                        |
|Map missing key            |`ERROR_OOB`                        |
|Type mismatch on param     |`ERROR_TYPE`                       |
|Match exhaustion           |`ERROR_MATCH_NO_ARM`               |
|Recursion depth (limit 200)|`ERROR_EVAL_DEPTH`                 |
|Effect call failure        |OS error message                   |
|JSON parse failure         |`ERROR_BADARG json_parse`          |
|Bad base64                 |`ERROR_BADARG base64url_decode`    |
|Decryption failure         |`ERROR_EVAL xchacha20poly1305_open`|

## Standard Library

The standard library is written in pure FARD. Import with `import std.X as X`.

### std.list

```fard
import std.list as list

list.map(xs, f)          // apply f to each element
list.filter(xs, pred)    // keep elements where pred returns true
list.fold(xs, init, f)   // reduce left
list.range(start, end)   // [start, start+1, ..., end-1]
list.find(xs, pred)      // first matching element or unit
list.any(xs, pred)       // true if any element matches
list.all(xs, pred)       // true if all elements match
list.zip(xs, ys)         // list of [x, y] pairs
```

### std.result

```fard
import std.result as result

result.is_ok(v)              // true if ok(...)
result.is_err(v)             // true if err(...)
result.unwrap_or(v, default) // inner value or default
result.unwrap_err(v, default)// error message or default
result.map(v, f)             // apply f inside ok, pass err through
result.and_then(v, f)        // chain: f receives inner value of ok
```

### std.map

```fard
import std.map as map

map.from_list(pairs)      // [["k", v], ...] → map
map.map_vals(m, f)        // apply f to each value
map.filter_keys(m, pred)  // keep keys where pred returns true
map.merge(a, b)           // b's keys overwrite a's
```

## Builtin Functions

### Arithmetic

|Function   |Signature      |
|-----------|---------------|
|`add(a, b)`|Int × Int → Int|
|`sub(a, b)`|Int × Int → Int|
|`mul(a, b)`|Int × Int → Int|
|`div(a, b)`|Int × Int → Int|
|`rem(a, b)`|Int × Int → Int|
|`neg(a)`   |Int → Int      |

### Comparison

|Function  |Signature   |
|----------|------------|
|`eq(a, b)`|V × V → Bool|
|`lt(a, b)`|V × V → Bool|
|`gt(a, b)`|V × V → Bool|
|`le(a, b)`|V × V → Bool|
|`ge(a, b)`|V × V → Bool|
|`not(a)`  |Bool → Bool |

### Text

|Function                   |Signature                |
|---------------------------|-------------------------|
|`text_concat(a, b)`        |Text × Text → Text       |
|`text_len(s)`              |Text → Int               |
|`text_contains(s, sub)`    |Text × Text → Bool       |
|`text_starts_with(s, pre)` |Text × Text → Bool       |
|`text_split(s, sep)`       |Text × Text → List       |
|`text_trim(s)`             |Text → Text              |
|`text_slice(s, i, j)`      |Text × Int × Int → Text  |
|`text_replace(s, from, to)`|Text × Text × Text → Text|
|`text_join(xs, sep)`       |List × Text → Text       |
|`int_to_text(n)`           |Int → Text               |
|`int_parse(s)`             |Text → Int               |

### List

|Function              |Signature              |
|----------------------|-----------------------|
|`list_len(xs)`        |List → Int             |
|`list_get(xs, i)`     |List × Int → V         |
|`list_append(xs, v)`  |List × V → List        |
|`list_concat(xs, ys)` |List × List → List     |
|`list_reverse(xs)`    |List → List            |
|`list_contains(xs, v)`|List × V → Bool        |
|`list_slice(xs, i, j)`|List × Int × Int → List|

### Map

|Function          |Signature           |
|------------------|--------------------|
|`map_new()`       |→ Map               |
|`map_get(m, k)`   |Map × Text → V      |
|`map_set(m, k, v)`|Map × Text × V → Map|
|`map_has(m, k)`   |Map × Text → Bool   |
|`map_keys(m)`     |Map → List          |
|`map_delete(m, k)`|Map × Text → Map    |

### Bytes

|Function              |Signature                |
|----------------------|-------------------------|
|`bytes_len(b)`        |Bytes → Int              |
|`bytes_concat(a, b)`  |Bytes × Bytes → Bytes    |
|`bytes_slice(b, i, j)`|Bytes × Int × Int → Bytes|
|`bytes_eq(a, b)`      |Bytes × Bytes → Bool     |
|`bytes_from_text(s)`  |Text → Bytes             |

### Encode / Crypto

|Function                               |Signature           |
|---------------------------------------|--------------------|
|`base64url_encode(b)`                  |Bytes → Text        |
|`base64url_decode(s)`                  |Text → Bytes | Err  |
|`json_parse(s)`                        |Text → V | Err      |
|`json_emit(v)`                         |V → Text | Err      |
|`sha256(b)`                            |Bytes → Bytes       |
|`hkdf_sha256(ikm, salt, info, len)`    |Bytes⁴ → Bytes | Err|
|`xchacha20poly1305_seal(k, n, aad, pt)`|Bytes⁴ → Bytes | Err|
|`xchacha20poly1305_open(k, n, aad, ct)`|Bytes⁴ → Bytes | Err|

### Result

|Function|Signature |
|--------|----------|
|`ok(v)` |V → Ok(V) |
|`err(s)`|Text → Err|

## Determinism

Identical source + identical input + identical effects = identical RunID.

This holds across machines, across implementations, across time. The
RunID is a pure function of the program, its input, and its environment.
Change any of these, the RunID changes. Keep them identical, the RunID
is identical.

## Specifications

|Document                          |Contents                          |
|----------------------------------|----------------------------------|
|`spec/fard_spec_stack_v0_final.md`|Trust stack specification (frozen)|
|`spec/fardlang_grammar_v0.5.txt`  |Surface language grammar          |
|`spec/fard_formal_description.txt`|Formal description of FARD        |

## License

MUI