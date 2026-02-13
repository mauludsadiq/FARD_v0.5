use std::fs;
use std::process::Command;

#[test]
fn m5_preimage_matches_golden_bytes() {
  let outdir = "out/m5_ok_bundle";
  let _ = fs::remove_dir_all(outdir);

  let st = Command::new("cargo")
    .args(["run","-q","--bin","fardrun","--","run","--program","spec/tmp/m5_ok_bundle.fard","--out",outdir])
    .status()
    .expect("SPAWN_FARDRUN");
  assert!(st.success(), "FARDRUN_FAILED");

  let got = Command::new("cargo")
    .args(["run","-q","--bin","fardlock","--","show-preimage","--out",outdir])
    .output()
    .expect("SPAWN_SHOW_PREIMAGE");
  assert!(got.status.success(), "SHOW_PREIMAGE_FAILED");

  let golden = fs::read("spec/golden/m5_preimage.json").expect("READ_GOLDEN");
  assert_eq!(golden, got.stdout, "M5_PREIMAGE_GOLDEN_BYTES_MISMATCH");
}
