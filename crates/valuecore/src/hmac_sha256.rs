//! Native HMAC-SHA256 (RFC 2104) — no external dependencies.
use crate::sha256::Sha256;

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // Normalize key to block size (64 bytes)
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hk = Sha256::digest(key);
        k[..32].copy_from_slice(&hk);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    // Inner and outer padding
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    // H(ipad || msg)
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(msg);
    let inner_hash = inner.finalize();

    // H(opad || inner_hash)
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon_hex::hex_lower;

    #[test]
    fn test_rfc2202_vector1() {
        // RFC 2202 test case 1: key=0x0b*20, data="Hi There"
        let key = [0x0bu8; 20];
        let msg = b"Hi There";
        let result = hmac_sha256(&key, msg);
        assert_eq!(
            hex_lower(&result),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn test_rfc2202_vector2() {
        // RFC 2202 test case 2: key="Jefe", data="what do ya want for nothing?"
        let result = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex_lower(&result),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}
