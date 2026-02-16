use std::fs;
use std::process::Command;

fn read_json(path: &str) -> serde_json::Value {
    let s = fs::read_to_string(path).unwrap();
    serde_json::from_str(&s).unwrap()
}

#[test]
fn png_red_1x1_bytes_are_canonical_and_exact() {
    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out_dir = format!("_out_png_test_{}", std::process::id());
    let _ = fs::remove_dir_all(&out_dir);

    let status = Command::new(exe)
        .args([
            "run",
            "--program",
            "tests/lang_gates_v1/programs/g99_png_red_1x1.fard",
            "--out",
            &out_dir,
        ])
        .status()
        .unwrap();

    assert!(status.success(), "runner failed");

    let v = read_json(&format!("{}/result.json", out_dir));

    let expect_hex = "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000f494441547801010400fbff00ff0000030101008d1de5820000000049454e44ae426082";
    let expect = serde_json::json!({
        "t":"bytes",
        "v": format!("hex:{}", expect_hex)
    });

    assert_eq!(v["result"], expect, "result.json mismatch");
}
