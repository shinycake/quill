//! Quill core: ordered TDLib envelopes, reducers, and synthetic UI helpers.
//! The GPUI binary lives in `src/main.rs` and is compiled with `--features ui`.

pub mod auth;
pub mod composer;
pub mod diagnostics;
pub mod ids;
pub mod layout;
pub mod lifecycle;
pub mod pins;
pub mod platform;
pub mod settings;
pub mod state;
pub mod telegram;
pub mod text;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub fn verify_schema_file(schema_path: &Path) -> Result<(), SchemaError> {
    let bytes = fs::read(schema_path).map_err(|_| SchemaError::Missing)?;
    let digest = hex_sha256(&bytes);
    if digest != pins::TD_API_TL_SHA256 {
        return Err(SchemaError::Checksum);
    }
    let text = String::from_utf8_lossy(&bytes);
    for name in pins::REQUIRED_SCHEMA_CONSTRUCTORS {
        let found = text.lines().any(|line| {
            let line = line.trim();
            line == *name
                || line.starts_with(&format!("{name} "))
                || line.starts_with(&format!("{name}="))
        });
        if !found {
            return Err(SchemaError::MissingConstructor(name));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaError {
    Missing,
    Checksum,
    MissingConstructor(&'static str),
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn vendored_schema_matches_pin() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(pins::TD_API_TL_PATH);
        assert_eq!(verify_schema_file(&path), Ok(()));
    }
}
