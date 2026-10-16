//! A panic on the decode thread must end in the player's error state.

use super::{Scripted, wait_until};
use crate::video_decode::{Demuxer, Item, MediaInfo, OpenOptions, PixelOutput, Player, lock};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// A demuxer that panics on its Nth call (the test-only panic hook).
struct PanicsAt {
    inner: Scripted,
    left: usize,
}

impl Demuxer for PanicsAt {
    fn info(&self) -> MediaInfo {
        self.inner.info()
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        if self.left == 0 {
            panic!("injected decode panic");
        }
        self.left -= 1;
        self.inner.next()
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        self.inner.seek(secs)
    }
}

fn panicking_player(after: usize, audio: bool) -> Player {
    let calls = Arc::new(AtomicUsize::new(0));
    let demuxer = PanicsAt {
        inner: Scripted::new(25.0, None, audio, calls),
        left: after,
    };
    let options = OpenOptions {
        max_width: 0,
        max_height: 0,
        threads: 1,
        want_audio: audio,
        video_frames: 4,
        output: PixelOutput::Bgra,
    };
    Player::with_demuxer(Box::new(demuxer), options).expect("spawn the decode thread")
}

#[test]
fn a_decode_panic_puts_the_player_in_its_error_state() {
    let mut player = panicking_player(3, true);
    assert!(wait_until(|| player.error().is_some()));
    // The queue and the sound end are released with the thread.
    assert!(player.take_audio_tap().is_none());
    assert!(player.take_frame().is_none());
    // The guard caught the panic: the thread ended and joins at once.
    assert_eq!(player.join_thread(), Some(false));
    // Playing on does not hang or panic.
    player.play();
    assert!(player.error().is_some());
}

#[test]
fn dropping_a_player_mid_decode_neither_errors_nor_hangs() {
    let player = panicking_player(usize::MAX, false);
    let shared = player.shared.clone();
    std::thread::sleep(Duration::from_millis(20));
    drop(player);
    std::thread::sleep(Duration::from_millis(50));
    assert!(lock(&shared.state).error.is_none());
}
