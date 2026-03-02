use anyhow::{anyhow, Result};

pub fn i64_add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_add"))
}

pub fn i64_sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_sub"))
}

pub fn i64_mul(a: i64, b: i64) -> Result<i64> {
    a.checked_mul(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_mul"))
}

pub fn i64_div(a: i64, b: i64) -> Result<i64> {
    if b == 0 { return Err(anyhow!("ERROR_DIV_ZERO i64_div")); }
    a.checked_div(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_div"))
}

pub fn i64_rem(a: i64, b: i64) -> Result<i64> {
    if b == 0 { return Err(anyhow!("ERROR_DIV_ZERO i64_rem")); }
    a.checked_rem(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_rem"))
}

pub fn i64_neg(a: i64) -> Result<i64> {
    a.checked_neg().ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_neg"))
}

pub fn i64_abs(a: i64) -> Result<i64> {
    a.checked_abs().ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_abs"))
}

pub fn i64_min(a: i64, b: i64) -> i64 {
    a.min(b)
}

pub fn i64_max(a: i64, b: i64) -> i64 {
    a.max(b)
}

pub fn i64_pow(base: i64, exp: u32) -> Result<i64> {
    base.checked_pow(exp).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_pow"))
}
