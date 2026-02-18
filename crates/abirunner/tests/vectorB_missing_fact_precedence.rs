use std::process::Command;

#[test]
fn missing_fact_precedence_is_error_missing_fact() {
    let bundle = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/vectorB_missing_fact/bundle"
    );

    let out = Command::new(env!("CARGO_BIN_EXE_abirun"))
        .arg(bundle)
        .output()
        .expect("run abirun");

    assert!(
        !out.status.success(),
        "expected failure but got success; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stderr.contains("ERROR_MISSING_FACT"),
        "expected ERROR_MISSING_FACT; stderr={}",
        stderr
    );

    assert!(
        !stderr.contains("ERROR_MISSING_EFFECT"),
        "must not raise effect error when import fact missing; stderr={}",
        stderr
    );
}
