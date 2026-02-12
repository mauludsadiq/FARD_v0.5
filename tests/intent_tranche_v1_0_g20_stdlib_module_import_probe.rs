use std::process::Command;

fn read_bytes(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("READ_FAIL path={} err={}", path, e))
}

fn json_parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("JSON_PARSE_FAIL err={}", e))
}

fn as_obj<'a>(v: &'a serde_json::Value, ctx: &str) -> &'a serde_json::Map<String, serde_json::Value> {
    v.as_object().unwrap_or_else(|| panic!("TYPE_FAIL expected=object ctx={}", ctx))
}

fn module_list_from_manifest() -> Vec<String> {
    let path = "spec/stdlib_surface.v1_0.ontology.json";
    let bytes = read_bytes(path);
    let v = json_parse(&bytes);

    let top = as_obj(&v, "top");
    let modules_v = top.get("modules").unwrap_or_else(|| panic!("MISSING top.modules"));
    let modules = as_obj(modules_v, "top.modules");

    let mut out: Vec<String> = modules.keys().cloned().collect();
    out.sort();
    out
}

fn write_probe(spec: &str) -> (String, String) {
    std::fs::create_dir_all("out/probes").unwrap_or_else(|e| panic!("MKDIR_FAIL err={}", e));
    let safe = spec.replace('/', "_");

    let program_path = format!("out/probes/g20_import_{}.fard", safe);
    let out_dir = format!("out/probes/g20_run_{}", safe);

    let prog = format!("import(\"{}\") as M\nemit(\"ok\")\n", spec);
    std::fs::write(&program_path, prog.as_bytes())
        .unwrap_or_else(|e| panic!("WRITE_FAIL path={} err={}", program_path, e));

    (program_path, out_dir)
}

fn run_probe(program_path: &str, out_dir: &str) -> (i32, String) {
    std::fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("MKDIR_FAIL out={} err={}", out_dir, e));

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let o = Command::new(exe)
        .arg("run")
        .arg("--program")
        .arg(program_path)
        .arg("--out")
        .arg(out_dir)
        .output()
        .unwrap_or_else(|e| panic!("SPAWN_FAIL exe={} err={}", exe, e));

    let code = o.status.code().unwrap_or(101);
    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
    (code, stderr)
}

#[test]
fn stdlib_modules_declared_by_manifest_are_importable() {
    let mods = module_list_from_manifest();
    if mods.is_empty() {
        panic!("MODULES_EMPTY");
    }

    for spec in mods {
        let (program_path, out_dir) = write_probe(&spec);
        let (code, err) = run_probe(&program_path, &out_dir);
        if code != 0 {
            panic!("IMPORT_PROBE_FAIL spec={} code={} stderr={}", spec, code, err);
        }
    }
}
