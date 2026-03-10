FARD v1.0.0 — Language Specification
Generated: 2026-03-10
========================================

FARD is a pure, deterministic, content-addressed scripting language.
Every run produces a cryptographic digest committing to inputs and outputs.

PRIMITIVES
----------
  int, float, bool, text, bytes, list, record, func, chan, mutex

CONTROL FLOW
------------
  if/else, match (comma-delimited arms), while (certified computation primitive)
  let bindings, fn definitions, top-level recursion, pipe operator |>

STDLIB — 22 modules, 144 functions
--------------------------------------
  std/str — concat, join, split, len, slice, trim, upper, lower, contains, starts_with, ends_with, replace, index_of, chars, pad_left, pad_right, repeat, from_int, from_float
  std/list — map, filter, fold, any, all, find, find_index, flat_map, take, drop, len, zip_with, chunk, sort_by, par_map, reverse, concat
  std/math — sin, cos, tan, asin, acos, atan, atan2, sqrt, pow, abs, floor, ceil, round, log, log2, log10, exp, pi, e
  std/float — to_str_fixed, is_nan, is_inf
  std/re — is_match, find, find_all, split, replace
  std/map — new, get, set, has, delete, keys, values, entries
  std/set — new, add, remove, has, union, intersect, diff, to_list, from_list, size
  std/json — encode, decode, canonicalize
  std/base64 — encode, decode
  std/csv — parse, encode
  std/hash — sha256_bytes, sha256_text
  std/uuid — v4, validate
  std/datetime — now, format, parse, add, diff, field
  std/path — join, base, dir, ext, isAbs, normalize
  std/io — read_file, write_file, append_file, read_lines, file_exists, delete_file, read_stdin, list_dir, make_dir
  std/http — get, post, request
  std/result — ok, err, is_ok, is_err, unwrap_ok, unwrap_err, map, andThen
  std/option — some, none, is_some, is_none, map, and_then, unwrap_or, from_nullable, to_result
  std/type — of
  std/eval — eval
  std/chan — new, send, recv, try_recv, close
  std/mutex — new, lock, unlock, with_lock

GUARANTEES
----------
  - Every run emits fard_run_digest=sha256:... to stdout
  - while loops produce a step-by-step hash chain
  - All output written to --out dir with digests.json

TEST SUITE
----------
  197+ tests in pure FARD across all stdlib modules

FARD v1.0.0 — self-specifying, self-verifying.
