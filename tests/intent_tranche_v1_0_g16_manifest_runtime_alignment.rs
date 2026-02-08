use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn read_bytes(path: &str) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("READ_FAIL path={} err={}", path, e))
}

fn json_parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("JSON_PARSE_FAIL err={}", e))
}

fn as_obj<'a>(
    v: &'a serde_json::Value,
    ctx: &str,
) -> &'a serde_json::Map<String, serde_json::Value> {
    v.as_object()
        .unwrap_or_else(|| panic!("TYPE_FAIL expected=object ctx={}", ctx))
}

fn as_str<'a>(v: &'a serde_json::Value, ctx: &str) -> &'a str {
    v.as_str()
        .unwrap_or_else(|| panic!("TYPE_FAIL expected=string ctx={}", ctx))
}

fn load_manifest() -> serde_json::Value {
    let path = "spec/stdlib_surface.v1_0.ontology.json";
    let bytes = read_bytes(path);
    json_parse(&bytes)
}

fn fard_bin() -> String {
    if let Ok(p) = std::env::var("FARD_BIN") {
        return p;
    }
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_fardrun") {
        return p;
    }
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_fard") {
        return p;
    }

    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    let cand1 = format!("{}/target/{}/fardrun", root, profile);
    if std::path::Path::new(&cand1).exists() {
        return cand1;
    }

    let cand2 = format!("{}/target/debug/fardrun", root);
    if std::path::Path::new(&cand2).exists() {
        return cand2;
    }

    let cand3 = format!("{}/target/release/fardrun", root);
    if std::path::Path::new(&cand3).exists() {
        return cand3;
    }

    panic!(
        "FARD_BIN_NOT_FOUND set FARD_BIN=/path/to/fardrun or build it at target/{}/fardrun",
        profile
    );
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

fn run_one(module: &str, export: &str) {
    let bin = fard_bin();

    let d = tmpdir("fard_g16");
    let prog_path = d.join("p.fard");
    let out_dir = d.join("out");

    let src = format!(
        "import(\"{module}\") as M\nlet _ = M.{export}\n\"OK\"\n",
        module = module,
        export = export
    );
    fs::write(&prog_path, src.as_bytes()).expect("WRITE_PROG_FAIL");

    let mut cmd = Command::new(&bin);
    cmd.arg("run")
        .arg("--program")
        .arg(&prog_path)
        .arg("--out")
        .arg(&out_dir);

    let out = cmd.output().expect("RUN_SPAWN_FAIL");
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
    if !rj.exists() {
        panic!(
            "RESULT_MISSING module={} export={} path={}",
            module,
            export,
            rj.display()
        );
    }
}

#[test]
fn g16_manifest_implemented_exports_resolve_in_runtime() {
    let v = load_manifest();
    let top = as_obj(&v, "top");
    let modules = as_obj(top.get("modules").unwrap(), "top.modules");

    let mut implemented: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (mname, mval) in modules {
        let mo = as_obj(mval, &format!("module {}", mname));
        let exports = as_obj(mo.get("exports").unwrap(), &format!("{}.exports", mname));
        for (ename, eval) in exports {
            let eo = as_obj(eval, &format!("export {}.{}", mname, ename));
            let status = as_str(
                eo.get("status").unwrap(),
                &format!("{}.{}.status", mname, ename),
            );
            if status == "implemented" {
                implemented
                    .entry(mname.clone())
                    .or_default()
                    .push(ename.clone());
            }
        }
    }

    if implemented.is_empty() {
        panic!("NO_IMPLEMENTED_EXPORTS_IN_MANIFEST");
    }

    for (m, mut es) in implemented {
        es.sort();
        for e in es {
            run_one(&m, &e);
        }
    }
}
