use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

fn write_file(path: &str, s: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, s.as_bytes()).expect("write file");
}

fn trace_paths(out_dir: &str) -> (String, String) {
    let p0 = format!("{out_dir}/trace.ndjson");
    let p1 = format!("{out_dir}/out/trace.ndjson");
    (p0, p1)
}

fn read_trace_any(out_dir: &str) -> Vec<String> {
    let (p0, p1) = trace_paths(out_dir);

    if let Ok(s) = fs::read_to_string(&p0) {
        let v: Vec<String> = s
            .lines()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty())
            .collect();
        if !v.is_empty() {
            return v;
        }
    }

    if let Ok(s) = fs::read_to_string(&p1) {
        let v: Vec<String> = s
            .lines()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty())
            .collect();
        if !v.is_empty() {
            return v;
        }
    }

    panic!("trace must be non-empty at either path: {} OR {}", p0, p1);
}

fn as_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("event must be object")
}

fn req_str(obj: &serde_json::Map<String, serde_json::Value>, k: &str) -> String {
    obj.get(k)
        .and_then(|x| x.as_str())
        .unwrap_or_else(|| panic!("required field missing or not string: {k}"))
        .to_string()
}

fn req_arr(obj: &serde_json::Map<String, serde_json::Value>, k: &str) -> Vec<serde_json::Value> {
    obj.get(k)
        .and_then(|x| x.as_array())
        .unwrap_or_else(|| panic!("required field missing or not array: {k}"))
        .clone()
}

fn run_fard(name: &str, src: &str, expect_ok: bool) -> String {
    let program = format!("spec/tmp/{name}.fard");
    let outdir = format!("out/{name}");

    let _ = fs::remove_dir_all(&outdir);
    write_file(&program, src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let status = Command::new(exe)
        .args(["run", "--program", &program, "--out", &outdir])
        .status()
        .expect("spawn fardrun");

    if expect_ok {
        assert!(status.success(), "runner nonzero: {name}");
    } else {
        assert!(!status.success(), "runner unexpectedly ok: {name}");
    }

    outdir
}

#[derive(Clone, Debug)]
struct ArtifactNode {
    cid: String,
    #[allow(dead_code)]
    #[allow(dead_code)]
    is_input: bool,
}

fn build_artifact_index_and_edges(
    trace_lines: &[String],
) -> (BTreeMap<String, ArtifactNode>, Vec<(String, String)>) {
    let mut nodes: BTreeMap<String, ArtifactNode> = BTreeMap::new();
    let mut edges: Vec<(String, String)> = Vec::new(); // parent_name -> child_name

    for line in trace_lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("trace line must be json");
        let obj = as_obj(&v);
        let t = req_str(obj, "t");

        if t == "artifact_in" {
            let name = req_str(obj, "name");
            let cid = req_str(obj, "cid");

            // M3: imports must be named, and name must be unique in-run
            assert!(
                !nodes.contains_key(&name),
                "duplicate artifact name in trace (artifact_in): {name}"
            );

            nodes.insert(
                name,
                ArtifactNode {
                    cid,
                    is_input: true,
                },
            );
        }

        if t == "artifact_out" {
            let name = req_str(obj, "name");
            let cid = req_str(obj, "cid");

            // M3: output name must be unique in-run
            assert!(
                !nodes.contains_key(&name),
                "duplicate artifact name in trace (artifact_out): {name}"
            );

            // M3: outputs must declare parents
            let parents = req_arr(obj, "parents");
            assert!(
                !parents.is_empty(),
                "artifact_out.parents must be non-empty for derived outputs: {name}"
            );

            for p in parents {
                let pobj = p.as_object().expect("parent entry must be object");
                let p_name = pobj
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or_else(|| panic!("parent.name missing or not string for child {name}"))
                    .to_string();
                let p_cid = pobj
                    .get("cid")
                    .and_then(|x| x.as_str())
                    .unwrap_or_else(|| panic!("parent.cid missing or not string for child {name}"))
                    .to_string();

                // M3: parents must refer to prior declared artifacts
                let pn = nodes.get(&p_name).unwrap_or_else(|| {
                    panic!("artifact_out parent refers to unknown or not-yet-declared name: {p_name} (child {name})")
                });

                // M3: parent cid must match the declared cid
                assert!(
                    pn.cid == p_cid,
                    "artifact_out parent cid mismatch: parent {p_name} declared {} but child {name} references {p_cid}",
                    pn.cid
                );

                edges.push((p_name, name.clone()));
            }

            nodes.insert(
                name,
                ArtifactNode {
                    cid,
                    is_input: false,
                },
            );
        }
    }

    (nodes, edges)
}

