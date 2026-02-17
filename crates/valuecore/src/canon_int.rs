use anyhow::{bail, Result};
use num_bigint::BigInt;
use num_traits::Zero;

pub fn emit_int_string(out: &mut Vec<u8>, z: &BigInt) {
    // BigInt has no negative zero; but still emit canonical decimal.
    let s = z.to_string();
    out.extend_from_slice(s.as_bytes());
}

// Strict grammar: ^-?(0|[1-9][0-9]*)$ and "-0" forbidden
pub fn parse_int_string(s: &str) -> Result<BigInt> {
    if s.is_empty() {
        bail!("DECODE_BAD_INT");
    }
    if s == "-0" {
        bail!("DECODE_BAD_INT");
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let neg = if bytes[0] == b'-' {
        i = 1;
        true
    } else {
        false
    };
    if i >= bytes.len() {
        bail!("DECODE_BAD_INT");
    }
    if bytes[i] == b'0' {
        if i + 1 != bytes.len() {
            bail!("DECODE_BAD_INT");
        }
        return Ok(BigInt::zero());
    }
    if !(b'1'..=b'9').contains(&bytes[i]) {
        bail!("DECODE_BAD_INT");
    }
    for &c in &bytes[i + 1..] {
        if !(b'0'..=b'9').contains(&c) {
            bail!("DECODE_BAD_INT");
        }
    }
    let v: BigInt = s.parse().map_err(|_| anyhow::anyhow!("DECODE_BAD_INT"))?;
    if neg && v.is_zero() {
        bail!("DECODE_BAD_INT");
    }
    Ok(v)
}
