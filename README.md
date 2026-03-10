# FARD 1.0.0

FARD is a pure, deterministic, content-addressed scripting language.
Every program run produces a cryptographic digest committing to all
inputs, outputs, and intermediate computation steps. Traceability is
not a feature — it is an invariant of every execution.

```
fard_run_digest=sha256:4dda9ce7d4dcfe7ddc5eda2f80d78bbf81c188e...
```

This digest is printed to stdout on every run. It commits to the source,
all imports, all inputs, and the full execution trace. Two runs of the
same program on the same input always produce the same digest.

-----

## Quick Start

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

-----

## Language

### Values

FARD has seven value types:

```
42              // Int   (64-bit signed)
3.14            // Float (64-bit IEEE 754)
true            // Bool
null            // Unit (the only unit value)
"hello"         // Text
[1, 2, 3]       // List
{ x: 1, y: 2 }  // Record
```

Multiline text uses backtick delimiters:

```
let msg = `line one
line two
line three`
```

String interpolation:

```
let name = "world"
let n = 42
"hello ${name}, answer is ${n}"   // → "hello world, answer is 42"
```

### Functions

Functions are first-class values. They can be passed as arguments,
returned from other functions, and stored in records and lists.

```
fn add(a, b) { a + b }
fn double(n) { n * 2 }
fn apply(f, x) { f(x) }

apply(double, 21)   // → 42
```

Anonymous functions (lambdas):

```
let square = fn(x) { x * x }
square(5)   // → 25
```

Higher-order functions:

```
import("std/list") as list

let nums = [1, 2, 3, 4, 5]
let doubled = list.map(nums, fn(x) { x * 2 })       // [2, 4, 6, 8, 10]
let evens   = list.filter(nums, fn(x) { x % 2 == 0 }) // [2, 4]
let sum     = list.fold(nums, 0, fn(acc, x) { acc + x }) // 15
```

Closures capture their lexical environment:

```
fn make_adder(n) {
  fn(x) { x + n }
}

let add5 = make_adder(5)
add5(10)   // → 15
add5(20)   // → 25
```

### Named / Keyword Arguments

Functions can be called with named arguments in any order:

```
fn greet(name, greeting) {
  str.concat(greeting, str.concat(" ", name))
}

greet(name: "Alice", greeting: "Hello")   // → "Hello Alice"
greet(greeting: "Hi", name: "Bob")        // → "Hi Bob"
```

### Let Bindings

```
let x = 42
let y = x * 2
let msg = "the answer is ${y}"
```

Let-in expressions (inline):

```
let result = let x = 10 in let y = 20 in x + y
```

### Conditionals

```
if x > 0 then "positive" else "non-positive"
```

### Early Return

```
fn clamp(x, lo, hi) {
  let _ = if x < lo then { return lo } else null in
  let _ = if x > hi then { return hi } else null in
  x
}

clamp(5, 0, 10)   // → 5
clamp(-1, 0, 10)  // → 0
clamp(99, 0, 10)  // → 10
```

### Match

Match on any value. Arms are comma-separated:

```
match n % 2 == 0 {
  true  => "even",
  false => "odd"
}
```

Match on type:

```
import("std/type") as type

match type.of(x) {
  "int"  => str.concat("int: ", str.from_int(x)),
  "text" => str.concat("text: ", x),
  _      => "other"
}
```

### Recursion

Tail-recursive functions are optimised:

```
fn factorial(n) {
  if n <= 1 then 1 else n * factorial(n - 1)
}

fn sum_to(n, acc) {
  if n == 0 then acc else sum_to(n - 1, acc + n)
}

sum_to(1000000, 0)   // tail-recursive, no stack overflow
```

### While (Hash-Chained Computation)

The `while` construct is FARD’s certified iteration primitive. It is not
mutable looping — it is a hash-chained ledger of every state transition.
Every step is hashed into a running chain, producing a cryptographic
certificate of the entire computation.

```
let result = while {n: 0, acc: 0}
  fn(s) { s.n < 10 }
  fn(s) { {n: s.n + 1, acc: s.acc + s.n} }
```

`result` is a record with three fields:

```
result.value      // final state: {n: 10, acc: 45}
result.steps      // list of all intermediate states
result.chain_hex  // sha256 digest chaining every step
```

