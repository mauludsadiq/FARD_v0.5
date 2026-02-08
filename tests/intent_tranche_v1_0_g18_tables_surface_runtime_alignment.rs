use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn fard_bin() -> String {
    if let Ok(p) = std::env::var("FARD_BIN") {
        return p;
    }
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_fardrun") {
        return p;
    }
    "target/debug/fardrun".to_string()
}

fn tmpdir(prefix: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    let pid = std::process::id();
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    d.push(format!("{}_{}_{}", prefix, pid, t));
    fs::create_dir_all(&d).expect("TMPDIR_CREATE_FAIL");
    d
}

fn mk_prog(module: &str, export: &str) -> String {
    let mut s = String::new();
    s.push_str("import(\"std/rec\") as r\n");
    s.push_str(&format!("import(\"{}\") as M\n", module));
    s.push_str("let v0 = M\n");
    let mut i = 0usize;
    for seg in export.split('.') {
        let prev = i;
        i += 1;
        s.push_str(&format!("let v{} = r.get(v{}, \"{}\")\n", i, prev, seg));
    }
    s.push_str(&format!("let _ = v{}\n", i));
    s.push_str("0\n");
    s
}

fn run_one(module: &str, export: &str) {
    let bin = fard_bin();
    let d = tmpdir("fard_g18");
    let prog_path = d.join("p.fard");
    let out_dir = d.join("out");

    let src = mk_prog(module, export);
    fs::write(&prog_path, src.as_bytes()).expect("WRITE_PROG_FAIL");

    let out = Command::new(&bin)
        .arg("run")
        .arg("--program")
        .arg(&prog_path)
        .arg("--out")
        .arg(&out_dir)
        .output()
        .expect("RUN_SPAWN_FAIL");

    if !out.status.success() {
        let code = out.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        panic!(
            "RUN_FAIL module={} export={} code={} stdout={} stderr={}",
            module, export, code, stdout, stderr
        );
    }

    let rj = out_dir.join("result.json");
    assert!(
        rj.exists(),
        "RESULT_MISSING module={} export={} path={}",
        module,
        export,
        rj.display()
    );
}

#[test]
fn g18_tables_surface_runtime_alignment() {
    let bytes =
        fs::read("spec/stdlib_surface_tables.v1_0.ontology.json").expect("READ_TABLES_FAIL");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON_PARSE_FAIL");

    let slice_re = std::env::var("FARD_G18_SLICE_RE")
        .unwrap_or_else(|_| r"^std/(option|null|path|time|trace|artifact)$".to_string());
    let slice = |m: &str| regex::Regex::new(&slice_re).unwrap().is_match(m);
    let mut selected = 0usize;

    let modules = v
        .get("modules")
        .and_then(|x| x.as_array())
        .expect("TYPE_FAIL modules");

    for mo in modules {
        let mname = mo
            .get("name")
            .and_then(|x| x.as_str())
            .expect("TYPE_FAIL module.name");

        if !slice(mname) {
            continue;
        }
        selected += 1;

        let exports = mo
            .get("exports")
            .and_then(|x| x.as_array())
            .expect("TYPE_FAIL module.exports");

        for eo in exports {
            let ename = eo
                .get("name")
                .and_then(|x| x.as_str())
                .expect("TYPE_FAIL export.name");
            run_one(mname, ename);
        }
    }

    assert!(selected > 0, "EMPTY_SLICE re={}", slice_re);
}
