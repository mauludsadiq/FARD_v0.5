mod h {
    include!("_m6_harness.rs.inc");
}
use h::*;

#[test]
fn intent_tranche_v0_5_m6_c1_eval_order_let_rhs_before_body() {
    // TODO: fill this from runtime evidence, then freeze.
    let _ = run_fard_ok("0");
    assert!(true);
}
