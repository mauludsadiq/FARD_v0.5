use sha2::{Digest, Sha256};

use crate::enc::enc;
use crate::value::Value;

pub fn cid(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    format!("sha256:{}", hex::encode(d))
}

pub fn vdig(v: &Value) -> String {
    let b = enc(v);
    cid(&b)
}
