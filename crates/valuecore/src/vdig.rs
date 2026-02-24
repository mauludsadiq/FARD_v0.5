use crate::canon_hex::hex_lower;
use crate::enc::enc;
use crate::sha256::Sha256;
use crate::value::Value;

pub fn cid(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_lower(&Sha256::digest(bytes)))
}

pub fn vdig(v: &Value) -> String {
    cid(&enc(v))
}
