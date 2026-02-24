use anyhow::{bail, Result};

pub fn emit_hex_lower(out: &mut Vec<u8>, b: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in b {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0f) as usize]);
    }
}

pub fn hex_lower(b: &[u8]) -> String {
    let mut out = Vec::with_capacity(b.len() * 2);
    emit_hex_lower(&mut out, b);
    unsafe { String::from_utf8_unchecked(out) }
}

// Decoder strictness: reject uppercase A-F to enforce canonical lowercase.
pub fn parse_hex_lower(s: &str) -> Result<Vec<u8>> {
    if (s.len() % 2) != 0 {
        bail!("DECODE_BAD_HEX");
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_nibble_lower(chunk[0])?;
        let lo = hex_nibble_lower(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

// Permissive decode: accepts both upper and lower case.
pub fn parse_hex(s: &str) -> Result<Vec<u8>> {
    if (s.len() % 2) != 0 {
        bail!("DECODE_BAD_HEX");
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble_lower(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => bail!("DECODE_BAD_HEX"),
    }
}

fn hex_nibble(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => bail!("DECODE_BAD_HEX"),
    }
}
