//! Quill core: ordered TDLib envelopes, reducers, and synthetic UI helpers.
//! The GPUI binary lives in `src/main.rs` and is compiled with `--features ui`.

pub mod auth;
pub mod composer;
pub mod connect;
pub mod connect_smoke;
pub mod credentials;
pub mod diagnostics;
pub mod ids;
pub mod layout;
pub mod lifecycle;
pub mod local_path;
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
    if bytes.len() != pins::TD_API_TL_BYTES {
        return Err(SchemaError::Length {
            got: bytes.len(),
            expected: pins::TD_API_TL_BYTES,
        });
    }
    let digest = hex_sha256(&bytes);
    if digest != pins::TD_API_TL_SHA256 {
        return Err(SchemaError::Checksum);
    }
    let text = String::from_utf8_lossy(&bytes);
    if !text.contains("vector<") {
        return Err(SchemaError::TruncatedVectors);
    }
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
    TruncatedVectors,
    Length { got: usize, expected: usize },
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
    use std::process::Command;

    #[test]
    fn sha256_empty() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn vendored_schema_equals_official_commit() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(pins::TD_API_TL_PATH);
        let vendored = fs::read(&path).expect("vendored schema");
        let official = fetch_official_schema();
        assert_eq!(
            official.len(),
            pins::TD_API_TL_BYTES,
            "official schema size drifted from pin"
        );
        assert_eq!(
            hex_sha256(&official),
            pins::TD_API_TL_SHA256,
            "official schema digest drifted from pin"
        );
        assert_eq!(
            vendored,
            official,
            "vendored schema/td_api.tl must be byte-identical to official td_api.tl at {}",
            pins::TDLIB_GIT_COMMIT
        );
        assert_eq!(verify_schema_file(&path), Ok(()));
        assert!(
            std::str::from_utf8(&official).unwrap().contains("vector<"),
            "official schema must keep typed vector<T> parameters"
        );
    }

    fn fetch_official_schema() -> Vec<u8> {
        let output = Command::new("curl")
            .args([
                "-fsSL",
                "--retry",
                "4",
                "--retry-delay",
                "2",
                pins::TD_API_TL_UPSTREAM_URL,
            ])
            .output()
            .expect("spawn curl to fetch official td_api.tl");
        assert!(
            output.status.success(),
            "failed to fetch {}: {}",
            pins::TD_API_TL_UPSTREAM_URL,
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
}
