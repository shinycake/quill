//! The line layouts a launch keeps for the next one (macOS,
//! `gpui_macos::line_cache`), sealed with the active account's database key.
//!
//! A layout holds its line's text: chat names and message previews. TDLib
//! keeps that text in a database encrypted with the account's key, so the
//! layouts are encrypted with a key derived from it (HKDF-SHA-256, then
//! XChaCha20-Poly1305) in `accounts/<id>/launch-lines.bin`. With a local
//! passcode the database key is wrapped by the passcode, and so are the
//! layouts. Without the key (a locked launch) they are not read or written.
#![deny(clippy::indexing_slicing)]

use crate::platform::DatabaseKey;
use crate::settings::AccountPaths;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;
use std::path::PathBuf;
use zeroize::Zeroizing;

const FILE: &str = "launch-lines.bin";
const MAGIC: &[u8; 4] = b"QLL1";
const NONCE_LEN: usize = 24;
const CONTEXT: &[u8] = b"quill-launch-lines-v1";
/// A sealed file larger than this is not read.
const MAX_FILE: u64 = 4 << 20;

/// Where the active account's layouts live, and the key that seals them.
pub struct LinesFile {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
}

impl LinesFile {
    pub fn new(paths: &AccountPaths, database_key: &DatabaseKey) -> Self {
        let mut key = Zeroizing::new([0u8; 32]);
        // 32 bytes is always a valid HKDF-SHA-256 output length.
        let _ =
            Hkdf::<Sha256>::new(None, database_key.as_bytes()).expand(CONTEXT, key.as_mut_slice());
        Self {
            path: paths.root.join(FILE),
            key,
        }
    }

    /// The layouts the last launch saved, or `None` when there are none or
    /// they were sealed with another key.
    pub fn read(&self) -> Option<Vec<u8>> {
        if std::fs::metadata(&self.path).ok()?.len() > MAX_FILE {
            return None;
        }
        self.open(&std::fs::read(&self.path).ok()?)
    }

    pub fn write(&self, layouts: &[u8]) -> std::io::Result<()> {
        let sealed = self.seal(layouts).ok_or(std::io::ErrorKind::InvalidData)?;
        let partial = self.path.with_extension("partial");
        std::fs::write(&partial, sealed)?;
        std::fs::rename(&partial, &self.path)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(<&Key>::from(&*self.key))
    }

    fn seal(&self, plain: &[u8]) -> Option<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let body = self
            .cipher()
            .encrypt(
                <&XNonce>::from(&nonce),
                Payload {
                    msg: plain,
                    aad: CONTEXT,
                },
            )
            .ok()?;
        let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + body.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&body);
        Some(out)
    }

    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        let rest = sealed.strip_prefix(MAGIC)?;
        let (nonce, msg) = rest.split_at_checked(NONCE_LEN)?;
        let nonce = <&XNonce>::try_from(nonce).ok()?;
        self.cipher()
            .decrypt(nonce, Payload { msg, aad: CONTEXT })
            .ok()
    }
}

/// The active account's database key is known (the launch's connection is
/// being prepared, on a worker): hand the text system its saved layouts,
/// and keep the file to write this launch's to.
pub fn open(paths: &AccountPaths, database_key: &DatabaseKey) {
    #[cfg(all(target_os = "macos", feature = "ui"))]
    {
        let file = LinesFile::new(paths, database_key);
        let stamp = crate::launch_prewarm::cache_stamp();
        gpui_macos::line_cache::load(file.read().as_deref(), &stamp);
        let _ = OPENED.set(file);
    }
    #[cfg(not(all(target_os = "macos", feature = "ui")))]
    let _ = (paths, database_key);
}

#[cfg(all(target_os = "macos", feature = "ui"))]
static OPENED: std::sync::OnceLock<LinesFile> = std::sync::OnceLock::new();

/// Writes this launch's layouts, when [`open`] knew the key. Recording stops
/// either way. Call it off the main thread.
pub fn save() {
    #[cfg(all(target_os = "macos", feature = "ui"))]
    {
        let stamp = crate::launch_prewarm::cache_stamp();
        if let Some(layouts) = gpui_macos::line_cache::save(&stamp)
            && let Some(file) = OPENED.get()
        {
            let _ = file.write(&layouts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-lines-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn file(dir: &std::path::Path, key: u8) -> LinesFile {
        let paths = AccountPaths::for_root(dir, &crate::ids::AccountKey::primary());
        std::fs::create_dir_all(&paths.root).expect("account folder");
        LinesFile::new(
            &paths,
            &DatabaseKey::from_bytes(vec![key; 32]).expect("key"),
        )
    }

    #[test]
    fn layouts_come_back_only_with_the_same_database_key() {
        let dir = temp("key");
        let lines = file(&dir, 1);
        lines.write(b"chat names and previews").expect("written");

        let on_disk = std::fs::read(&lines.path).expect("in the account folder");
        assert!(
            !on_disk.windows(10).any(|part| part == b"chat names"),
            "the text is not on disk in the clear"
        );
        assert_eq!(
            file(&dir, 1).read().as_deref(),
            Some(&b"chat names and previews"[..])
        );
        assert_eq!(file(&dir, 2).read(), None, "another account's key");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_is_not_read() {
        let dir = temp("damaged");
        let lines = file(&dir, 1);
        lines.write(b"previews").expect("written");
        let mut sealed = std::fs::read(&lines.path).expect("sealed");
        if let Some(last) = sealed.last_mut() {
            *last ^= 1;
        }
        std::fs::write(&lines.path, &sealed).expect("damaged");
        assert_eq!(lines.read(), None);
        std::fs::write(&lines.path, MAGIC).expect("cut");
        assert_eq!(lines.read(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