The `chain_hex` uniquely identifies the entire computation history,
not just the final result.

### Records

Records are structural. Field access uses dot notation:

```
let p = { x: 3, y: 4 }
p.x   // → 3
p.y   // → 4
```

Records can be passed to functions expecting specific fields:

```
fn distance(p) {
  import("std/math") as math
  math.sqrt(p.x * p.x + p.y * p.y)
}

distance({ x: 3, y: 4 })   // → 5.0
```

There is no static record type system. Field access on a missing field
raises `ERROR_RUNTIME` at runtime. If you need validated construction,
use a constructor function:

```
fn make_point(x, y) {
  if type.of(x) != "int" then { return {err: "x must be int"} } else null in
  { x: x, y: y }
}
```

### Imports

```
import("std/math") as math      // standard library module
import("std/str")  as str
import("./mylib")  as mylib     // local .fard file (same directory)
```

Modules are records. All exported names are fields on the imported record.

-----

## Metaprogramming

### eval

`std/eval` evaluates a string as a FARD program and returns its result:

```
import("std/eval") as e

e.eval("1 + 2 + 3")   // → 6
e.eval("fn double(n) { n * 2 }\ndouble(21)")   // → 42
```

### AST Access

`std/ast` parses a source string into AST records you can inspect and
transform:

```
import("std/ast") as ast

let nodes = ast.parse("1 + 2")
nodes[0].t    // → "bin"
nodes[0].op   // → "+"
nodes[0].l.v  // → 1
nodes[0].r.v  // → 2
```

AST node types: `"int"`, `"float"`, `"bool"`, `"str"`, `"null"`,
`"var"`, `"bin"`, `"call"`, `"if"`, `"let"`, `"list"`, `"fn"`, `"other"`.

### Macros

Combine `std/ast`, `std/eval`, and `std/str` to implement macros —
source-level transformations that run at program time:

```
import("std/eval") as e
import("std/str")  as str
import("std/list") as list

let defmacro = fn(template, bindings) {
  let code = list.fold(bindings, template, fn(acc, pair) {
    str.replace(acc, pair[0], pair[1])
  }) in
  e.eval(code)
}

// Generate a multiplier function at runtime
let mul3 = defmacro("fn(x) { x * _N_ }", [["_N_", "3"]])
mul3(7)   // → 21

// Conditional macro
defmacro("if _COND_ then _T_ else _F_", [
  ["_COND_", "true"],
  ["_T_", "42"],
  ["_F_", "0"]
])  // → 42
```

-----

## Concurrency

### Promises (async/await)

`std/promise` spawns a function on an OS thread and returns a handle.
`promise.await` blocks until the result is ready:

```
import("std/promise") as promise

let p1 = promise.spawn(fn() { expensive_computation_a() })
let p2 = promise.spawn(fn() { expensive_computation_b() })

let a = promise.await(p1)
let b = promise.await(p2)
a + b
```

### Channels

```
import("std/chan") as chan

let c = chan.new()
chan.send(c, 42)
let v = chan.recv(c)   // → {t: "some", v: 42}
```

### Mutexes

```
import("std/mutex") as mutex

let m = mutex.new(0)
mutex.with_lock(m, fn(n) { n + 1 })   // atomically increment
```

### Parallel Map

```
import("std/list") as list

list.par_map([1, 2, 3, 4, 5], fn(x) { x * x })
// → [1, 4, 9, 16, 25]  (computed in parallel)
```

-----

## Standard Library (24 modules)

### std/str

`len`, `concat`, `join`, `split`, `slice`, `upper`, `lower`, `trim`,
`contains`, `starts_with`, `ends_with`, `pad_left`, `pad_right`,
`repeat`, `index_of`, `chars`, `replace`, `from_int`, `from_float`

### std/list

`map`, `filter`, `fold`, `any`, `all`, `find`, `find_index`,
`flat_map`, `take`, `drop`, `zip_with`, `chunk`, `sort_by`,
`par_map`, `len`, `range`, `reverse`, `concat`, `group_by`

### std/math

