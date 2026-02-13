use std::fs;
use std::process::Command;

fn run(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
    let _ = fs::remove_dir_all(outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program_path,
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL")
}

fn must_have_error(outdir: &str) {
    let p = format!("{}/error.json", outdir);
    let _ = fs::read(&p).expect("MISSING_error.json");
}

#[test]
fn m1_rejects_invalid_shape_in_qmark_path() {
    let out = "out/m1_reject_qmark_invalid_shape";
    let p = "spec/tmp/m1_reject_qmark_invalid_shape.fard";
    let st = run(
        r#"
import("std/result") as result
let _x = {t:"ok"}?
result.ok(0)
"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");
    must_have_error(out);
}

#[test]
fn m1_rejects_invalid_shape_in_andthen_path() {
    let out = "out/m1_reject_andthen_invalid_shape";
    let p = "spec/tmp/m1_reject_andthen_invalid_shape.fard";
    let st = run(
        r#"
import("std/result") as result
let bad = {t:"ok", e:{k:1}}
result.andThen(bad, fn(x){ result.ok(x) })
"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");
    must_have_error(out);
}

#[test]
fn m1_rejects_invalid_shape_in_match_canonical_destructure() {
    let out = "out/m1_reject_match_invalid_shape";
    let p = "spec/tmp/m1_reject_match_invalid_shape.fard";
    let st = run(
        r#"
import("std/result") as result
let bad = {t:"err", v:{k:1}}
match bad {
  {t:"ok", v:v} => result.ok(v),
  {t:"err", e:e} => result.err(e),
}
"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");
    must_have_error(out);
}

#[test]
fn m1_rejects_case_drift_and_extra_keys() {
    let out = "out/m1_reject_case_and_extra";
    let p = "spec/tmp/m1_reject_case_and_extra.fard";
    let st = run(
        r#"
import("std/result") as result
let bad = {t:"OK", v:1, extra:2}
result.andThen(bad, fn(x){ result.ok(x) })
"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");
    must_have_error(out);
}
