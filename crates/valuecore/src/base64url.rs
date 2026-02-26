//! Base64url encoding — RFC 4648 §5, no padding.
//! Alphabet: A-Z a-z 0-9 - _
//! Verified against RFC 4648 test vectors.

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes to base64url (no padding).
pub fn encode(input: &[u8]) -> String {
    let mut out = Vec::with_capacity((input.len() * 4 + 2) / 3);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((combined >> 18) & 0x3f) as usize]);
        out.push(ALPHABET[((combined >> 12) & 0x3f) as usize]);
        if chunk.len() > 1 {
            out.push(ALPHABET[((combined >> 6) & 0x3f) as usize]);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(combined & 0x3f) as usize]);
        }
    }
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode base64url (no padding). Returns Err with message on invalid input.
pub fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut table = [0xffu8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 1);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &byte in input {
        let val = table[byte as usize];
        if val == 0xff {
            return Err(format!("invalid base64url character: 0x{:02x}", byte));
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(encode(b""), "");
        assert_eq!(decode(b"").unwrap(), b"");
    }

    #[test]
    fn rfc_vectors() {
        assert_eq!(encode(b"f"), "Zg");
        assert_eq!(encode(b"fo"), "Zm8");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg");
        assert_eq!(encode(b"fooba"), "Zm9vYmE");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn roundtrip() {
        let cases: &[&[u8]] = &[b"", b"a", b"ab", b"abc", b"\x00\xff\xfe", b"hello world"];
        for case in cases {
            let enc = encode(case);
            let dec = decode(enc.as_bytes()).unwrap();
            assert_eq!(&dec, case);
        }
    }

    #[test]
    fn url_safe_chars() {
        let enc = encode(&[0xfb, 0xff]);
        assert!(!enc.contains('+'));
        assert!(!enc.contains('/'));
    }

    #[test]
    fn invalid_char_rejected() {
        assert!(decode(b"Zm9v=").is_err());
        assert!(decode(b"Zm9v+").is_err());
    }
}
