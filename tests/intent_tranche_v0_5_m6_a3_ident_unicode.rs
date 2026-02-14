mod h {
    include!("_m6_harness.rs.inc");
}
use h::*;

#[test]
fn intent_tranche_v0_5_m6_a3_ident_unicode() {
    // TODO: fill this from runtime evidence, then freeze.
    let _ = run_fard_ok("0");
    assert!(true);
}
