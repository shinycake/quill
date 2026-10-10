//! Pinning a participant's video tile in a group call.
//!
//! tdesktop (`calls/group/calls_group_viewport*.cpp`,
//! `Viewport::setVideoEndpointPinned`): pinning is local UI state. The
//! pinned stream (camera or screen of one participant) becomes the large
//! tile and the others shrink to a strip. If the pinned stream disappears
//! the pin is dropped, so a later stream from the same person does not
//! surprise the viewer.

use crate::telegram::envelope::MessageSender;

/// One video stream of a participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileKey {
    pub participant: MessageSender,
    /// Screen share (true) or camera (false).
    pub screen: bool,
}

/// Local pin state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TilePin {
    pinned: Option<TileKey>,
}

/// Tiles split into the enlarged one and the rest, order preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileLayout {
    pub main: Option<TileKey>,
    pub rest: Vec<TileKey>,
}

impl TilePin {
    pub fn pinned(&self) -> Option<TileKey> {
        self.pinned
    }

    pub fn is_pinned(&self, key: TileKey) -> bool {
        self.pinned == Some(key)
    }

    /// Pin `key`, or unpin it when it is already the pinned one.
    pub fn toggle(&mut self, key: TileKey) {
        self.pinned = if self.pinned == Some(key) {
            None
        } else {
            Some(key)
        };
    }

    pub fn clear(&mut self) {
        self.pinned = None;
    }

    /// Drop the pin when its stream is no longer available.
    pub fn retain_available(&mut self, available: &[TileKey]) {
        if let Some(pinned) = self.pinned
            && !available.contains(&pinned)
        {
            self.pinned = None;
        }
    }

    /// Split `tiles`: the pinned one (if present) leads, the rest keep order.
    pub fn layout(&self, tiles: &[TileKey]) -> TileLayout {
        match self.pinned.filter(|pinned| tiles.contains(pinned)) {
            Some(main) => TileLayout {
                main: Some(main),
                rest: tiles.iter().copied().filter(|t| *t != main).collect(),
            },
            None => TileLayout {
                main: None,
                rest: tiles.to_vec(),
            },
        }
    }
}

/// The tile the full-screen stage shows: the pinned one, only while its
/// stream is still there. tdesktop fills the whole window with the video
/// on a black background (`calls_group_viewport*.cpp`, `_fullscreen`).
pub fn stage_tile(pin: &TilePin, tiles: &[TileKey]) -> Option<TileKey> {
    pin.layout(tiles).main
}

/// Whether the window shows the stage instead of the member list: it must
/// be full screen and have a pinned stream to show.
pub fn stage_active(fullscreen: bool, stage: Option<TileKey>) -> bool {
    fullscreen && stage.is_some()
}

/// Escape leaves full screen (tdesktop `Panel::toggleFullScreen` on Esc).
pub fn exits_fullscreen(key: &str) -> bool {
    key.eq_ignore_ascii_case("escape")
}

#[cfg(test)]
mod tests {
    use super::{TileKey, TilePin, exits_fullscreen, stage_active, stage_tile};
    use crate::telegram::envelope::MessageSender;

    fn key(user_id: i64, screen: bool) -> TileKey {
        TileKey {
            participant: MessageSender::User { user_id },
            screen,
        }
    }

    #[test]
    fn toggle_pins_and_unpins() {
        let mut pin = TilePin::default();
        pin.toggle(key(1, false));
        assert!(pin.is_pinned(key(1, false)));
        assert!(!pin.is_pinned(key(1, true)), "camera and screen differ");
        pin.toggle(key(2, true));
        assert_eq!(pin.pinned(), Some(key(2, true)), "one pin at a time");
        pin.toggle(key(2, true));
        assert_eq!(pin.pinned(), None);
    }

    #[test]
    fn layout_puts_the_pinned_tile_first() {
        let mut pin = TilePin::default();
        let tiles = [key(1, false), key(2, false), key(3, true)];
        assert_eq!(pin.layout(&tiles).main, None);
        assert_eq!(pin.layout(&tiles).rest, tiles.to_vec());
        pin.toggle(key(2, false));
        let layout = pin.layout(&tiles);
        assert_eq!(layout.main, Some(key(2, false)));
        assert_eq!(layout.rest, vec![key(1, false), key(3, true)]);
    }

    #[test]
    fn a_vanished_stream_drops_the_pin() {
        let mut pin = TilePin::default();
        pin.toggle(key(2, true));
        pin.retain_available(&[key(1, false), key(2, true)]);
        assert!(pin.is_pinned(key(2, true)));
        pin.retain_available(&[key(1, false)]);
        assert_eq!(pin.pinned(), None);
        pin.retain_available(&[key(2, true)]);
        assert_eq!(pin.pinned(), None, "does not come back by itself");
    }

    #[test]
    fn layout_ignores_a_pin_that_is_not_shown() {
        let mut pin = TilePin::default();
        pin.toggle(key(9, false));
        let tiles = [key(1, false)];
        assert_eq!(pin.layout(&tiles).main, None);
        assert_eq!(pin.layout(&tiles).rest, tiles.to_vec());
    }

    #[test]
    fn the_stage_needs_full_screen_and_a_pinned_stream() {
        let mut pin = TilePin::default();
        let tiles = [key(1, false), key(2, true)];
        assert_eq!(stage_tile(&pin, &tiles), None);
        assert!(!stage_active(true, stage_tile(&pin, &tiles)));
        pin.toggle(key(2, true));
        let stage = stage_tile(&pin, &tiles);
        assert_eq!(stage, Some(key(2, true)));
        assert!(stage_active(true, stage));
        assert!(!stage_active(false, stage), "windowed shows the list");
        assert_eq!(stage_tile(&pin, &[key(1, false)]), None, "stream gone");
    }

    #[test]
    fn only_escape_leaves_full_screen() {
        assert!(exits_fullscreen("escape"));
        assert!(exits_fullscreen("Escape"));
        assert!(!exits_fullscreen("f"));
        assert!(!exits_fullscreen("enter"));
    }
}
