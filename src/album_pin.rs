//! The album pin action in the viewer's header: Pin or Unpin by whether any
//! member of the album is pinned (`MessagePinAlbum`). Answering it scans
//! the loaded history, so the answer is kept for as long as the history,
//! the album and the right to pin are unchanged, and the chrome (which
//! redraws as often as every 100 ms while a clip plays) does not scan a long
//! history on every redraw.
#![deny(clippy::indexing_slicing)]

use crate::state::HistoryMessages;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlbumPin {
    /// Not an album the user may pin, or no member is loaded.
    None,
    Pin,
    Unpin,
}

/// What the answer was computed for: chat, album, the history instance and
/// revision, and whether the user may pin.
type Basis = (i64, i64, u64, u64, bool);

#[derive(Default)]
pub enum AlbumPins {
    #[default]
    Unasked,
    Answered(Basis, AlbumPin),
}

impl AlbumPins {
    pub fn action(
        &mut self,
        chat_id: i64,
        album_id: i64,
        can_pin: bool,
        history: &HistoryMessages,
    ) -> AlbumPin {
        let basis = (
            chat_id,
            album_id,
            history.instance(),
            history.revision(),
            can_pin,
        );
        if let Self::Answered(seen, pin) = self
            && *seen == basis
        {
            return *pin;
        }
        let pin = if can_pin {
            of_history(history, album_id)
        } else {
            AlbumPin::None
        };
        *self = Self::Answered(basis, pin);
        pin
    }
}

fn of_history(history: &HistoryMessages, album_id: i64) -> AlbumPin {
    if album_id == 0 {
        return AlbumPin::None;
    }
    let mut members = history
        .values()
        .filter(|message| message.media_album_id == album_id)
        .peekable();
    if members.peek().is_none() {
        return AlbumPin::None;
    }
    if members.any(|message| message.is_pinned) {
        AlbumPin::Unpin
    } else {
        AlbumPin::Pin
    }
}

#[cfg(test)]
#[path = "album_pin_tests.rs"]
mod tests;
