mod h {
    include!("_m6_harness.rs.inc");
}
use h::*;

#[test]
fn intent_tranche_v0_5_m6_b1_precedence_call_vs_unary_vs_qmark() {
    // TODO: fill this from runtime evidence, then freeze.
    let _ = run_fard_ok("0");
    assert!(true);
}
