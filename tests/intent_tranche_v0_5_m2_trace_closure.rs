use std::fs;

fn read_lines(p: &str) -> Vec<String> {
    let s = fs::read_to_string(p).expect("read trace");
    s.lines().map(|x| x.to_string()).filter(|x| !x.trim().is_empty()).collect()
}

fn assert_m2_event_shape(line: &str) {
    let v: serde_json::Value = serde_json::from_str(line).expect("trace line must be json");
    let obj = v.as_object().expect("trace line must be object");
    let t = obj.get("t").and_then(|x| x.as_str()).expect("event.t string");

    match t {
        "emit" => {
            assert!(obj.contains_key("v"), "emit requires v");
        }
        "module_resolve" => {
            assert!(obj.get("name").and_then(|x| x.as_str()).is_some(), "module_resolve requires name:string");
            assert!(obj.get("kind").and_then(|x| x.as_str()).is_some(), "module_resolve requires kind:string");
            assert!(obj.get("cid").and_then(|x| x.as_str()).is_some(), "module_resolve requires cid:string");
        }
        "artifact_in" => {
            assert!(obj.get("path").and_then(|x| x.as_str()).is_some(), "artifact_in requires path:string");
            assert!(obj.get("cid").and_then(|x| x.as_str()).is_some(), "artifact_in requires cid:string");
        }
        "artifact_out" => {
            assert!(obj.get("name").and_then(|x| x.as_str()).is_some(), "artifact_out requires name:string");
            assert!(obj.get("cid").and_then(|x| x.as_str()).is_some(), "artifact_out requires cid:string");
        }
        "error" => {
            assert!(obj.get("code").and_then(|x| x.as_str()).is_some(), "error requires code:string");
            assert!(obj.get("message").and_then(|x| x.as_str()).is_some(), "error requires message:string");
        }
        "grow_node" => {
            assert!(obj.contains_key("v"), "grow_node requires v");
        }
        _ => panic!("M2: unknown event kind: {t}"),
    }
}

fn check_trace_dir(out_dir: &str) {
    let trace_path = format!("{out_dir}/trace.ndjson");
    let lines = read_lines(&trace_path);
    assert!(!lines.is_empty(), "trace must be non-empty: {trace_path}");
    for line in lines {
        assert_m2_event_shape(&line);
    }
}

#[test]
fn m2_trace_closure_across_probes() {
    check_trace_dir("out/m2_p0");
    check_trace_dir("out/m2_p1");
    check_trace_dir("out/m2_p2");
    check_trace_dir("out/m2_p3");
    check_trace_dir("out/m2_p4");
}
