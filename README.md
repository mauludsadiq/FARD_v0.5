# FARD

FARD is a deterministic, content-addressed scripting language where every execution produces a cryptographic receipt.

Each run emits a SHA-256 digest committing to the source code, imported modules, inputs, intermediate computation steps, and final result.

Two executions of the same program on the same inputs produce the same digest.

Traceability is not a feature — it is an invariant of execution.

```
fard_run_digest=sha256:4dda9ce7d4dcfe7ddc5eda2f80d78bbf81c188e...
```

**Version:** v1.6.0 — [Releases](https://github.com/mauludsadiq/FARD/releases)

-----

## What FARD Is

FARD is a deterministic scripting language with a functional core, controlled mutable state, and a content-addressed execution runtime.

It provides:

- cryptographic witness receipts on every run
- 53 standard library modules
- 242 built-in primitives
- an 11-binary toolchain
- native FFI via dynamic library loading
- a WebAssembly compilation target
- an LSP server with VS Code extension
- a SQLite-backed receipt registry
- a content-addressed package manager

FARD turns program execution itself into a cryptographic artifact.

Programs written in FARD do not merely return values — they return values together with verifiable evidence of how those values were computed. Every receipt binds the result to the exact source, imports, inputs, and execution history that produced it. Any execution can therefore be independently verified on another machine.

-----

## Install

```bash
# macOS (Apple Silicon)
curl -L https://github.com/mauludsadiq/FARD/releases/latest/download/fard-macos-aarch64.tar.gz | tar xz
sudo mv fard-macos-aarch64/fard* /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/mauludsadiq/FARD/releases/latest/download/fard-macos-x86_64.tar.gz | tar xz
sudo mv fard-macos-x86_64/fard* /usr/local/bin/

# Linux
curl -L https://github.com/mauludsadiq/FARD/releases/latest/download/fard-linux-x86_64.tar.gz | tar xz
sudo mv fard-linux-x86_64/fard* /usr/local/bin/
```

Or build from source:

```bash
git clone https://github.com/mauludsadiq/FARD.git
cd FARD
cargo build --release
```

-----

## Quick Start

```bash
# Create a new project
fardrun new my-project
cd my-project

# Run it
fardrun run --program main.fard --out ./out
cat out/result.json   # {"result":"Hello, world!"}
```

Or write a program directly:

```
fn double(n) { n * 2 }
double(21)
```

```bash
fardrun run --program main.fard --out ./out
cat out/result.json   # {"result":42}
```

-----

## Language

### Values

```
42              // Int   (64-bit signed)
3.14            // Float (64-bit IEEE 754)
true            // Bool
null            // Unit
"hello"         // Text
[1, 2, 3]       // List
{ x: 1, y: 2 }  // Record
```

String interpolation:

```
let name = "world"
"hello ${name}"
```

### Functions

Functions are first-class. Closures capture their lexical environment.

```
fn add(a, b) { a + b }
fn apply(f, x) { f(x) }

let double = fn(x) { x * 2 }
apply(double, 21)   // -> 42

fn make_adder(n) { fn(x) { x + n } }
let add5 = make_adder(5)
add5(10)   // -> 15
```

Named arguments:

```
fn greet(name, greeting) { str.concat(greeting, str.concat(" ", name)) }
greet(name: "Alice", greeting: "Hello")   // -> "Hello Alice"
```

### Let Bindings

```
let x = 42
let y = x * 2
let result = let x = 10 in let y = 20 in x + y
```

### Conditionals and Pattern Matching

```
if x > 0 then "positive" else "non-positive"

match type.of(x) {
  "int"  => str.from_int(x),
  "text" => x,
  _      => "other"
}
```

### Early Return

```
fn clamp(x, lo, hi) {
  let _ = if x < lo then { return lo } else null in
  let _ = if x > hi then { return hi } else null in
  x
}
```

### Recursion

Tail-recursive functions are optimised. No stack overflow on large inputs.

```
fn factorial(n) { if n <= 1 then 1 else n * factorial(n - 1) }
fn sum_to(n, acc) { if n == 0 then acc else sum_to(n - 1, acc + n) }
sum_to(1000000, 0)
```

### While (Hash-Chained Iteration)

`while` is not mutable looping. It is a hash-chained ledger of every state transition, producing a cryptographic certificate of the entire computation.

```
let result = while {n: 0, acc: 0}
  fn(s) { s.n < 10 }
  fn(s) { {n: s.n + 1, acc: s.acc + s.n} }

result.value      // {n: 10, acc: 45}
result.steps      // all intermediate states
result.chain_hex  // sha256 of the full computation history
```

### Mutable Cells

FARD has a functional core with a controlled mutable escape hatch via `std/cell`:

```
import("std/cell") as cell

let counter = cell.new(0)
let _       = cell.set(counter, cell.get(counter) + 1)
cell.get(counter)   // -> 1
```

### Imports

```
import("std/math")   as math
import("std/list")   as list
import("./mylib")    as mylib
import("pkg:greet")  as greet
```

-----

## Standard Library (53 Modules)

### Core Data

**std/str** — `len`, `concat`, `join`, `split`, `slice`, `upper`, `lower`, `trim`,
`contains`, `starts_with`, `ends_with`, `pad_left`, `pad_right`, `repeat`,
`index_of`, `chars`, `replace`, `from_int`, `from_float`

**std/list** — `map`, `filter`, `fold`, `any`, `all`, `find`, `find_index`,
`flat_map`, `take`, `drop`, `zip_with`, `chunk`, `sort_by`, `par_map`,
`len`, `range`, `reverse`, `concat`, `group_by`

**std/map** — `new`, `get`, `set`, `has`, `delete`, `keys`, `values`, `entries`

**std/set** — `new`, `add`, `remove`, `has`, `union`, `intersect`, `diff`,
`to_list`, `from_list`, `size`

**std/rec** / **std/record** — `get`, `set`, `has`, `keys`, `merge`, `delete`

**std/option** — `some`, `none`, `is_some`, `is_none`, `unwrap`, `unwrap_or`,
`map`, `and_then`, `from_nullable`, `to_nullable`

**std/result** — `ok`, `err`, `is_ok`, `is_err`, `unwrap`, `unwrap_or`,
`map`, `map_err`, `and_then`, `or_else`

**std/type** — `of`, `is_int`, `is_float`, `is_bool`, `is_text`, `is_list`,
`is_record`, `is_null`, `is_fn`

**std/cast** — `int`, `float`, `text`

**std/null** — `is_null`, `coalesce`

### Numbers

**std/math** — `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `log`,
`log2`, `log10`, `sqrt`, `pow`, `abs`, `floor`, `ceil`, `round`, `pi`, `e`

**std/float** — `add`, `sub`, `mul`, `div`, `sqrt`, `abs`, `ln`, `pow`, `neg`,
`le`, `gt`, `from_int`, `to_text`, `to_str_fixed`, `is_nan`, `is_inf`

**std/int** — `to_str_padded`

**std/bigint** — Arbitrary-precision integers via `num-bigint`:
`from_int`, `from_str`, `to_str`, `add`, `sub`, `mul`, `div`, `mod`, `pow`, `eq`, `lt`, `gt`

**std/bits** — Bitwise with correct two’s complement on negatives:
`band`, `bor`, `bxor`, `bnot`, `bshl`, `bshr`

**std/linalg** — Matrix and vector operations:
`zeros`, `eye`, `dot`, `norm`, `vec_add`, `vec_sub`, `vec_scale`, `transpose`

**std/uuid** — `v4`, `validate`

### Text and Encoding

**std/re** — `is_match`, `find`, `find_all`, `split`, `replace`

**std/json** — `encode`, `decode`, `canonicalize`

**std/base64** — `encode`, `decode`

**std/codec** — `base64url_encode`, `base64url_decode`, `hex_encode`, `hex_decode`

**std/csv** — `parse`, `encode`

**std/bytes** — raw byte manipulation

### Cryptography and Hashing

**std/hash** — `sha256_bytes`, `sha256_text`

**std/crypto** — `hmac_sha256`, `aes_encrypt`, `aes_decrypt`, `pbkdf2`,
`ed25519_sign`, `ed25519_verify`

### I/O and System

**std/io** — `read_file`, `write_file`, `append_file`, `read_lines`,
`read_stdin`, `read_stdin_lines`, `file_exists`, `delete_file`,
`list_dir`, `make_dir`

**std/fs** — `read`, `write`, `exists`, `stat`, `list`

**std/path** — `join`, `base`, `dir`, `ext`, `isAbs`, `normalize`

**std/env** — `get`, `args`

**std/process** — `spawn(cmd, args, stdin)`, `exit`

**std/http** — `get`, `post`, `request`

**std/net** — `serve(port, handler_fn)` — blocking HTTP server

### Time

**std/datetime** — `now`, `format`, `parse`, `add`, `diff`, `field`

**std/time** — `now_ms`, `sleep_ms`

### Concurrency

**std/promise** — `spawn`, `await`

**std/chan** — `new`, `send`, `recv`, `try_recv`, `close`

**std/mutex** — `new`, `lock`, `unlock`, `with_lock`

**std/cell** — `new`, `get`, `set` (controlled mutable state)

### Compression

**std/compress** — `gzip_compress`, `gzip_decompress`, `zstd_compress`, `zstd_decompress`

### Metaprogramming

**std/eval** — `eval(source_text)` — evaluate a string as FARD

**std/ast** — `parse(source_text)` — parse FARD source into AST node records

### Tracing and Observability

**std/trace** — `info`, `warn`, `error`, `span` — structured logging into the execution trace

**std/witness** — `verify(run_id)`, `verify_chain(run_id)`, `self_digest()`

**std/artifact** — bind a prior verified run by RunID into the current program

### Interoperability

**std/ffi** — `load`, `open`, `call`, `call_pure`, `call_str`, `close` — call C dynamic libraries

**std/png** — `red_1x1` — PNG image generation

**std/cli** — command-line argument parsing

### Domain-Specific

**std/graph** — `new`, `add_node`, `add_edge`, `bfs`, `dfs`, `shortest_path`, `topo_sort`

**std/sembit** — semantic bitfield partitioning

**std/grow** — `append`, `merge`, `unfold`, `unfold_tree`

**std/flow** — `id`, `pipe`, `tap` — function composition combinators

-----

## Package Manager

Packages are versioned, SHA-256 verified, and cached locally.

Add dependencies to `fard.toml`:

```toml
name = "my-project"
version = "1.0.0"
entry = "main.fard"

[deps]
greet = "greet@1.6.0"
```

Install:

```bash
fardrun install --manifest fard.toml
```

Use in code:

```
import("pkg:greet") as greet
greet.hello("world")   // -> "Hello, world!"
```

Publish a package:

```bash
fardrun publish --package ./my-pkg --token <github-token>
```

Registry: `https://github.com/mauludsadiq/FARD/releases/latest/download/registry.json`

-----

## Concurrency

```
import("std/promise") as promise
import("std/chan")     as chan
import("std/list")    as list

// Parallel execution
let p1 = promise.spawn(fn() { expensive_a() })
let p2 = promise.spawn(fn() { expensive_b() })
let a  = promise.await(p1)
let b  = promise.await(p2)
a + b

// Parallel map
list.par_map([1, 2, 3, 4, 5], fn(x) { x * x })

// Channels
let c = chan.new()
chan.send(c, 42)
chan.recv(c)   // -> {t: "some", v: 42}
```

-----

## Metaprogramming

```
import("std/eval") as e
e.eval("fn double(n) { n * 2 }\ndouble(21)")   // -> 42
```

```
import("std/ast") as ast
let nodes = ast.parse("1 + 2")
nodes[0].t    // -> "bin"
nodes[0].op   // -> "+"
```

-----

## Cryptographic Witnessing

### Self-Digest

A program can know its own content hash at runtime:

```
import("std/witness") as w
w.self_digest()   // -> "sha256:e60cb9e82ac28f..."
```

This is a fixed-point: the digest is stable across iterations.

### Artifact Binding

Link to a prior verified run by RunID:

```
artifact step1 = "sha256:689dede5..."
step1.output   // the result of that prior run
```

### Chain Verification

```
import("std/witness") as w
let r = w.verify_chain("sha256:47912fef...")
r.t      // "ok"
r.depth  // depth of the verified chain
```

### Distributed Verification

Set `FARD_REGISTRY_URL` to fetch receipts from a remote registry when not found locally:

```bash
export FARD_REGISTRY_URL=http://registry.example.com:7370
fardrun run --program main.fard --out ./out
```

-----

## FFI

Call native C libraries directly from FARD:

```
import("std/ffi") as ffi

let lib    = ffi.load("/usr/lib/libm.dylib")
let result = ffi.call(lib.ok, "abs", [-42])
result.ok   // -> 42

// call_pure: result included in witness hash chain
let r2 = ffi.call_pure(lib.ok, "abs", [-7])

// call_str: C function returning char*
let r3 = ffi.call_str(lib.ok, "some_fn", ["arg"])
```

Type mapping: `Int` → `i64`, `Float` → `f64`, `Text` → `char*`, `Bool` → `0/1`

-----

## WebAssembly

Compile pure FARD functions to WebAssembly:

```bash
fardwasm main.fard --out main.wat              # WebAssembly Text
fardwasm main.fard --target wasi --out main.wasm   # Binary WASM
```

Supports integers, arithmetic, booleans, let bindings, if/then/else, and recursion.

```bash
wat2wasm main.wat -o main.wasm
wasm-interp main.wasm -r factorial -a 'i64:10'
# factorial(i64:10) => i64:3628800
```

-----

## CLI

### fardrun new

```bash
fardrun new my-project                    # minimal template
fardrun new my-server --template server   # HTTP server
fardrun new my-pipeline --template ci     # CI runner
```

### fardrun run

```bash
fardrun run --program main.fard --out ./out
```

Output directory contains `result.json`, `error.json`, `trace.ndjson`, `module_graph.json`, `digests.json`.

### fardrun test

```
fn gcd(a, b) { if b == 0 then a else gcd(b, a % b) }

test "basic"       { gcd(12, 8) == 4 }
test "commutative" { gcd(8, 12) == gcd(12, 8) }
```

```bash
fardrun test --program math.fard
#   v basic
#   v commutative
#   2 passed
```

### fardrun repl

```bash
fardrun repl
# FARD v1.6.0 REPL
# fard> let x = 42
# fard> x * 2
# 84
```

History persists to `~/.fard_history`. Up/Down arrow navigation. Ctrl-D to exit.

### fardfmt

```bash
fardfmt main.fard            # format in place
fardfmt --check main.fard    # exit 1 if not formatted
fardfmt --stdin              # read from stdin
```

### fardcheck

```bash
fardcheck main.fard
# ok -- 47 items checked, 0 errors
```

Hindley-Milner style best-effort type checker. Dynamic values propagate as `Dynamic` without false positives.

### fardwasm

```bash
fardwasm main.fard --out main.wat
fardwasm main.fard --target wasi --out main.wasm
```

### fardregistry

```bash
fardregistry --port 7370 --db receipts.db --seed receipts/
```

Routes: `GET /health`, `GET /stats`, `GET /receipt/<id>`, `GET /verify/<id>`,
`GET /packages`, `GET /packages/<n>`, `POST /publish`, `POST /packages/publish`

### fardlock

```bash
fardlock gen-toml --manifest fard.toml --out fard.lock.json
fardrun run --program main.fard --lockfile fard.lock.json --enforce-lockfile
```

### fardbundle

```bash
fardbundle build  --root . --entry main.fard --out ./bundle
fardbundle verify --bundle bundle/bundle.json --out ./out
fardbundle run    --bundle bundle/bundle.json --out ./out
```

### fardverify

```bash
fardverify trace    --out ./out
fardverify artifact --out ./out
fardverify bundle   --out ./out
```

-----

## VS Code Extension

```bash
code --install-extension editors/vscode/fard-language-0.1.0.vsix
```

Or via VS Code: `Cmd+Shift+P` → `Extensions: Install from VSIX`.

Features: syntax highlighting, inline diagnostics, dot-completion for all 53 stdlib modules, hover documentation, import path completion.

Configure the Language Server path:

```json
{ "fard.lspPath": "/usr/local/bin/fard-lsp" }
```

-----

## Binaries

|Binary        |Purpose                                                    |
|--------------|-----------------------------------------------------------|
|`fardrun`     |Runtime: `run`, `test`, `repl`, `new`, `install`, `publish`|
|`fardfmt`     |Canonical formatter                                        |
|`fardcheck`   |HM-style type checker                                      |
|`fardwasm`    |FARD to WAT/WASM compiler                                  |
|`fardregistry`|SQLite-backed receipt registry server                      |
|`fardlock`    |Lockfile generation and enforcement                        |
|`fardbundle`  |Bundle build, verify, and run                              |
|`fardverify`  |Trace, artifact, and bundle verification                   |
|`fardpkg`     |Package management                                         |
|`fard-lsp`    |Language Server Protocol (diagnostics, hover, completion)  |
|`fardc`       |Compiler frontend and canonicalizer                        |

-----

## Crates

|Crate        |Purpose                                     |
|-------------|--------------------------------------------|
|`valuecore`  |Canonical value encoding, SHA-256, JSON     |
|`witnesscore`|Witness construction and identity projection|
|`abirunner`  |Pure function from bundle to witness bytes  |
|`registry`   |Content-addressed filesystem receipt storage|
|`fardc`      |Compiler frontend with canonicalization     |
|`fardlang`   |Parser, type checker, evaluator             |
|`fardcli`    |CLI entry point                             |
|`fard-lsp`   |LSP server (tower-lsp, tokio)               |

-----

## Examples

The examples below show that FARD is not limited to witness generation — it already supports service hosting, CI automation, database-backed workflows, and proof-oriented systems.

### fard-fmt-server

HTTP service that formats FARD source code. Written entirely in FARD.

```bash
fardrun run --program examples/fard-fmt-server/main.fard --out ./out &
curl -X POST http://localhost:8080/fmt -d 'fn add(a,b){a+b}'
# fn add(a, b) { a + b }
curl -X POST http://localhost:8080/fmt/check -d 'fn add(a,b){a+b}'
# {"changed":true,"ok":true}
```

### fard-ci

CI pipeline runner. Reads a `pipeline.json` spec, runs each step via `fardrun`, verifies witnesses.

```bash
cd examples/fard-ci
fardrun run --program main.fard --out ./out
# {"result":{"ok":true,"passed":3,"summary":"3/3 passed -- fard-ci-selftest"}}
```

### fard-db

SQLite key-value store via native FFI. Every operation produces a witness receipt.

```bash
cd examples/fard-db/native && cargo build --release
fardrun run --program examples/fard-db/main.fard --out ./out
# {"result":{"name":"FARD","count_before":3,"count_after":2,...}}
```

### collapse_stack

Cryptographic Z-state machine with hash-chained delta application. A formal proof assistant written in FARD — apply theorems as deltas to a verified state, each step producing a new content-addressed receipt.

### mathematical_proof_system

GCD, LCM, extended Euclidean algorithm, primality testing, and modular arithmetic in a typed FARD module.

### qasim_safety

Numerical safety verification using `std/linalg` and `std/float` for matrix operations and SHA-256 chain proofs.

-----

## Error Messages

FARD provides actionable error messages with suggestions:

```
Error: unbound var: str -- did you forget to import? Try: import("std/str") as str
Error: no member 'mpa' -- did you mean 'map'?
Error: arity mismatch: expected 2 args, got 3
```

Error output format:

```json
{
  "code": "ERROR_RUNTIME",
  "message": "list index 5 out of bounds (len 3)",
  "span": { "file": "main.fard", "line": 7, "col": 3 }
}
```

|Code                |Meaning                          |
|--------------------|---------------------------------|
|`ERROR_PARSE`       |Syntax error                     |
|`ERROR_RUNTIME`     |Runtime failure                  |
|`ERROR_DIV_ZERO`    |Division by zero                 |
|`ERROR_PAT_MISMATCH`|Pattern match failed             |
|`ERROR_ARITY`       |Wrong number of arguments        |
|`ERROR_BADARG`      |Wrong argument type for a builtin|
|`ERROR_IO`          |File or network I/O failure      |
|`ERROR_LOCK`        |Lockfile enforcement failure     |
|`ERROR_FFI`         |Foreign function interface error |

-----

## Determinism

Given identical source code, imports, and inputs, FARD guarantees identical results, identical execution traces, and identical execution digests.

This invariant holds across machines, operating systems, and time.

Operations that depend on external state are explicitly marked as oracle boundaries, including `std/http`, `std/datetime.now`, `std/io.read_stdin`, `std/uuid.v4`, and `std/ffi.call`. Their observed values are recorded in the execution trace so runs remain auditable even when interacting with the outside world.

`std/ffi.call_pure` declares a foreign call deterministic and includes its result in the witness hash chain.

-----

## Architecture

```
Layer 5  Execution ABI v0        bundle -> witness bytes
Layer 4  Registry Semantics v0   content-addressed receipt storage
Layer 3  Composition Semantics   executions link by verified RunID
Layer 2  Artifact Semantics      same (program, input, deps) -> same RunID
Layer 1  Value Core v0           same value -> same bytes -> same hash
```

The entire system reduces to one primitive:

```
CID(bytes) = "sha256:" || hex(SHA256(bytes))
```

-----

## Self-Verifying

313 tests across 36 files, all written in pure FARD:

```bash
for f in tests/test_*.fard; do fardrun test --program "$f"; done
```

The spec and release announcements are generated by FARD programs:

```bash
fardrun run --program tools/gen_spec.fard --out ./out
fardrun run --program tools/gen_announcement.fard --out ./out
```

-----

## Specifications

|Document                          |Contents                          |
|----------------------------------|----------------------------------|
|`spec/fard_spec_stack_v0_final.md`|Trust stack specification (frozen)|
|`spec/fardlang_grammar_v0.5.txt`  |Surface language grammar          |
|`SPEC.md`                         |Stdlib surface spec (generated)   |
|`ANNOUNCEMENT.md`                 |Release announcement (generated)  |

-----

## License

MUI