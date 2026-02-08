use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fardrun_bin() -> String {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_fardrun") {
        return p;
    }
    "target/debug/fardrun".to_string()
}

fn tmpdir(name: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("fard_g17_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_prog(dir: &Path, module: &str, export: &str) -> PathBuf {
    let alias = "m";
    let prog = format!(
        r#"import("std/rec") as r
import("{module}") as {alias}
let x = r.get({alias}, "{export}")
0
"#
    );
    let p = dir.join("main.fard");
    fs::write(&p, prog.as_bytes()).unwrap();
    p
}

#[test]
fn g17_surface_manifest_matches_builtin_std_maps() {
    let man = PathBuf::from("ontology/stdlib_surface.v1_0.ontology.json");
    let bytes = fs::read(&man).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let entries = v.get("entries").and_then(|x| x.as_array()).unwrap();

    let bin = fardrun_bin();

    for (i, e) in entries.iter().enumerate() {
        let module = e.get("module").and_then(|x| x.as_str()).unwrap();
        let export = e.get("export").and_then(|x| x.as_str()).unwrap();

        let work = tmpdir(&format!(
            "case{}_{}_{}",
            i,
            module.replace("/", "_"),
            export
        ));
        let out = work.join("out");
        fs::create_dir_all(&out).unwrap();

        let prog = write_prog(&work, module, export);

        let output = Command::new(&bin)
            .arg("run")
            .arg("--program")
            .arg(&prog)
            .arg("--out")
            .arg(&out)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        assert!(
            output.status.success(),
            "g17 failed module={} export={} status={:?} stderr={}",
            module,
            export,
            output.status.code(),
            stderr
        );

        assert!(
            !stderr.contains("EXPORT_MISSING"),
            "g17 EXPORT_MISSING module={} export={} stderr={}",
            module,
            export,
            stderr
        );
    }
}