`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`log`, `log2`, `log10`, `sqrt`, `pow`, `abs`,
`floor`, `ceil`, `round`, `pi`, `e`

### std/float

`to_str_fixed(f, decimals)`, `is_nan(f)`, `is_inf(f)`

### std/int

`to_str_padded(n, width, pad_char)`

### std/bigint

Arbitrary-precision integers backed by `num-bigint`:
`from_int`, `from_str`, `to_str`, `add`, `sub`, `mul`, `div`, `mod`, `pow`,
`eq`, `lt`, `gt`

```
import("std/bigint") as big

let n = big.pow(big.from_int(2), 64)
big.to_str(n)   // → "18446744073709551616"
```

### std/bits

Bitwise operations with correct two’s complement semantics on negatives:
`band`, `bor`, `bxor`, `bnot`, `bshl`, `bshr`

### std/map

Hash map: `new`, `get`, `set`, `has`, `delete`, `keys`, `values`, `entries`

### std/set

`new`, `add`, `remove`, `has`, `union`, `intersect`, `diff`,
`to_list`, `from_list`, `size`

### std/re

Regular expressions:
`is_match(pattern, text)`, `find`, `find_all`, `split`, `replace`

### std/json

`encode(val)` → text, `decode(text)` → val, `canonicalize(text)` → text

### std/hash

`sha256_bytes(bytes)` → digest record, `sha256_text(text)` → digest record

### std/base64

`encode(text)` → text, `decode(text)` → text

### std/csv

`parse(text)` → list of lists, `encode(rows)` → text

### std/uuid

`v4()` → 36-char UUID string, `validate(s)` → bool

### std/datetime

`now()` → unix timestamp int, `format(ts, fmt)` → text,
`parse(text, fmt)` → int, `add(ts, unit, n)` → int,
`diff(a, b)` → int, `field(ts, field)` → int

Units: `"seconds"`, `"minutes"`, `"hours"`, `"days"`
Fields: `"year"`, `"month"`, `"day"`, `"hour"`, `"minute"`, `"second"`

### std/path

`join(a, b)`, `base(p)`, `dir(p)`, `ext(p)`, `isAbs(p)`, `normalize(p)`

### std/io

`read_file(path)` → `{ok: text}`, `write_file(path, content)` → `{ok: null}`,
`append_file(path, content)`, `read_lines(path)` → list,
`read_stdin()` → text, `read_stdin_lines()` → list,
`file_exists(path)` → bool, `delete_file(path)`, `list_dir(path)` → list,
`make_dir(path)` → bool

### std/http

`get(url)`, `post(url, body)`, `request(rec)` → `{status, body, headers}`

### std/promise

`spawn(fn)` → promise handle, `await(handle)` → value

### std/chan

`new()`, `send(c, v)` → bool, `recv(c)` → `{t:"some",v:x}` or null,
`try_recv(c)`, `close(c)` → bool

### std/mutex

`new(init)`, `lock(m)` → val, `unlock(m, val)` → bool,
`with_lock(m, fn)` → result

### std/ast

`parse(source_text)` → list of AST node records

### std/eval

`eval(source_text)` → value

-----

## CLI

### Run

```bash
fardrun run --program main.fard --out ./out
# fard_run_digest=sha256:...
```

Output directory contains:

- `result.json` — `{"result": <value>}` on success
- `error.json` — `{"code": "...", "message": "..."}` on failure
- `trace.ndjson` — newline-delimited JSON execution trace
- `module_graph.json` — import graph with content digests
- `digests.json` — sha256 of every output file

### Test

Write `test` blocks anywhere in a `.fard` file:

```
fn gcd(a, b) { if b == 0 then a else gcd(b, a % b) }

test "gcd basic"       { gcd(12, 8) == 4 }
test "gcd commutative" { gcd(8, 12) == gcd(12, 8) }
test "gcd with 1"      { gcd(7, 1) == 1 }
```

```bash
fardrun test --program math_test.fard
#   ✓ gcd basic
#   ✓ gcd commutative
#   ✓ gcd with 1
#   3 passed

