use pretty_assertions::assert_eq;
use valuecore::{enc, vdig, Value};
use witnesscore::{mod_entry_v0_1, program_identity_v0_1, trace_v0_1, witness_v0_1};

#[test]
fn vector0_witness_bytes_and_runid_match_frozen_spec() {
    // Vector 0 — Zero-dependency (simplest possible execution)
    //
    // Source: fn main() { unit }
    // Source CID: sha256:053cec7ca391f54effc090ee6f7fff72b912a04a8a38b2466946edbf924f55bf
    // Input: unit
    // Effects: []
    // Imports: []
    // Result: unit
    // RunID: sha256:ab7ebe0282b3bce23992bdb672a547d9eb152bef5434cc726284ac4301c63478

    let program = program_identity_v0_1(
        "main",
        vec![mod_entry_v0_1(
            "main",
            "sha256:053cec7ca391f54effc090ee6f7fff72b912a04a8a38b2466946edbf924f55bf",
        )],
    )
    .unwrap();

    let input = Value::Unit;
    let effects: Vec<Value> = vec![];
    let imports: Vec<Value> = vec![];
    let result = Value::Unit;
    let trace = trace_v0_1(Value::Unit);

    let w = witness_v0_1(program, &input, effects, imports, result, trace).unwrap();

    let got_bytes = enc(&w);
    let got = String::from_utf8(got_bytes).unwrap();

    // Frozen ENC(W*) blob from the spec (Vector 0). Must match byte-for-byte.
    let frozen = r#"{"t":"record","v":[["effects",{"t":"list","v":[]}],["imports",{"t":"list","v":[]}],["input",{"t":"text","v":"sha256:91e321035af75af8327b2d94d23e1fa73cfb5546f112de6a65e494645148a3ea"}],["kind",{"t":"text","v":"fard/witness/v0.1"}],["program",{"t":"record","v":[["entry",{"t":"text","v":"main"}],["kind",{"t":"text","v":"fard/program/v0.1"}],["mods",{"t":"list","v":[{"t":"record","v":[["name",{"t":"text","v":"main"}],["source",{"t":"text","v":"sha256:053cec7ca391f54effc090ee6f7fff72b912a04a8a38b2466946edbf924f55bf"}]]}]}]]}],["result",{"t":"unit"}],["trace",{"t":"record","v":[["cid",{"t":"unit"}],["kind",{"t":"text","v":"fard/trace/v0.1"}]]}]]}"#;

    assert_eq!(got, frozen);

    let runid = vdig(&w);
    assert_eq!(
        runid,
        "sha256:ab7ebe0282b3bce23992bdb672a547d9eb152bef5434cc726284ac4301c63478"
    );
}
