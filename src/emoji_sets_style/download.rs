//! Downloading a pack (tdesktop `Ui::Emoji::Loader`, a `BlobLoader`).
//!
//! Blocking and bounded: the UI runs it on the background executor and
//! reads [`Progress`] to draw the row's percentage and to cancel (tdesktop
//! destroys the loader when the box closes).
#![deny(clippy::indexing_slicing)]

use std::path::Path;

use sha2::{Digest, Sha256};

use super::catalog::{EmojiSetId, set_dir};
pub use crate::spell_download::Progress;

/// Only HTTPS on GitHub is fetched; the release redirects to its asset host
/// and the hash check vets what arrives.
pub fn is_allowed_url(url: &str) -> bool {
    url.strip_prefix("https://github.com/shinycake/quill/releases/download/")
        .is_some_and(|rest| !rest.contains("..") && !rest.contains('@'))
}

/// Check `zip` against the pack's hash and install it into
/// `<app_root>/emoji-sets/<key>`.
pub fn install(app_root: &Path, id: EmojiSetId, zip: &[u8]) -> Result<(), String> {
    if id.is_system() {
        return Err("Unknown emoji set.".into());
    }
    let hash = Sha256::digest(zip);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    if hex != id.sha256() {
        return Err("The downloaded emoji set is damaged. Retry.".into());
    }
    super::pack::unpack(zip, &set_dir(app_root, id)).map_err(|e| e.to_string())
}

/// Install a pack the user downloaded by hand (the landing page links the
/// release zips): the file's hash says which set it is, so any name works.
pub fn install_file(app_root: &Path, file: &Path) -> Result<EmojiSetId, String> {
    use std::io::Read;

    use super::catalog::MAX_PACK_BYTES;

    const NOT_A_PACK: &str = "That file isn't a Quill emoji set.";
    let mut zip = Vec::new();
    std::fs::File::open(file)
        .and_then(|f| f.take(MAX_PACK_BYTES + 1).read_to_end(&mut zip))
        .map_err(|_| "Couldn't read that file.")?;
    if zip.len() as u64 > MAX_PACK_BYTES {
        return Err(NOT_A_PACK.into());
    }
    let hash = Sha256::digest(&zip);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    let id = EmojiSetId::ALL
        .into_iter()
        .find(|id| !id.is_system() && id.sha256() == hex)
        .ok_or(NOT_A_PACK)?;
    install(app_root, id, &zip)?;
    Ok(id)
}

#[cfg(feature = "ui")]
fn fetch(url: &str, progress: &Progress) -> Result<Vec<u8>, &'static str> {
    use std::io::Read;
    use std::sync::atomic::Ordering;

    use super::catalog::MAX_PACK_BYTES;

    if !is_allowed_url(url) {
        return Err("Unexpected emoji set address.");
    }
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .header("User-Agent", format!("Quill/{}", crate::version::APP))
        .call()
        .map_err(|err| match err {
            ureq::Error::StatusCode(404) => "This emoji set isn't available yet.",
            _ => "Couldn't download the emoji set. Check your connection and retry.",
        })?;
    let mut reader = response.body_mut().as_reader().take(MAX_PACK_BYTES + 1);
    let mut out = Vec::new();
    let mut chunk = [0u8; 32 * 1024];
    loop {
        if progress.is_cancelled() {
            return Err("Cancelled.");
        }
        let n = reader
            .read(&mut chunk)
            .map_err(|_| "The download was interrupted. Retry.")?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(chunk.get(..n).unwrap_or_default());
        progress.received.fetch_add(n as u64, Ordering::Relaxed);
        if out.len() as u64 > MAX_PACK_BYTES {
            return Err("The emoji set is larger than expected.");
        }
    }
    Ok(out)
}

/// Download set `id` and install it. The error text is shown on the row.
#[cfg(feature = "ui")]
pub fn download(app_root: &Path, id: EmojiSetId, progress: &Progress) -> Result<(), String> {
    let zip = fetch(&id.download_url(), progress)?;
    if progress.is_cancelled() {
        return Err("Cancelled.".into());
    }
    install(app_root, id, &zip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_quill_release_urls() {
        for id in EmojiSetId::ALL.into_iter().filter(|id| !id.is_system()) {
            assert!(is_allowed_url(&id.download_url()));
        }
        assert!(!is_allowed_url(
            "http://github.com/shinycake/quill/releases/download/x.zip"
        ));
        assert!(!is_allowed_url(
            "https://github.com/evil/quill/releases/download/x.zip"
        ));
        assert!(!is_allowed_url(
            "https://github.com.evil.example/shinycake/quill/releases/download/x.zip"
        ));
    }

    #[test]
    fn a_hash_mismatch_installs_nothing() {
        let root = std::env::temp_dir().join(format!("quill-emoji-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let err = install(&root, EmojiSetId::Noto, b"not the pack").unwrap_err();
        assert!(err.contains("damaged"));
        assert!(!set_dir(&root, EmojiSetId::Noto).exists());
        assert!(install(&root, EmojiSetId::System, b"").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_that_is_no_pack_is_refused() {
        let root = std::env::temp_dir().join(format!("quill-emoji-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        let file = root.join("emoji-set-noto.zip");
        let _ = std::fs::write(&file, b"renamed holiday photo");
        let err = install_file(&root, &file).unwrap_err();
        assert!(err.contains("isn't a Quill emoji set"));
        assert!(install_file(&root, &root.join("missing.zip")).is_err());
        assert!(!set_dir(&root, EmojiSetId::Noto).exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The real packs, built by `quill-tools/emoji-sets`: set
/// `QUILL_EMOJI_PACKS` to the folder with the zips and run with
/// `--ignored`.
#[cfg(test)]
mod built_packs {
    use super::*;

    #[test]
    #[ignore = "needs the built packs"]
    fn built_packs_match_the_catalog_and_install() {
        let Ok(dir) = std::env::var("QUILL_EMOJI_PACKS") else {
            return;
        };
        let root = std::env::temp_dir().join(format!("quill-emoji-built-{}", std::process::id()));
        for id in EmojiSetId::ALL.into_iter().filter(|id| !id.is_system()) {
            let zip = std::fs::read(Path::new(&dir).join(format!("emoji-set-{}.zip", id.key())))
                .expect("pack");
            assert_eq!(zip.len() as u64, id.bytes(), "{id:?} size");
            let file = Path::new(&dir).join(format!("emoji-set-{}.zip", id.key()));
            assert_eq!(install_file(&root, &file), Ok(id), "{id:?} from a file");
            let pack = super::super::pack::SpritePack::load(&set_dir(&root, id)).expect("load");
            assert!(pack.len() >= 3900, "{id:?} has {} emoji", pack.len());
            for emoji in ["😀", "❤️", "👍🏽", "🇺🇸", "👨‍👩‍👧", "1️⃣"]
            {
                assert!(pack.lookup(emoji).is_some(), "{id:?} lacks {emoji}");
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
