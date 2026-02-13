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

fn read_bytes(p: &str) -> Vec<u8> {
    fs::read(p).expect("READ_FAIL")
}

#[test]
fn m1_qmark_equiv_ok_path() {
    let out_a = "out/m1_qmark_equiv_a";
    let out_b = "out/m1_qmark_equiv_b";
    let p_a = "spec/tmp/m1_qmark_equiv_a.fard";
    let p_b = "spec/tmp/m1_qmark_equiv_b.fard";

    let st_a = run(
        r#"
import("std/result") as result
let r = result.ok(3)
let x = r?
result.ok(x + 9)
"#,
        out_a,
        p_a,
    );
    assert!(st_a.success(), "RUNNER_NONZERO_A");

    let st_b = run(
        r#"
import("std/result") as result
let r = result.ok(3)
result.andThen(r, fn(x){ result.ok(x + 9) })
"#,
        out_b,
        p_b,
    );
    assert!(st_b.success(), "RUNNER_NONZERO_B");

    assert_eq!(
        read_bytes(&format!("{}/result.json", out_a)),
        read_bytes(&format!("{}/result.json", out_b)),
        "QMARK_EQUIV_FAIL"
    );
}
