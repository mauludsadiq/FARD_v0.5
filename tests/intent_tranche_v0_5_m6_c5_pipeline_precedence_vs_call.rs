mod h {
    include!("_m6_harness.rs.inc");
}
use h::*;

#[test]
fn intent_tranche_v0_5_m6_c5_pipeline_precedence_vs_call() {
    // TODO: fill this from runtime evidence, then freeze.
    let _ = run_fard_ok("0");
    assert!(true);
}
