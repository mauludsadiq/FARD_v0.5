use anyhow::{bail, Result};

pub fn emit_hex_lower(out: &mut Vec<u8>, b: &[u8]) {
    let s = hex::encode(b);
    out.extend_from_slice(s.as_bytes());
}

// Decoder strictness: reject uppercase A-F to enforce canonical lowercase.
pub fn parse_hex_lower(s: &str) -> Result<Vec<u8>> {
    if (s.len() % 2) != 0 {
        bail!("DECODE_BAD_HEX");
    }
    for c in s.bytes() {
        let ok = matches!(c, b'0'..=b'9' | b'a'..=b'f');
        if !ok {
            bail!("DECODE_BAD_HEX");
        }
    }
    let bytes = hex::decode(s).map_err(|_| anyhow::anyhow!("DECODE_BAD_HEX"))?;
    Ok(bytes)
}
