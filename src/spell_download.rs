//! Downloads a spelling dictionary for the manager
//! (`spell_catalog`; tdesktop `Spellchecker::DictLoader`).
//!
//! Blocking and bounded: the UI runs it on the background executor and
//! reads [`Progress`] to draw the row's percentage and to cancel
//! (tdesktop: switching a downloading dictionary off destroys the loader).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::spell_catalog::DOWNLOAD_HOST;

/// Shared between the download thread and the UI.
#[derive(Debug, Default)]
pub struct Progress {
    /// Bytes received over both files.
    pub received: AtomicU64,
    /// Set by the UI to stop at the next chunk.
    pub cancelled: AtomicBool,
    /// Set when the download thread has finished, either way.
    pub finished: AtomicBool,
}

impl Progress {
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }
}

/// Only HTTPS URLs on the dictionary host are fetched.
pub fn is_allowed_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|host| host == DOWNLOAD_HOST)
}

#[cfg(feature = "ui")]
fn fetch(url: &str, progress: &Progress) -> Result<Vec<u8>, &'static str> {
    use std::io::Read;

    if !is_allowed_url(url) {
        return Err("Unexpected dictionary address.");
    }
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(180)))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .header("User-Agent", format!("Quill/{}", crate::version::APP))
        .call()
        .map_err(|_| "Couldn't reach the dictionary server. Check your connection and retry.")?;
    let limit = crate::spell_catalog::MAX_FILE_BYTES;
    let mut reader = response.body_mut().as_reader().take(limit + 1);
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
        out.extend_from_slice(&chunk[..n]);
        progress.received.fetch_add(n as u64, Ordering::Relaxed);
        if out.len() as u64 > limit {
            return Err("The dictionary is larger than expected.");
        }
    }
    Ok(out)
}

/// Download `code`'s `.aff` and `.dic`, vet them and install them into
/// `dir`. The error text is shown on the row.
#[cfg(feature = "ui")]
pub fn download_dictionary(
    dir: &std::path::Path,
    code: &str,
    progress: &Progress,
) -> Result<(), String> {
    let entry = crate::spell_catalog::entry(code).ok_or("Unknown dictionary.")?;
    let aff = fetch(&crate::spell_catalog::download_url(entry, "aff"), progress)?;
    let dic = fetch(&crate::spell_catalog::download_url(entry, "dic"), progress)?;
    if progress.is_cancelled() {
        return Err("Cancelled.".to_string());
    }
    crate::spell_catalog::install(dir, code, &aff, &dic).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_on_the_dictionary_host() {
        let ok = format!("https://{DOWNLOAD_HOST}/wooorm/dictionaries/x/index.dic");
        assert!(is_allowed_url(&ok));
        assert!(!is_allowed_url(&ok.replace("https", "http")));
        assert!(!is_allowed_url(
            "https://evil.example/raw.githubusercontent.com/"
        ));
        assert!(!is_allowed_url(&format!(
            "https://{DOWNLOAD_HOST}.evil.example/x"
        )));
        assert!(!is_allowed_url(&format!("https://user@{DOWNLOAD_HOST}/x")));
    }

    #[test]
    fn catalog_urls_are_allowed() {
        for e in crate::spell_catalog::CATALOG {
            for ext in ["aff", "dic"] {
                assert!(is_allowed_url(&crate::spell_catalog::download_url(e, ext)));
            }
        }
    }

    #[test]
    fn progress_flags() {
        let p = Progress::default();
        assert!(!p.is_cancelled() && !p.is_finished());
        p.received.fetch_add(5, Ordering::Relaxed);
        p.cancel();
        p.finish();
        assert!(p.is_cancelled() && p.is_finished());
        assert_eq!(p.received(), 5);
    }
}
