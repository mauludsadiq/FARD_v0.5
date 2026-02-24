use std::fs;

pub fn sha256_file_hex(path: &str) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read failed: {path}: {e}"));
    sha256_bytes_hex(&bytes)
}

pub fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut h = valuecore::Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    valuecore::hex_lower(&out)
}
