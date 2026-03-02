use valuecore::int::*;

#[test]
fn add_overflow() {
    assert!(i64_add(i64::MAX, 1).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn sub_overflow() {
    assert!(i64_sub(i64::MIN, 1).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn mul_overflow() {
    assert!(i64_mul(i64::MAX, 2).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn div_by_zero() {
    assert!(i64_div(10, 0).unwrap_err().to_string().contains("ERROR_DIV_ZERO"));
}

#[test]
fn rem_by_zero() {
    assert!(i64_rem(10, 0).unwrap_err().to_string().contains("ERROR_DIV_ZERO"));
}

#[test]
fn div_overflow_min_neg_one() {
    assert!(i64_div(i64::MIN, -1).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
    assert!(i64_rem(i64::MIN, -1).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn neg_overflow() {
    assert!(i64_neg(i64::MIN).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn abs_overflow() {
    assert!(i64_abs(i64::MIN).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn division_truncates_toward_zero() {
    for &(a, b, q, r) in &[
        ( 7,  3,  2,  1),
        (-7,  3, -2, -1),
        ( 7, -3, -2,  1),
        (-7, -3,  2, -1),
    ] {
        assert_eq!(i64_div(a, b).unwrap(), q, "div a={} b={}", a, b);
        assert_eq!(i64_rem(a, b).unwrap(), r, "rem a={} b={}", a, b);
        assert_eq!(a, q * b + r, "identity a={} b={}", a, b);
    }
}

#[test]
fn min_max_no_overflow() {
    assert_eq!(i64_min(3, 7), 3);
    assert_eq!(i64_max(3, 7), 7);
    assert_eq!(i64_min(i64::MIN, i64::MAX), i64::MIN);
    assert_eq!(i64_max(i64::MIN, i64::MAX), i64::MAX);
}

#[test]
fn pow_overflow() {
    assert!(i64_pow(i64::MAX, 2).unwrap_err().to_string().contains("ERROR_OVERFLOW"));
}

#[test]
fn pow_basic() {
    assert_eq!(i64_pow(2, 10).unwrap(), 1024);
    assert_eq!(i64_pow(0, 0).unwrap(), 1);
    assert_eq!(i64_pow(-2, 3).unwrap(), -8);
}
