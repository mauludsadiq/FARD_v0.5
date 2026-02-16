use std::process::Command;
use std::path::Path;

#[test]
fn bytes_json_roundtrip_smoke() {
    let root = std::env::current_dir().unwrap();
    let bin = root.join("target").join("debug").join("fardrun");
    assert!(bin.exists(), "missing fardrun binary at {:?}", bin);

    let outdir = root.join("_out_bytes_rt");
    let _ = std::fs::remove_dir_all(&outdir);

    let prog = root.join("spec").join("tmp").join("bytes_roundtrip.fard");
    std::fs::create_dir_all(prog.parent().unwrap()).unwrap();

    std::fs::write(&prog, r#"
let b = {"t":"bytes","v":"hex:ff0000ff"}
b
"#).unwrap();

    let status = Command::new(bin)
        .args(["run","--program"])
        .arg(&prog)
        .args(["--out"])
        .arg(&outdir)
        .status()
        .unwrap();

    assert!(status.success(), "run failed");
    assert!(Path::new(&outdir.join("result.json")).exists(), "missing result.json");
}
