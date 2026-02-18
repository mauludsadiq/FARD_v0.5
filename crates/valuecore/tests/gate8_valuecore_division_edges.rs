use valuecore::v0;

#[test]
fn gate8_div_by_zero_is_error_div_zero() {
    let e = v0::i64_div(10, 0).unwrap_err().to_string();
    assert!(e.contains("ERROR_DIV_ZERO"), "missing ERROR_DIV_ZERO: {}", e);

    let e = v0::i64_rem(10, 0).unwrap_err().to_string();
    assert!(e.contains("ERROR_DIV_ZERO"), "missing ERROR_DIV_ZERO: {}", e);
}

#[test]
fn gate8_div_overflow_min_over_minus_one_is_error_overflow() {
    let e = v0::i64_div(i64::MIN, -1).unwrap_err().to_string();
    assert!(e.contains("ERROR_OVERFLOW"), "missing ERROR_OVERFLOW: {}", e);

    let e = v0::i64_rem(i64::MIN, -1).unwrap_err().to_string();
    assert!(e.contains("ERROR_OVERFLOW"), "missing ERROR_OVERFLOW: {}", e);
}

#[test]
fn gate8_division_trunc_toward_zero_and_remainder_audited() {
    assert_eq!(v0::i64_div(7, 3).unwrap(), 2);
    assert_eq!(v0::i64_rem(7, 3).unwrap(), 1);

    assert_eq!(v0::i64_div(-7, 3).unwrap(), -2);
    assert_eq!(v0::i64_rem(-7, 3).unwrap(), -1);

    assert_eq!(v0::i64_div(7, -3).unwrap(), -2);
    assert_eq!(v0::i64_rem(7, -3).unwrap(), 1);

    assert_eq!(v0::i64_div(-7, -3).unwrap(), 2);
    assert_eq!(v0::i64_rem(-7, -3).unwrap(), -1);

    for &(a, b) in &[
        (7, 3),
        (-7, 3),
        (7, -3),
        (-7, -3),
        (0, 5),
        (5, 1),
        (-5, 1),
        (5, -1),
        (-5, -1),
        (42, 5),
        (-42, 5),
        (42, -5),
        (-42, -5),
    ] {
        let q = v0::i64_div(a, b).unwrap();
        let r = v0::i64_rem(a, b).unwrap();

        assert_eq!(a, q * b + r, "identity failed: a={} b={} q={} r={}", a, b, q, r);

        let abs_r = r.abs();
        let abs_b = b.abs();
        assert!(abs_r < abs_b, "remainder magnitude failed: a={} b={} r={}", a, b, r);
    }
}
