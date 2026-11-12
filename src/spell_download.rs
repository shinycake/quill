//! Downloads a spelling dictionary for the manager
//! (`spell_catalog`; tdesktop `Spellchecker::DictLoader`).
//!
//! Blocking and bounded: the UI runs it on the background executor and
//! reads [`Progress`] to draw the row's percentage and to cancel
//! (tdesktop: switching a downloading dictionary off destroys the loader).
#![deny(clippy::indexing_slicing)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use crate::spell_catalog::DOWNLOAD_HOST;

/// Where the download is. It only moves forward, so a stop request and the
/// worker's completion cannot disagree: a download that finished after a
/// stop was asked for reads `Cancelled`, not `Done`. The worker's end is the
/// [`Worker`] it holds: the stage sees it drop (return, panic, or the job
/// never started) through `ended`.
#[derive(Debug)]
enum DownloadStage {
    Running {
        ended: Receiver<()>,
    },
    /// The UI asked to stop; the worker ends at the next chunk.
    Cancelling {
        ended: Receiver<()>,
    },
    /// The worker finished without a stop request.
    Done,
    /// The worker finished after a stop request.
    Cancelled,
}

impl DownloadStage {
    /// Moves on to `Done` or `Cancelled` once the worker's end is gone.
    fn settle(&mut self) {
        let (Self::Running { ended } | Self::Cancelling { ended }) = self else {
            return;
        };
        if matches!(ended.try_recv(), Err(TryRecvError::Empty)) {
            return;
        }
        *self = match self {
            Self::Running { .. } => Self::Done,
            _ => Self::Cancelled,
        };
    }
}

/// The worker's end of a download: dropping it marks the download finished,
/// however the worker ends.
#[derive(Debug)]
pub struct Worker {
    _end: Sender<()>,
}

/// Shared between the download thread and the UI.
#[derive(Debug)]
pub struct Progress {
    /// Bytes received over both files.
    pub received: AtomicU64,
    stage: Mutex<DownloadStage>,
}

impl Progress {
    /// A download that has begun: the progress for the UI to read and the
    /// [`Worker`] the thread holds until it ends.
    pub fn start() -> (std::sync::Arc<Self>, Worker) {
        let (end, ended) = channel();
        let progress = Self {
            received: AtomicU64::new(0),
            stage: Mutex::new(DownloadStage::Running { ended }),
        };
        (std::sync::Arc::new(progress), Worker { _end: end })
    }

    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    fn settled<R>(&self, read: impl FnOnce(&mut DownloadStage) -> R) -> R {
        let mut stage = crate::locks::lock_or_recover(&self.stage);
        stage.settle();
        read(&mut stage)
    }

    /// Asks the worker to stop; a finished download stays finished.
    pub fn cancel(&self) {
        self.settled(|stage| {
            *stage = match std::mem::replace(stage, DownloadStage::Done) {
                DownloadStage::Running { ended } => DownloadStage::Cancelling { ended },
                other => other,
            };
        });
    }

    /// A stop was asked for, whether or not the worker has finished.
    pub fn is_cancelled(&self) -> bool {
        self.settled(|stage| {
            matches!(
                stage,
                DownloadStage::Cancelling { .. } | DownloadStage::Cancelled
            )
        })
    }

    pub fn is_finished(&self) -> bool {
        self.settled(|stage| matches!(stage, DownloadStage::Done | DownloadStage::Cancelled))
    }
}

/// Only HTTPS URLs on the dictionary host are fetched.
pub fn is_allowed_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|host| host == DOWNLOAD_HOST)
}

/// Why a download ended without installing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// The user cancelled (or switched it off); nothing to show.
    Cancelled,
    /// The text the row shows.
    Message(String),
}

impl From<&str> for DownloadError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

#[cfg(feature = "ui")]
fn fetch(url: &str, progress: &Progress) -> Result<Vec<u8>, DownloadError> {
    use std::io::Read;

    if !is_allowed_url(url) {
        return Err("Unexpected dictionary address.".into());
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
        .map_err(|_| {
            DownloadError::from(
                "Couldn't reach the dictionary server. Check your connection and retry.",
            )
        })?;
    let limit = crate::spell_catalog::MAX_FILE_BYTES;
    let mut reader = response.body_mut().as_reader().take(limit + 1);
    let mut out = Vec::new();
    let mut chunk = [0u8; 32 * 1024];
    loop {
        if progress.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let n = reader
            .read(&mut chunk)
            .map_err(|_| DownloadError::from("The download was interrupted. Retry."))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(chunk.get(..n).unwrap_or_default());
        progress.received.fetch_add(n as u64, Ordering::Relaxed);
        if out.len() as u64 > limit {
            return Err("The dictionary is larger than expected.".into());
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
) -> Result<(), DownloadError> {
    let entry = crate::spell_catalog::entry(code).ok_or("Unknown dictionary.")?;
    let aff = fetch(&crate::spell_catalog::download_url(entry, "aff"), progress)?;
    let dic = fetch(&crate::spell_catalog::download_url(entry, "dic"), progress)?;
    if progress.is_cancelled() {
        return Err(DownloadError::Cancelled);
    }
    crate::spell_catalog::install(dir, code, &aff, &dic)
        .map_err(|e| DownloadError::Message(e.to_string()))
}

/// The text a row shows when its worker panicked.
pub const WORKER_PANICKED: &str = "The download stopped unexpectedly.";

/// Runs the download job on the worker. A panic in it comes back as an
/// `Err` (the row shows it) instead of unwinding into whoever awaits the
/// worker, and the download is finished either way (the [`Worker`] drops).
pub fn run_guarded(
    worker: Worker,
    job: impl FnOnce() -> Result<(), DownloadError>,
) -> Result<(), DownloadError> {
    let _done = worker;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
        .unwrap_or_else(|_| Err(DownloadError::Message(WORKER_PANICKED.to_string())))
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
        let (p, worker) = Progress::start();
        assert!(!p.is_cancelled() && !p.is_finished());
        p.received.fetch_add(5, Ordering::Relaxed);
        p.cancel();
        assert!(p.is_cancelled() && !p.is_finished());
        drop(worker);
        assert!(p.is_cancelled() && p.is_finished(), "a cancelled finish");
        assert_eq!(p.received(), 5);
    }

    #[test]
    fn a_finished_download_stays_finished_when_cancel_comes_late() {
        let (p, worker) = Progress::start();
        drop(worker);
        p.cancel();
        assert!(!p.is_cancelled() && p.is_finished());
    }

    #[test]
    fn a_worker_that_never_ran_still_finishes_the_download() {
        let (p, worker) = Progress::start();
        let job = move || drop(worker);
        drop(job);
        assert!(p.is_finished());
    }

    #[test]
    fn a_panicking_worker_finishes_and_returns_an_error() {
        let (p, worker) = Progress::start();
        let outcome = run_guarded(worker, || {
            std::panic::resume_unwind(Box::new("worker failed"))
        });
        assert_eq!(
            outcome,
            Err(DownloadError::Message(WORKER_PANICKED.to_string()))
        );
        assert!(p.is_finished());
    }

    #[test]
    fn a_worker_result_passes_through() {
        let (p, worker) = Progress::start();
        assert_eq!(run_guarded(worker, || Ok(())), Ok(()));
        assert!(p.is_finished());
        let (p, worker) = Progress::start();
        assert_eq!(
            run_guarded(worker, || Err(DownloadError::Cancelled)),
            Err(DownloadError::Cancelled)
        );
        assert!(p.is_finished());
    }
}
