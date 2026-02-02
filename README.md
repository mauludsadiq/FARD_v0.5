# FARD v0.5 — Language Gate (pass/fail)

This repo is a **compiler-team style “language gate”**: a strict suite of pass/fail checks that declares:

> ✅ “FARD is now a programming language”

…once (and only once) every gate turns green.

There is **no prose-based evaluation** here. The suite is **programs + assertions**.

## What this repo contains

- `tests/gate/programs/*.fard` — the gate programs
- `tests/gate/gates.json` — the gate spec (what to run, what must hold)
- `src/bin/gaterun.rs` — Rust gate runner (executes your existing FARD runner, checks artifacts)
- `fard_gate.toml` — config pointing to your local FARD runner and artifact paths

This does **not** ship a FARD interpreter. It validates your interpreter/toolchain.

---

## Quickstart

### 1) Point the gate runner at your FARD runner

Edit `fard_gate.toml`:

- `runner.cmd` — command vector to execute your runner
- `runner.args` — base args before the gate runner appends `--program <path> --out <dir>`
- `artifacts.*_relpath` — where your runner writes `trace.ndjson` + `result.json`

Default is set to a placeholder:

```toml
[runner]
cmd = ["python", "-m", "fard.toolchain.fardrun_v0_4"]
args = ["run"]

[artifacts]
trace_relpath = "trace.ndjson"
result_relpath = "result.json"
lock_relpath = "fard.lock.json"
```

If your runner is a single binary, use:

```toml
cmd = ["./target/debug/fardrun"]
```

### 2) Run the gate suite

```bash
cargo run --bin gaterun
```

You should see a table like:

- `PASS G01_core_eval`
- `FAIL G03_std_list_sort_int ...` (until you implement it)

Exit code:
- `0` if **all** gates pass
- nonzero if any gate fails

---

## The definition of “FARD is now a programming language”

**FARD v0.5 passes the gate** if and only if all gates in `tests/gate/gates.json` pass.

This suite is intentionally minimal but **complete**: it gates the required features for FARD to function as a real language, not a demo.

### Gate list (current)

1. **G01_core_eval** — core evaluation surface works
   - `let`, `fn`, call, `if`, lists, records, dot-get (implemented as `get(x,"k")`)
   - must produce deterministic final result

2. **G02_modules_export_import** — real module system
   - `import("lib/math") as M`
   - `export { pi, square }`
   - namespace access `M.pi`, `M.square(5)`

3. **G03_trace_parseable** — proof-grade trace
   - `trace.ndjson` exists
   - every line parses as JSON

4. **G04_determinism_same_trace_bytes** — determinism contract
   - same program bytes + same stdlib bytes + same runtime ⇒ identical `trace.ndjson` bytes
   - gate compares sha256 of `trace.ndjson` across two identical runs

5. **G05_std_list_sort_int** — deterministic sorting in `std/list`
   - `List.sort_int([3,1,2,1]) -> [1,1,2,3]`

6. **G06_std_list_dedupe_sorted_int** — linear dedupe on sorted list
   - `dedupe_sorted_int(sort_int(xs))` produces canonical unique list

7. **G07_std_list_hist_int** — integer histogram
   - `hist_int([5,5,10,0,10,10]) -> [{v:0,count:1},{v:5,count:2},{v:10,count:3}]` (canonical order)

8. **G08_std_grow_unfold** — growth operator required by the ecosystem
   - `Grow.unfold(seed, fuel, step)` must exist and be deterministic
   - expected to produce `[0..9]` for the provided step rule

9. **G09_import_cycle_rejected** — module graph correctness
   - cyclic imports must hard-fail with an explicit error (`IMPORT_CYCLE` or equivalent)

10. **G10_lock_mismatch_rejected** — module locking is enforced
   - passing an invalid lock file must hard-fail (`LOCK_MISMATCH` or equivalent)

---

## Conventions expected from your FARD runner

The gate runner makes only three assumptions (configurable in `fard_gate.toml`):

1) You can run a program via something equivalent to:

```bash
<runner.cmd...> <runner.args...> --program <path> --out <dir>
```

2) The run writes:
- `trace.ndjson` (NDJSON; one JSON object per line)
- `result.json` (JSON; final result somewhere inside)

3) Optional lock enforcement:

```bash
<runner...> --program <path> --out <dir> --lock <lock_path>
```

If your CLI uses different flags, adapt `run_fard()` in `src/lib.rs` or add a thin wrapper script.

---

## VS Code workflow

1. Open this folder in VS Code
2. Update `fard_gate.toml` to point at your local FARD runner
3. Run:

```bash
cargo run --bin gaterun
```

4. Implement missing pieces in your main FARD repo until gates are green.

---

## Why these gates (engineering rationale)

This is the minimum set that makes FARD *language-complete* for your stated goal: rewriting full repos in FARD with deterministic traces.

- Without **G02**, you don’t have namespaces / libraries.
- Without **G03 + G04**, you don’t have auditable proof artifacts.
- Without **G05–G08**, you can’t express the repository-scale workflows you’ve already been writing (palette enumeration, spectral analysis, etc.).
- Without **G09**, the module resolver is unsafe.
- Without **G10**, determinism is not locked to bytes.

---

## License

MIT OR Apache-2.0

# wip_marker_20260202_141107
