use anyhow::{bail, Result};
use fardlang::canon::canonical_module_bytes;
use fardlang::check::check_module;
use fardlang::parse_module;

/// v1 frontend: parse + check + canonicalize.
/// For now, we only support a v1 module whose `fn main` tail expression is `unit` or `1` etc,
/// but canonicalization is fully engaged (module_bytes defined).
pub fn compile_v1_module_to_canon(bytes: &[u8]) -> Result<Vec<u8>> {
    let m = parse_module(bytes)?;
    check_module(&m)?;
    Ok(canonical_module_bytes(&m))
}

/// bootstrap: ensure we still can compile Vector0 surface (module main; fn main { unit })
pub fn ensure_min_entry_is_present(canon: &[u8]) -> Result<()> {
    let s = std::str::from_utf8(canon).unwrap();
    if !s.contains("fn main") {
        bail!("ERROR_PARSE module must define fn main");
    }
    Ok(())
}
