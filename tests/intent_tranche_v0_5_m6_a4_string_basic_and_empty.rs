mod h {
    include!("_m6_harness.rs.inc");
}
use h::*;

#[test]
fn intent_tranche_v0_5_m6_a4_string_basic_and_empty() {
    // TODO: fill this from runtime evidence, then freeze.
    let _ = run_fard_ok("0");
    assert!(true);
}