fn assert_graph_acyclic(nodes: &BTreeMap<String, ArtifactNode>, edges: &[(String, String)]) {
    // adjacency: parent -> children
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, c) in edges {
        adj.entry(p.clone()).or_default().push(c.clone());
    }

    // DFS with colors: 0=unseen, 1=visiting, 2=done
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    for k in nodes.keys() {
        color.insert(k.clone(), 0);
    }

    fn dfs(
        u: &str,
        adj: &BTreeMap<String, Vec<String>>,
        color: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
    ) {
        let cu = *color.get(u).unwrap_or(&0);
        if cu == 2 {
            return;
        }
        if cu == 1 {
            let mut cycle = stack.clone();
            cycle.push(u.to_string());
            panic!("artifact graph cycle detected: {}", cycle.join(" -> "));
        }

        color.insert(u.to_string(), 1);
        stack.push(u.to_string());

        if let Some(vs) = adj.get(u) {
            for v in vs {
                dfs(v, adj, color, stack);
            }
        }

        stack.pop();
        color.insert(u.to_string(), 2);
    }

    for k in nodes.keys() {
        if *color.get(k).unwrap_or(&0) == 0 {
            let mut stack = Vec::new();
            dfs(k, &adj, &mut color, &mut stack);
        }
    }
}

#[test]
fn m3_artifact_causality_gate_import_then_emit_artifact() {
    // Input artifact (the "parent")
    write_file("spec/tmp/m3_in.bin", "x");

    // NOTE:
    // This gate assumes the runtime emits:
    //   artifact_in:  {t:"artifact_in", name:string, path:string, cid:string}
    //   artifact_out: {t:"artifact_out", name:string, cid:string, parents:[{name,cid},...]}
    //
    // If your current language surface does not accept naming/parents yet,
    // keep the program here as the canonical target program for M3, and update
    // runtime/stdlib to support it.
    //
    // Two-step program: import → emit_artifact (derived output)
    let outdir = run_fard(
        "m3_gate_import_emit",
        r#"
emit({k:"m3"})
let _x = import_artifact_named("in0", "spec/tmp/m3_in.bin") in
let _y = emit_artifact_derived("out0", "m3_out.bin", {a:1}, ["in0"]) in
0
"#,
        true,
    );

    let lines = read_trace_any(&outdir);

    // Parse trace, build artifact index (names->cid) and edges (parent->child),
    // assert: parents correctness + reconstructible acyclic graph.
    let (nodes, edges) = build_artifact_index_and_edges(&lines);

    // Sanity: we must see the named input and named output
    assert!(nodes.contains_key("in0"), "missing artifact_in node in0");
    assert!(nodes.contains_key("out0"), "missing artifact_out node out0");

    // Reconstructibility: out0 must have at least one incoming edge, from in0
    let mut incoming: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (p, c) in &edges {
        incoming.entry(c.clone()).or_default().insert(p.clone());
    }

    let ins = incoming.get("out0").expect("out0 missing incoming parents");
    assert!(ins.contains("in0"), "out0 parents must include in0");

    // Graph must be acyclic (within a run)
    assert_graph_acyclic(&nodes, &edges);
}
