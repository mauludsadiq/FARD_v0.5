use sha2::{Digest, Sha256};

use crate::canon_hex::hex_lower;
use crate::enc::enc;
use crate::value::Value;

pub fn cid(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex_lower(&h.finalize()))
}

pub fn vdig(v: &Value) -> String {
    let b = enc(v);
    cid(&b)
}