fardrun test --program math_test.fard --json   # structured JSON output
```

Exit code 0 if all pass, 1 if any fail.

### REPL

```bash
fardrun repl
# fard> let x = 42
# fard> x * 2
# {"result":84}
# fard> import("std/math") as math
# fard> math.sqrt(2.0)
# {"result":1.4142135623730951}
# fard> :quit
```

### Format

```bash
fardfmt main.fard            # format in place
fardfmt --check main.fard    # exit 1 if not formatted (CI gate)
fardfmt --stdin              # read from stdin, write to stdout
```

Formatting rules: 2-space indent inside `fn` bodies, one space around
binary operators, `{ k: v }` record spacing, trailing whitespace removed,
single trailing newline.

### Publish a package

```bash
fardrun publish --package ./my-pkg --token <github-token>
```

Package layout requires a `fard.toml`:

```toml
name = "greet"
version = "2026-03-08"
entry = "main.fard"
```

Packages are date-versioned, content-addressed, and hosted as tarballs
with a `registry.json` index. No central registry authority.

-----

## Error Format

All runtime errors write `out/error.json` and exit non-zero:

```json
{
  "code": "ERROR_RUNTIME",
  "message": "list index 5 out of bounds (len 3)",
  "span": { "file": "main.fard", "line": 7, "col": 3 }
}
```

|Code                |Meaning                                                   |
|--------------------|----------------------------------------------------------|
|`ERROR_PARSE`       |Syntax error — program could not be parsed                |
|`ERROR_RUNTIME`     |Runtime failure (index out of bounds, field missing, etc.)|
|`ERROR_DIV_ZERO`    |Integer or float division by zero                         |
|`ERROR_PAT_MISMATCH`|Pattern match failed (let binding destructure failed)     |
|`ERROR_ARITY`       |Wrong number of arguments to a function                   |
|`ERROR_BADARG`      |Wrong argument type for a builtin                         |
|`ERROR_IO`          |File or network I/O failure                               |
|`ERROR_LOCK`        |Lockfile enforcement failure                              |

-----

## Determinism

Given identical source + identical imports + identical inputs:

- The result is always identical
- The trace is always identical
- The digest is always identical

This holds across machines, OS versions, and time. FARD has no hidden
sources of non-determinism (no implicit timestamps, no random seeds,
no unspecified hash ordering). The only non-deterministic primitives
are explicitly marked as oracle boundaries: `std/http`, `std/datetime.now`,
`std/io.read_stdin`, `std/uuid.v4`. These are recorded in the trace so
their values are auditable.

-----

## Self-Verifying

FARD v1.0.0 is self-verifying in three ways:

**The test suite runs in pure FARD.** 281 tests across 28 files:

```bash
for f in tests/test_*.fard; do fardrun test --program "$f"; done
```

**The spec was generated by a FARD program:**

```bash
fardrun run --program tools/gen_spec.fard --out ./out
# produces SPEC.md and SPEC_META.json
```

**The release announcement was generated by a FARD program:**

```bash
fardrun run --program tools/gen_announcement.fard --out ./out
cat ANNOUNCEMENT.md
```

-----

## Architecture

```
Layer 5  Execution ABI v0        bundle → ENC(W*) on stdout
Layer 4  Registry Semantics v0   content-addressed witness storage
Layer 3  Composition Semantics   executions link by verified RunID
Layer 2  Artifact Semantics      same (program, input, deps) → same RunID
Layer 1  Value Core v0           same value → same bytes → same hash
```

Each layer depends only on the one below. The entire system reduces to
one primitive:

```
CID(bytes) = "sha256:" || hex(SHA256(bytes))
```

-----

## Crates

|Crate        |Purpose                                                             |
|-------------|--------------------------------------------------------------------|
|`valuecore`  |Value encoding, decoding, hashing (Layer 1)                         |
|`witnesscore`|Witness construction and identity projection (Layer 2/3)            |
|`abirunner`  |Bundle runner — pure function from bundle to witness bytes (Layer 5)|
|`registry`   |Content-addressed storage by CID (Layer 4)                          |
|`fardc`      |Compiler — .fard source to bundle                                   |
|`fardlang`   |Parser, canonical printer, evaluator                                |

-----

## Specifications

|Document                          |Contents                                       |
|----------------------------------|-----------------------------------------------|
|`spec/fard_spec_stack_v0_final.md`|Trust stack specification (frozen)             |
|`spec/fardlang_grammar_v0.5.txt`  |Surface language grammar                       |
|`FARD_1.0_ROADMAP.md`             |Feature roadmap and design decisions           |
|`SPEC.md`                         |Full stdlib surface spec (generated by FARD)   |
|`ANNOUNCEMENT.md`                 |v1.0.0 release announcement (generated by FARD)|

-----

## License

MUI