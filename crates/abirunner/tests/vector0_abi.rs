use pretty_assertions::assert_eq;
use std::process::Command;

#[test]
fn abi_vector0_stdout_is_exact_frozen_enc_w() {
    let bundle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vector0/bundle");

    let out = Command::new(env!("CARGO_BIN_EXE_abirun"))
        .arg(bundle)
        .output()
        .expect("run abirun");

    assert!(
        out.status.success(),
        "status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    // stdout must be EXACT ENC(W*) bytes (no newline).
    let got = String::from_utf8(out.stdout).unwrap();

    let frozen = r#"{"t":"record","v":[["effects",{"t":"list","v":[]}],["imports",{"t":"list","v":[]}],["input",{"t":"text","v":"sha256:91e321035af75af8327b2d94d23e1fa73cfb5546f112de6a65e494645148a3ea"}],["kind",{"t":"text","v":"fard/witness/v0.1"}],["program",{"t":"record","v":[["entry",{"t":"text","v":"main"}],["kind",{"t":"text","v":"fard/program/v0.1"}],["mods",{"t":"list","v":[{"t":"record","v":[["name",{"t":"text","v":"main"}],["source",{"t":"text","v":"sha256:053cec7ca391f54effc090ee6f7fff72b912a04a8a38b2466946edbf924f55bf"}]]}]}]]}],["result",{"t":"unit"}],["trace",{"t":"record","v":[["cid",{"t":"unit"}],["kind",{"t":"text","v":"fard/trace/v0.1"}]]}]]}"#;

    assert_eq!(got, frozen);
}
