// ValueCore crate
pub mod canon_hex;
pub mod sha256;
pub mod json;
pub mod hmac_sha256;
pub mod canon_int;
pub mod canon_str;
pub mod dec;
pub mod enc;
pub mod value;
pub mod vdig;

pub use dec::{dec, DecodeError};
pub use canon_hex::{hex_lower, parse_hex, parse_hex_lower};
pub use sha256::Sha256;
pub use json::JsonVal;
pub use hmac_sha256::hmac_sha256;
pub use enc::enc;
pub use value::{Value, ValueTag};
pub use vdig::{cid, vdig};

pub mod v0;

pub mod base64url;
pub mod uuid;
pub mod linalg;
