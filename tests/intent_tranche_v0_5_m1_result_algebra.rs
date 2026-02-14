include!("_m6_json.rs.inc");
include!("_m6_harness.rs.inc");

fn obj_get<'a>(v: &'a serde_json::Value, k: &str) -> &'a serde_json::Value {
    v.get(k).unwrap_or_else(|| panic!("missing key: {k}"))
}

fn run_ok_payload(prog_src: &str) -> serde_json::Value {
    let top = run_fard_ok(prog_src);
    unwrap_runner_envelope(&top).clone()
}

fn assert_json_eq(a: &serde_json::Value, b: &serde_json::Value) {
    assert_eq!(
        a,
        b,
        "json mismatch\nleft={}\nright={}",
        serde_json::to_string_pretty(a).unwrap(),
        serde_json::to_string_pretty(b).unwrap()
    );
}

#[test]
fn m1_andthen_identity_ok() {
    let src = r#"
import("std/result") as result
let r = result.ok(7)
let lhs = result.andThen(r, fn(x){ result.ok(x) })
{"lhs": lhs, "r": r}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "lhs"), obj_get(&res, "r"));
}

#[test]
fn m1_andthen_identity_err() {
    let src = r#"
import("std/result") as result
let r = result.err("E")
let lhs = result.andThen(r, fn(x){ result.ok(x) })
{"lhs": lhs, "r": r}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "lhs"), obj_get(&res, "r"));
}

#[test]
fn m1_andthen_associativity_ok() {
    let src = r#"
import("std/result") as result
fn f(x){ result.ok(x + 1) }
fn g(y){ result.ok(y * 2) }

let r = result.ok(3)
let lhs = result.andThen(result.andThen(r, f), g)
let rhs = result.andThen(r, fn(x){ result.andThen(f(x), g) })
{"lhs": lhs, "rhs": rhs}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "lhs"), obj_get(&res, "rhs"));
}

#[test]
fn m1_andthen_associativity_err_short_circuit() {
    let src = r#"
import("std/result") as result
fn f(_x){ result.err("E_F") }
fn g(_y){ result.ok(999) }

let r = result.ok(3)
let lhs = result.andThen(result.andThen(r, f), g)
let rhs = result.andThen(r, fn(x){ result.andThen(f(x), g) })
{"lhs": lhs, "rhs": rhs}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "lhs"), obj_get(&res, "rhs"));
}

#[test]
fn m1_match_equiv_andthen_ok() {
    let src = r#"
import("std/result") as result
fn f(x){ result.ok(x + 10) }

let r = result.ok(5)

let m =
  match r {
    {t:"ok", v:v} => f(v),
    {t:"err", e:e} => result.err(e),
  }

let a = result.andThen(r, f)
{"m": m, "a": a}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "m"), obj_get(&res, "a"));
}

#[test]
fn m1_match_equiv_andthen_err() {
    let src = r#"
import("std/result") as result
fn f(x){ result.ok(x + 10) }

let r = result.err("E")

let m =
  match r {
    {t:"ok", v:v} => f(v),
    {t:"err", e:e} => result.err(e),
  }

let a = result.andThen(r, f)
{"m": m, "a": a}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "m"), obj_get(&res, "a"));
}

#[test]
fn m1_qmark_equiv_andthen_ok() {
    let src = r#"
import("std/result") as result
fn f(x){ x + 2 }

let r = result.ok(8)

let q =
  match r {
    {t:"ok", v:v} => result.ok(f(v)),
    {t:"err", e:e} => result.err(e),
  }

let a = result.andThen(r, fn(x){ result.ok(f(x)) })
{"q": q, "a": a}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "q"), obj_get(&res, "a"));
}

#[test]
fn m1_qmark_equiv_andthen_err() {
    let src = r#"
import("std/result") as result
fn f(x){ x + 2 }

let r = result.err("E_Q")

let q =
  match r {
    {t:"ok", v:v} => result.ok(f(v)),
    {t:"err", e:e} => result.err(e),
  }

let a = result.andThen(r, fn(x){ result.ok(f(x)) })
{"q": q, "a": a}
"#;
    let res = run_ok_payload(src);
    assert_json_eq(obj_get(&res, "q"), obj_get(&res, "a"));
}
