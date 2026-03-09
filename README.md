# FARD 1.0

FARD is a programming language with traceable execution. Every program
that runs produces a cryptographic witness proving what code ran, what
it received, and what it returned. The witness is a value, encoded as
canonical bytes, identified by its hash.

Traceability is not a feature. It is an invariant of every execution.

## Quick Start

Build everything:

```bash
cargo build
```

Write a program:

```
fn double(n) { n * 2 }
double(21)
```

Run it:

```bash
fardrun run --program main.fard --out ./out
cat out/result.json   # {"result":42}
```

## Language

### Values

```
42              // Int
3.14            // Float
true            // Bool
unit            // Unit (null equivalent)
"hello"         // Text
[1, 2, 3]       // List
{ x: 1, y: 2 }  // Record
```

### Functions

```
fn add(a, b) { a + b }
fn gcd(a, b) { match b == 0 { true => a, false => gcd(b, a % b) } }
```

### Let bindings and pipe

```
let x = 42
let msg = "the answer is ${x}"

fn reduce(r) {
  let g = gcd(r.num, r.den) | { num: r.num / g, den: r.den / g }
}
```

### Match

```
match n % 2 == 0 {
  true  => "even"
  false => "odd"
}
```

### User-defined types

```
a Point is { x: Int, y: Int }
a Shape is Circle(r: Int) or Rect(w: Int, h: Int)

let p = Point({ x: 3, y: 4 })  // missing fields → ERROR_TYPE at call site
let c = Circle({ r: 5 })
```

### String interpolation

```
let name = "world"
"hello ${name}, sum is ${1 + 2}"  // → "hello world, sum is 3"
```

### Imports

```
import("./src/rational") as rat          // local
import("std/math") as math               // standard library
import("pkg:greet@2026-03-08") as greet  // package registry
```

## CLI

### Run

```bash
fardrun run --program main.fard --out ./out
```

Writes `out/result.json` and `out/trace.ndjson`. On failure writes `out/error.json`.

### Test

Write test blocks in any `.fard` file:

```
fn gcd(a, b) { match b == 0 { true => a, false => gcd(b, a % b) } }

test "gcd of 12 and 8 is 4"    { gcd(12, 8) == 4 }
test "gcd of 100 and 75 is 25" { gcd(100, 75) == 25 }
```

```bash
fardrun test --program math_test.fard
#   ✓ gcd of 12 and 8 is 4
#   ✓ gcd of 100 and 75 is 25
#   2 passed

fardrun test --program math_test.fard --json  # structured JSON output
```

Exit code 0 on pass, 1 on any failure.

### REPL

```bash
fardrun repl
# fard> let x = 42
# fard> x * 2
# {"result":84}
# fard> :quit
```

### Publish a package

```bash
fardrun publish --package ./my-pkg --token <github-token>
# [fard publish] packaging greet@2026-03-08...
# [fard publish] sha256: c8671262...
# [fard publish] uploading greet@2026-03-08.tar.gz...
# [fard publish] updating registry.json...
# [fard publish] published greet@2026-03-08 ✓
```

Package layout requires a `fard.toml`:

```toml
name = "greet"
version = "2026-03-08"
entry = "main.fard"
```

### Format

```bash
fardfmt main.fard          # format in place
fardfmt --check main.fard  # exit 1 if not formatted (for CI)
fardfmt --stdin            # read from stdin, write to stdout
```

Formatting rules: 2-space indent inside blocks, one space around binary
operators, `{ k: v }` record spacing, trailing whitespace removed,
single trailing newline.

## Standard Library

|Module      |Functions                                                                                                          |
|------------|-------------------------------------------------------------------------------------------------------------------|
|`std/math`  |`abs min max pow sqrt floor ceil round log log2 pi e inf`                                                          |
|`std/str`   |`len trim split concat upper lower replace contains starts_with ends_with pad_left pad_right repeat index_of chars`|
|`std/int`   |`add sub mul div mod abs min max pow neg clamp parse to_text`                                                      |
|`std/list`  |`map filter reduce range repeat concat head tail len get contains sort`                                            |
|`std/result`|`ok err unwrap is_ok`                                                                                              |
|`std/option`|`some none is_some`                                                                                                |
|`std/null`  |`isNull coalesce guardNotNull`                                                                                     |
|`std/path`  |`base dir ext isAbs join joinAll normalize`                                                                        |
|`std/rec`   |`keys values has get set remove merge select update`                                                               |
|`std/json`  |`encode decode canonicalize`                                                                                       |
|`std/fs`    |`read_text write_text exists read_dir stat delete make_dir`                                                        |

## Error Format

All errors write `out/error.json`:

```json
{
  "code": "ERROR_TYPE",
  "message": "ERROR_TYPE Point: missing required field(s): x, y",
  "span": { "file": "main.fard", "line": 2, "col": 9 }
}
```

|Code                |Meaning                                         |
|--------------------|------------------------------------------------|
|`ERROR_PARSE`       |Syntax error                                    |
|`ERROR_RUNTIME`     |Runtime failure                                 |
|`ERROR_TYPE`        |Missing required fields at type constructor call|
|`ERROR_DIV_ZERO`    |Integer division by zero                        |
|`ERROR_NULL_GUARD`  |`guardNotNull` called on `unit`                 |
|`ERROR_PAT_MISMATCH`|Pattern match failed                            |
|`ERROR_UNBOUND`     |Reference to undefined variable                 |

## Determinism

Identical source + identical input + identical deps = identical result hash.
This holds across machines, across implementations, across time.

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
|`fardlang`   |Parser, canonical printer, evaluator                                |

## Specifications

|Document                          |Contents                            |
|----------------------------------|------------------------------------|
|`spec/fard_spec_stack_v0_final.md`|Trust stack specification (frozen)  |
|`spec/fardlang_grammar_v0.5.txt`  |Surface language grammar            |
|`FARD_1.0_ROADMAP.md`             |Feature roadmap and design decisions|

## License

MUI