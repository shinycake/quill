//! Where the user is, from the platform's location services (tdesktop
//! `Core::ResolveCurrentExactLocation`, `current_geo_location_*`), the
//! address of a point where the platform has a keyless geocoder, and the
//! system country the map starts in without a fix.
//!
//! Each lookup answers once on a channel; the picker awaits it from a UI
//! task, which the answer wakes, so nothing here blocks the UI thread.
#![deny(clippy::indexing_slicing)]

use quill::location_picker::{GeoOutcome, GeoPoint};
use std::sync::Arc;
use std::sync::Mutex;

/// The receiving end of a lookup: it resolves once with the answer, or
/// with `Canceled` when every [`Answer`] went away without one.
pub(in crate::ui) type Receiver<T> = futures_channel::oneshot::Receiver<T>;

/// The sending end of a lookup. Cloneable (a platform keeps a spare for a
/// failed thread spawn); the first `send` answers, later ones are refused.
pub(in crate::ui) struct Answer<T>(
    std::sync::Arc<Mutex<Option<futures_channel::oneshot::Sender<T>>>>,
);

impl<T> Clone for Answer<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Answer<T> {
    /// Answer the lookup; hands `value` back when it was answered already or
    /// the receiver is gone.
    pub(in crate::ui) fn send(&self, value: T) -> Result<(), T> {
        let Ok(mut slot) = self.0.lock() else {
            return Err(value);
        };
        match slot.take() {
            Some(sender) => sender.send(value),
            None => Err(value),
        }
    }
}

/// A lookup's two ends.
pub(in crate::ui) fn channel<T>() -> (Answer<T>, Receiver<T>) {
    let (sender, receiver) = futures_channel::oneshot::channel();
    (
        Answer(std::sync::Arc::new(Mutex::new(Some(sender)))),
        receiver,
    )
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// Whether the system's "Allow location" prompt is open: the user's turn,
/// not the lookup's. Closing it wakes the task that waits for the lookup, so
/// the wait starts its full limit again from that moment.
#[derive(Default)]
pub(in crate::ui) struct PromptGate {
    state: Mutex<PromptState>,
}

#[derive(Default)]
struct PromptState {
    open: bool,
    /// How many times the prompt closed.
    closed: u64,
    waker: Option<std::task::Waker>,
}

impl PromptGate {
    /// The prompt opened or closed; a close wakes the waiting task.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(in crate::ui) fn set_open(&self, open: bool) {
        let woken = {
            let mut state = quill::locks::lock_or_recover(&self.state);
            let was = std::mem::replace(&mut state.open, open);
            if was && !open {
                state.closed = state.closed.wrapping_add(1);
                state.waker.take()
            } else {
                None
            }
        };
        if let Some(waker) = woken {
            waker.wake();
        }
    }

    pub(in crate::ui) fn is_open(&self) -> bool {
        quill::locks::lock_or_recover(&self.state).open
    }

    /// How many times the prompt has closed so far.
    pub(in crate::ui) fn closed(&self) -> u64 {
        quill::locks::lock_or_recover(&self.state).closed
    }

    /// Ready once the prompt has closed since `seen` ([`Self::closed`]).
    pub(in crate::ui) fn poll_closed(
        &self,
        seen: u64,
        task: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        let mut state = quill::locks::lock_or_recover(&self.state);
        if state.closed != seen {
            return std::task::Poll::Ready(());
        }
        state.waker = Some(task.waker().clone());
        std::task::Poll::Pending
    }
}

/// A running "where am I" lookup: its answer, and whether the system's
/// permission prompt is open (the user's turn, not the lookup's: its time
/// limit does not run meanwhile).
pub(in crate::ui) struct Locating {
    pub(in crate::ui) answer: Receiver<GeoOutcome>,
    pub(in crate::ui) permission_prompt: Arc<PromptGate>,
    /// Dropping it (with the `Locating`) ends the Linux and Windows
    /// lookup thread's wait; macOS stops its manager in `cancel`.
    _cancel: futures_channel::oneshot::Sender<()>,
}

impl Drop for Locating {
    /// Dropping the lookup (the picker closed, or it timed out) stops the
    /// platform's location manager instead of leaving it running.
    fn drop(&mut self) {
        platform::cancel(&self.permission_prompt);
    }
}

/// One "where am I" lookup. Call on the UI thread (CoreLocation answers on
/// the run loop of the thread that asked).
pub(in crate::ui) fn locate() -> Locating {
    let permission_prompt = Arc::new(PromptGate::default());
    let (cancel, cancelled) = futures_channel::oneshot::channel();
    let answer = platform::locate(&permission_prompt, cancelled);
    Locating {
        answer,
        permission_prompt,
        _cancel: cancel,
    }
}

/// The address of `point`, where the platform geocodes without a key
/// (CoreLocation's `CLGeocoder`); `None` elsewhere, and the picker shows the
/// coordinates, as tdesktop does without its Mapbox token.
pub(in crate::ui) fn reverse_geocode(point: GeoPoint) -> Option<Receiver<Option<String>>> {
    platform::reverse_geocode(point)
}

/// tdesktop `Platform::SystemCountry`: the ISO 3166 alpha-2 code of the
/// system region, upper-cased.
pub(in crate::ui) fn system_country() -> Option<String> {
    platform::system_country()
        .map(|code| code.trim().to_ascii_uppercase())
        .filter(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
}

/// The region of a POSIX locale name such as `en_US.UTF-8` or `pt_BR@euro`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn locale_region(locale: &str) -> Option<String> {
    let name = locale.split(['.', '@']).next()?;
    let (_, region) = name.split_once(['_', '-'])?;
    Some(region.to_string())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn join_address(parts: &[Option<String>]) -> Option<String> {
    let joined = parts
        .iter()
        .flatten()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::{join_address, locale_region};

    #[test]
    fn locale_names_give_their_region() {
        assert_eq!(locale_region("en_US.UTF-8").as_deref(), Some("US"));
        assert_eq!(locale_region("pt_BR@euro").as_deref(), Some("BR"));
        assert_eq!(locale_region("de-AT").as_deref(), Some("AT"));
        assert_eq!(locale_region("C"), None);
    }

    #[test]
    fn addresses_skip_missing_parts() {
        let part = |s: &str| Some(s.to_string());
        assert_eq!(
            join_address(&[part("Rue de Rivoli"), None, part("France")]).as_deref(),
            Some("Rue de Rivoli, France")
        );
        assert_eq!(join_address(&[None, part(" ")]), None);
    }
}
