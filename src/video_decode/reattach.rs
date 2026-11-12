//! A fresh audio tap for a player whose output let go of the sound.

use super::*;

impl AudioShared {
    pub(super) fn new(rate: u32, channels: u16) -> Self {
        Self {
            rate,
            channels: channels.max(1),
            queue: Mutex::new(AudioQueue::default()),
            generation: AtomicU64::new(0),
            origin: AtomicU64::new(0f64.to_bits()),
            played: AtomicU64::new(PlayedMark::NOTHING.0),
            starved: AtomicBool::new(true),
        }
    }
}

impl AudioTap {
    pub(super) fn new(shared: Arc<AudioShared>, wake: Arc<Shared>) -> Self {
        Self {
            shared,
            wake,
            chunk: Vec::new(),
            at: 0,
            chunk_pts: 0.0,
            end_pts: 0.0,
            pts_shift: 0.0,
            gap: 0,
            gap_at: 0,
            generation: 0,
        }
    }
}

impl Player {
    /// The output let go of the sound and no tap is out: the one case
    /// [`Self::reattach_audio_tap`] answers. False for a clip with no
    /// soundtrack (or one not asked for), before the first tap was taken,
    /// and while a tap is attached. Touches nothing, so a caller can ask
    /// every frame.
    pub fn sound_detached(&self) -> bool {
        let state = lock(&self.shared.state);
        // A tap that is out (still held) has an open queue, so the queue's
        // own life says it all.
        state
            .audio
            .as_ref()
            .is_some_and(|audio| audio.is_detached())
    }

    /// The output let go of the sound (no device at attach, or the device
    /// was lost): a new tap for the same sound, for an output that is up
    /// again, and the clip restarts where it is so the two stay in step
    /// (what was decoded meanwhile went to nobody). `None` while a tap is
    /// out and attached, before the first one was taken, or with no sound.
    pub fn reattach_audio_tap(&mut self) -> Option<AudioTap> {
        let position = self.position();
        let tap = {
            let mut state = lock(&self.shared.state);
            let old = state.audio.as_ref()?;
            if !old.is_detached() {
                return None;
            }
            let audio = Arc::new(AudioShared::new(old.rate, old.channels));
            state.audio = Some(audio.clone());
            AudioTap::new(audio, self.shared.clone())
        };
        self.seek(position);
        Some(tap)
    }
}
