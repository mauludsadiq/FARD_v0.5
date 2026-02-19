use pretty_assertions::assert_eq;
use std::process::Command;

use valuecore::{dec, vdig};

#[test]
fn abi_vector_a_single_effect_sat_is_digest_and_matches_frozen() {
    let bundle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vectorA/bundle");

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

    // stdout must be EXACT ENC(W*) bytes (no newline)
    let got = String::from_utf8(out.stdout.clone()).unwrap();

    // Compute RunID from stdout bytes
    let w = dec(&out.stdout).expect("decode witness from stdout");
    let runid = vdig(&w);

    // ---- FREEZE POINT ----
    // After first run, paste exact values below and keep forever.

    // 1) Frozen ENC(W*) blob
    const FROZEN_ENC_W: &str = "{\"t\":\"record\",\"v\":[[\"effects\",{\"t\":\"list\",\"v\":[{\"t\":\"record\",\"v\":[[\"kind\",{\"t\":\"text\",\"v\":\"read/file\"}],[\"req\",{\"t\":\"record\",\"v\":[[\"path\",{\"t\":\"text\",\"v\":\"hello.txt\"}]]}],[\"sat\",{\"t\":\"text\",\"v\":\"sha256:5c0fe797ea82e3171777470131c2e5d69d22163feced5a8202b8c87b22c5dedc\"}]]}]}],[\"imports\",{\"t\":\"list\",\"v\":[]}],[\"input\",{\"t\":\"text\",\"v\":\"sha256:91e321035af75af8327b2d94d23e1fa73cfb5546f112de6a65e494645148a3ea\"}],[\"kind\",{\"t\":\"text\",\"v\":\"fard/witness/v0.1\"}],[\"program\",{\"t\":\"record\",\"v\":[[\"entry\",{\"t\":\"text\",\"v\":\"main\"}],[\"kind\",{\"t\":\"text\",\"v\":\"fard/program/v0.1\"}],[\"mods\",{\"t\":\"list\",\"v\":[{\"t\":\"record\",\"v\":[[\"name\",{\"t\":\"text\",\"v\":\"main\"}],[\"source\",{\"t\":\"text\",\"v\":\"sha256:053cec7ca391f54effc090ee6f7fff72b912a04a8a38b2466946edbf924f55bf\"}]]}]}]]}],[\"result\",{\"t\":\"unit\"}],[\"trace\",{\"t\":\"record\",\"v\":[[\"cid\",{\"t\":\"unit\"}],[\"kind\",{\"t\":\"text\",\"v\":\"fard/trace/v0.1\"}]]}]]}";

    // 2) Frozen RunID (you indicated prefix sha256:8a9e4b57....)
    const FROZEN_RUNID: &str =
        "sha256:3415a772daf4fcc74216cf3cec989aa6fab4ee566b9304c97897b94ee59be610";

    // Strict checks (become active once frozen)
    if FROZEN_ENC_W != "__FILL_ME_WITH_EXACT_STDOUT__" {
        assert_eq!(got, FROZEN_ENC_W);
    }
    if FROZEN_RUNID != "sha256:__FILL_ME__" {
        assert_eq!(runid, FROZEN_RUNID);
    }

    // Always enforce the *semantic* Gate 2 property:
    // the effect SAT is a digest text, not the raw value.
    // We check the substring '"sat":{"t":"text","v":"sha256:' exists.
    assert!(
        got.contains(r#""sat",{"t":"text","v":"sha256:"#),
        "expected witness effect sat to be text sha256 digest; got={}",
        got
    );
}
