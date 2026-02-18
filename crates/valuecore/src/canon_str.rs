use anyhow::Result;

pub fn emit_json_string(out: &mut Vec<u8>, s: &str) -> Result<()> {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(br#"\""#),
            '\\' => out.extend_from_slice(br#"\\"#),
            '\n' => out.extend_from_slice(br#"\n"#),
            '\r' => out.extend_from_slice(br#"\r"#),
            '\t' => out.extend_from_slice(br#"\t"#),
            '\u{08}' => out.extend_from_slice(br#"\b"#),
            '\u{0C}' => out.extend_from_slice(br#"\f"#),
            c if (c as u32) <= 0x1F => {
                let v = c as u32;
                out.extend_from_slice(br#"\u00"#);
                out.push(nybble_lower(((v >> 4) & 0xF) as u8));
                out.push(nybble_lower((v & 0xF) as u8));
            }
            c => {
                let mut buf = [0u8; 4];
                let enc = c.encode_utf8(&mut buf);
                out.extend_from_slice(enc.as_bytes());
            }
        }
    }
    out.push(b'"');
    Ok(())
}

fn nybble_lower(n: u8) -> u8 {
    match n {
        0..=9 => b'0' + n,
        10..=15 => b'a' + (n - 10),
        _ => b'?',
    }
}
