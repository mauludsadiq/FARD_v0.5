use std::fs;

use serde_json::Value;

fn get_std_digest(v: &Value, spec: &str) -> Option<String> {
    let nodes = v.get("nodes")?.as_array()?;
    for n in nodes {
        let kind = n.get("kind")?.as_str()?;
        if kind != "std" {
            continue;
        }
        let s = n.get("spec")?.as_str()?;
        if s == spec {
            let d = n.get("digest")?.as_str()?;
            return Some(d.to_string());
        }
    }
    None
}


fn assert_sha256_digest(s: &str, label: &str) {
    assert!(s.starts_with("sha256:"), "{label} must start with sha256:");
    let hex = &s["sha256:".len()..];
    assert_eq!(hex.len(), 64, "{label} must have 64 hex chars");
    for ch in hex.chars() {
        let ok = ('0' <= ch && ch <= '9') || ('a' <= ch && ch <= 'f');
        assert!(ok, "{label} must be lowercase hex");
    }
}


fn count_std_nodes(v: &Value, spec: &str) -> usize {
    let nodes = match v.get("nodes").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return 0,
    };
    let mut c = 0usize;
    for n in nodes {
        let kind = match n.get("kind").and_then(|x| x.as_str()) {
            Some(k) => k,
            None => continue,
        };
        if kind != "std" {
            continue;
        }
        let s = match n.get("spec").and_then(|x| x.as_str()) {
            Some(ss) => ss,
            None => continue,
        };
        if s == spec {
            c += 1;
        }
    }
    c
}


fn get_std_node_id(v: &Value, spec: &str) -> Option<i64> {
    let nodes = v.get("nodes")?.as_array()?;
    for n in nodes {
        let kind = n.get("kind")?.as_str()?;
        if kind != "std" {
            continue;
        }
        let s = n.get("spec")?.as_str()?;
        if s == spec {
            return n.get("id")?.as_i64();
        }
    }
    None
}

fn count_import_edges_from_to(v: &Value, from: i64, to: i64) -> usize {
    let edges = match v.get("edges").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return 0,
    };
    let mut c = 0usize;
    for e in edges {
        let kind = match e.get("kind").and_then(|x| x.as_str()) {
            Some(k) => k,
            None => continue,
        };
        if kind != "import" {
            continue;
        }
        let f = match e.get("from").and_then(|x| x.as_i64()) {
            Some(x) => x,
            None => continue,
        };
        let t = match e.get("to").and_then(|x| x.as_i64()) {
            Some(x) => x,
            None => continue,
        };
        if f == from && t == to {
            c += 1;
        }
    }
    c
}

fn count_import_edges(v: &Value) -> usize {
    let edges = match v.get("edges").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return 0,
    };
    let mut c = 0usize;
    for e in edges {
        let kind = match e.get("kind").and_then(|x| x.as_str()) {
            Some(k) => k,
            None => continue,
        };
        if kind == "import" {
            c += 1;
        }
    }
    c
}


fn has_std_spec(v: &Value, spec: &str) -> bool {
    let nodes = match v.get("nodes").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return false,
    };
    for n in nodes {
        let kind = match n.get("kind").and_then(|x| x.as_str()) {
            Some(k) => k,
            None => continue,
        };
        if kind != "std" {
            continue;
        }
        let s = match n.get("spec").and_then(|x| x.as_str()) {
            Some(ss) => ss,
            None => continue,
        };
        if s == spec {
            return true;
        }
    }
    false
}

#[test]
fn g19_module_graph_std_record_alias_digest_equal() {
    // This test assumes your harness already writes a run output with module_graph.json.
    // We reuse the existing tranche pattern: run a tiny program then inspect out/module_graph.json.

    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).expect("mkdir out");

    let prog = tmp.path().join("p.fard");
    fs::write(
        &prog,
        r#"
import("std/rec") as A
import("std/record") as B
0
"#,
    )
    .expect("write program");

    // Invoke the compiled runner from target/ (same pattern used in other intent tranche tests).
    // If your other tests use a helper, replace this with that helper.
    let status = std::process::Command::new("target/debug/fardrun")
        .args([
            "run",
            "--program",
            prog.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("run fardrun");

    assert!(status.success(), "fardrun failed");

    let mg = fs::read_to_string(out.join("module_graph.json")).expect("read module_graph.json");
    let v: Value = serde_json::from_str(&mg).expect("parse module_graph.json");

    assert_eq!(count_std_nodes(&v, "std/rec"), 1, "std/rec must appear exactly once in module_graph nodes");
    assert_eq!(count_std_nodes(&v, "std/record"), 1, "std/record must appear exactly once in module_graph nodes");

    let id_prog = 0i64;
    let id_rec = get_std_node_id(&v, "std/rec").expect("std/rec node id missing");
    let id_record = get_std_node_id(&v, "std/record").expect("std/record node id missing");

    assert_eq!(
        count_import_edges_from_to(&v, id_prog, id_rec),
        1,
        "expected exactly one import edge from program node to std/rec"
    );
    assert_eq!(
        count_import_edges_from_to(&v, id_prog, id_record),
        1,
        "expected exactly one import edge from program node to std/record"
    );
    assert_eq!(
        count_import_edges(&v),
        2,
        "expected exactly two import edges total for this program"
    );
    assert!(has_std_spec(&v, "std/rec"), "module_graph must preserve std/rec spec literal");
    assert!(has_std_spec(&v, "std/record"), "module_graph must preserve std/record spec literal");

    let d_rec = get_std_digest(&v, "std/rec").expect("std/rec digest missing");
    let d_record = get_std_digest(&v, "std/record").expect("std/record digest missing");
    assert_sha256_digest(&d_rec, "std/rec digest");
    assert_sha256_digest(&d_record, "std/record digest");

    assert!(!d_rec.is_empty(), "std/rec digest must be non-empty");
    assert!(!d_record.is_empty(), "std/record digest must be non-empty");

    assert_eq!(
        d_rec, d_record,
        "std/record must alias std/rec digest in module_graph.json"
    );
}
