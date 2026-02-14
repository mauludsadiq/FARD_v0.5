pub fn read_json(path: &str) -> serde_json::Value {
  let b = std::fs::read(path).expect("READ_JSON_BYTES");
  serde_json::from_slice(&b).expect("READ_JSON_PARSE")
}

// v0.5 OK result.json shape: {"result": <jsonable-value>}
// Future-proof: if "result" is a Result-shaped object {t:"ok"/"err",...}, unwrap ok-v.
pub fn unwrap_ok_value_from_result_json(result_json: &serde_json::Value) -> serde_json::Value {
  let top = result_json.as_object().expect("RESULT_NOT_OBJECT");
  let r = top.get("result").expect("RESULT_MISSING_result");

  if let Some(robj) = r.as_object() {
    if let Some(tj) = robj.get("t") {
      let t = tj.as_str().expect("RESULT_t_NOT_STR");
      assert_eq!(t, "ok", "RESULT_EXPECTED_OK");
      return robj.get("v").expect("RESULT_MISSING_v").clone();
    }
  }

  r.clone()
}
