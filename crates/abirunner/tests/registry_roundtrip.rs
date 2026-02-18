use std::process::Command;

use valuecore::{dec, vdig};

#[test]
fn registry_put_get_roundtrip_cid_matches_runid() {
    // Produce witness bytes from a known passing vector (Vector A).
    let bundle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vectorA/bundle");

    let out = Command::new(env!("CARGO_BIN_EXE_abirun"))
        .arg(bundle)
        .output()
        .expect("run abirun");

    assert!(
        out.status.success(),
        "abirun failed status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let witness_bytes = out.stdout;

    // Compute RunID from bytes (RunID := VDIG(dec(bytes)))
    let w = dec(&witness_bytes).expect("decode witness");
    let runid = vdig(&w);

    // Round-trip via registry
    registry::put_bytes(&runid, &witness_bytes).expect("registry put");
    let got = registry::get_bytes(&runid).expect("registry get");

    // Exact bytes preserved
    assert_eq!(got, witness_bytes, "retrieved bytes must equal stored bytes");

    // CID(retrieved_bytes) == RunID
    let w2 = dec(&got).expect("decode retrieved witness");
    let runid2 = vdig(&w2);
    assert_eq!(runid2, runid, "CID(retrieved_bytes) must equal RunID");
}
