//! Every update package is signed by the release workflow with an Ed25519
//! key that only the `update-signing` environment holds
//! (docs/decisions/release-secrets.md). GitHub's asset digest proves the
//! download is the file on the release, not who put it there: anyone with
//! write access to the repository can replace a release file. The signature
//! covers the version, the file name and the digest, so a valid signature
//! cannot be moved to another release or another platform's package.
#![deny(clippy::indexing_slicing)]

use base64::Engine as _;
use std::io::Read;

/// The public half of the release workflow's `UPDATE_SIGNING_KEY`.
const RELEASE_KEY: &str = "mbSyw41GEifwnhD1cD08fqr5u6BFeCHMGm9OLdu+5a8=";

/// A signature file is one line of base64; anything bigger is not one.
const MAX_SIGNATURE_BYTES: u64 = 1024;

pub(super) const UNSIGNED: &str =
    "The update is not signed by Quill's release key. Nothing was installed.";

/// The signed text, also built by `scripts/release-sign-updates.sh`.
pub(super) fn message(version: &str, asset_name: &str, sha256: &str) -> String {
    format!(
        "quill-update-v1\n{version}\n{asset_name}\n{}\n",
        sha256.to_ascii_lowercase()
    )
}

/// Whether `signature` (the `.sig` file's text) signs this package with `key`
/// (a base64 Ed25519 public key).
pub(super) fn verifies(
    key: &str,
    version: &str,
    asset_name: &str,
    sha256: &str,
    signature: &[u8],
) -> bool {
    let engine = base64::engine::general_purpose::STANDARD;
    let (Ok(key), Ok(signature)) = (engine.decode(key), engine.decode(signature.trim_ascii()))
    else {
        return false;
    };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(message(version, asset_name, sha256).as_bytes(), &signature)
        .is_ok()
}

/// The key the package must be signed with: the release key, or in test and
/// screenshot builds that read a local fixture, the fixture's own key.
fn trusted_key() -> String {
    #[cfg(feature = "demo-capture")]
    if crate::updater::fixture_base().is_some()
        && let Ok(key) = std::env::var("QUILL_UPDATE_FIXTURE_KEY")
    {
        return key;
    }
    RELEASE_KEY.to_string()
}

/// Downloads `<asset url>.sig` and checks it signs the package the caller
/// already checked against `sha256`.
pub(super) fn check(asset_url: &str, version: &str, sha256: &str) -> Result<(), &'static str> {
    let asset_name = asset_url.rsplit('/').next().unwrap_or_default();
    let source = crate::updater::download_source(&format!("{asset_url}.sig"));
    let agent = super::fetch::agent(source.https_only, source.redirects);
    let mut response = agent
        .get(&source.url)
        .header("User-Agent", "Quill-updater")
        .call()
        .map_err(|_| UNSIGNED)?;
    let mut signature = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_SIGNATURE_BYTES)
        .read_to_end(&mut signature)
        .map_err(|_| UNSIGNED)?;
    if verifies(&trusted_key(), version, asset_name, sha256, &signature) {
        Ok(())
    } else {
        Err(UNSIGNED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn signed(version: &str, asset: &str, sha256: &str) -> (String, Vec<u8>) {
        let rng = ring::rand::SystemRandom::new();
        let Ok(pkcs8) = Ed25519KeyPair::generate_pkcs8(&rng) else {
            panic!("key generation");
        };
        let Ok(pair) = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()) else {
            panic!("key parse");
        };
        let engine = base64::engine::general_purpose::STANDARD;
        let signature = pair.sign(message(version, asset, sha256).as_bytes());
        (
            engine.encode(pair.public_key().as_ref()),
            format!("{}\n", engine.encode(signature.as_ref())).into_bytes(),
        )
    }

    const SHA: &str = "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12";

    #[test]
    fn a_package_signature_verifies_only_for_its_own_release_and_file() {
        let (key, sig) = signed("2026.1122.7000", "quill-linux-x86_64-bundle.tar.gz", SHA);
        assert!(verifies(
            &key,
            "2026.1122.7000",
            "quill-linux-x86_64-bundle.tar.gz",
            SHA,
            &sig
        ));
        assert!(!verifies(
            &key,
            "2026.1122.7001",
            "quill-linux-x86_64-bundle.tar.gz",
            SHA,
            &sig
        ));
        assert!(!verifies(
            &key,
            "2026.1122.7000",
            "quill-windows-x86_64.zip",
            SHA,
            &sig
        ));
        let other = SHA.replace("ab", "cd");
        assert!(!verifies(
            &key,
            "2026.1122.7000",
            "quill-linux-x86_64-bundle.tar.gz",
            &other,
            &sig
        ));
    }

    #[test]
    fn another_key_or_garbage_never_verifies() {
        let (_, sig) = signed("1.0.0", "quill-windows-x86_64.zip", SHA);
        let (other_key, _) = signed("1.0.0", "quill-windows-x86_64.zip", SHA);
        assert!(!verifies(
            &other_key,
            "1.0.0",
            "quill-windows-x86_64.zip",
            SHA,
            &sig
        ));
        assert!(!verifies(
            RELEASE_KEY,
            "1.0.0",
            "quill-windows-x86_64.zip",
            SHA,
            &sig
        ));
        assert!(!verifies(
            RELEASE_KEY,
            "1.0.0",
            "quill-windows-x86_64.zip",
            SHA,
            b"not base64!"
        ));
        assert!(!verifies(
            RELEASE_KEY,
            "1.0.0",
            "quill-windows-x86_64.zip",
            SHA,
            b""
        ));
    }

    /// Made by `scripts/release-sign-updates.sh` with the real release key for
    /// a 5-byte file ("hello") at version 0.0.1, which never updates anyone.
    #[test]
    fn the_release_script_signature_verifies_with_the_release_key() {
        let sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let sig = b"n/AxysKhqdMy2O7j6nQRvGaYbhZs0xuzyE5yS+lcCy1U1F+NYDTLNW+LNZwMN/9hV+n+j/vwG0fZXU77l/9wDQ==\n";
        assert!(verifies(
            RELEASE_KEY,
            "0.0.1",
            "quill-windows-x86_64.zip",
            sha,
            sig
        ));
        assert!(!verifies(
            RELEASE_KEY,
            "0.0.2",
            "quill-windows-x86_64.zip",
            sha,
            sig
        ));
    }

    #[test]
    fn the_digest_is_signed_in_lowercase() {
        let (key, sig) = signed("1.0.0", "quill-macos-aarch64.zip", SHA);
        assert!(verifies(
            &key,
            "1.0.0",
            "quill-macos-aarch64.zip",
            &SHA.to_ascii_uppercase(),
            &sig
        ));
    }
}
