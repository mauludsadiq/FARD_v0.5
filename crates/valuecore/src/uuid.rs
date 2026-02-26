//! UUID v4 generation — RFC 9562.
//! Entropy source: /dev/urandom (macOS + Linux).
//! Format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
//! where version nibble = 4, variant bits = 0b10xx.

use std::io::Read;

/// Generate a UUID v4 string using OS entropy (/dev/urandom).
pub fn new_v4() -> String {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("failed to read /dev/urandom");

    // Set version: bits 12-15 of byte 6 = 0100 (4)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    // Set variant: bits 6-7 of byte 8 = 10
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn format_correct() {
        let u = new_v4();
        assert_eq!(u.len(), 36);
        let parts: Vec<&str> = u.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn version_4() {
        for _ in 0..20 {
            let u = new_v4();
            // 3rd group, first char must be '4'
            assert_eq!(&u[14..15], "4", "version nibble wrong: {}", u);
        }
    }

    #[test]
    fn variant_bits() {
        for _ in 0..20 {
            let u = new_v4();
            // 4th group, first char must be 8, 9, a, or b
            let first = u.chars().nth(19).unwrap();
            assert!(
                matches!(first, '8' | '9' | 'a' | 'b'),
                "variant bits wrong: {} in {}",
                first,
                u
            );
        }
    }

    #[test]
    fn uniqueness() {
        let mut seen = HashSet::new();
        for _ in 0..100 {
            let u = new_v4();
            assert!(seen.insert(u.clone()), "duplicate UUID: {}", u);
        }
    }

    #[test]
    fn only_hex_and_dashes() {
        let u = new_v4();
        for c in u.chars() {
            assert!(
                c.is_ascii_hexdigit() || c == '-',
                "unexpected char: {} in {}",
                c,
                u
            );
        }
    }
}
