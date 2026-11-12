//! A call's sound output. tgcalls has no devices, so Quill plays the call's
//! sound through its own output, which is opened, held and closed on a thread
//! of its own: a device open takes about 100 ms and much longer on Bluetooth,
//! and the engine runs on the UI thread. Frames are dropped until it opens.
use super::reaper::ReaperHold;
use crate::calls::capture::CaptureError;
use crate::calls::capture::device_thread::DeviceThread;
use crate::calls::stream_audio::{StreamAudio, StreamPlayer};
use crate::locks::LockExt;
use arc_swap::{ArcSwapOption, Guard};
use std::sync::{Arc, Mutex};

/// Where the output is. The slot owns the device's thread, so the open
/// that may still land and the thread it runs on are one value: dropping or
/// replacing the stage closes the device and an open still running finds
/// no token of its own to land under.
enum Stage {
    /// No output: before the first open, after `clear`, or after the open failed.
    Closed,
    /// The open runs on `thread`; only the open holding `token` may land.
    Opening {
        thread: DeviceThread,
        token: Arc<()>,
    },
    /// The open answered and `audio` plays on `thread`'s device.
    Open { _thread: DeviceThread },
}

/// What the output should be right now. Kept here so that a choice made
/// while the device opens reaches it.
struct Wanted {
    stage: Stage,
    volume: f32,
    muted: bool,
}

pub(super) struct SoundSlot {
    /// The frames' way in, read without the lock: the view of the `Open`
    /// stage, stored and cleared only together with it under `wanted`.
    audio: ArcSwapOption<StreamAudio>,
    wanted: Mutex<Wanted>,
}

impl Default for SoundSlot {
    fn default() -> Self {
        Self {
            audio: ArcSwapOption::empty(),
            wanted: Mutex::new(Wanted {
                stage: Stage::Closed,
                volume: 1.0,
                muted: false,
            }),
        }
    }
}

impl SoundSlot {
    /// Where the sound goes now; `None` drops it.
    pub(super) fn audio(&self) -> Guard<Option<Arc<StreamAudio>>> {
        self.audio.load()
    }

    /// Stops the sound at once and closes the device on its thread; an open
    /// still running will not land.
    pub(super) fn clear(&self) {
        let mut wanted = self.wanted.lock_or_recover();
        wanted.stage = Stage::Closed;
        self.audio.store(None);
    }

    /// The local mute the output is held at, the one record of it.
    pub(super) fn muted(&self) -> bool {
        self.wanted.lock_or_recover().muted
    }

    pub(super) fn set_muted(&self, muted: bool) {
        let mut wanted = self.wanted.lock_or_recover();
        wanted.muted = muted;
        if let Some(audio) = self.audio.load().as_ref() {
            audio.gain().set_muted(muted);
        }
    }

    pub(super) fn set_volume(&self, volume: f32) {
        let mut wanted = self.wanted.lock_or_recover();
        wanted.volume = volume;
        if let Some(audio) = self.audio.load().as_ref() {
            audio.gain().set_volume(volume);
        }
    }

    /// Opens the output named `device` in place of the current one, which
    /// closes on its thread; `hold` keeps quit waiting for it. A failed open leaves the call silent and logs why, under
    /// `label`.
    pub(super) fn open(
        self: &Arc<Self>,
        label: String,
        device: Option<String>,
        volume: f32,
        muted: bool,
        hold: ReaperHold,
    ) {
        self.open_with(label, volume, muted, hold, move |volume, muted| {
            let player = StreamPlayer::open(device.as_deref(), volume, muted)?;
            let audio = player.audio().clone();
            Ok((player, audio))
        })
    }

    fn open_with<D: 'static>(
        self: &Arc<Self>,
        label: String,
        volume: f32,
        muted: bool,
        hold: ReaperHold,
        open: impl FnOnce(f32, bool) -> Result<(D, Arc<StreamAudio>), String> + Send + 'static,
    ) {
        let token = Arc::new(());
        let landing = token.clone();
        let slot = Arc::clone(self);
        // Held until the stage owns the thread, so the open cannot land
        // before there is a stage for it to land in.
        let mut wanted = self.wanted.lock_or_recover();
        let thread = DeviceThread::open(
            "quill-call-sound",
            &crate::wake::UPDATES,
            move || match open(volume, muted) {
                Ok((device, audio)) => {
                    slot.publish(&landing, audio);
                    Ok(device)
                }
                Err(err) => {
                    slot.open_failed(&landing);
                    crate::advanced::debug_mode::call_log(&format!(
                        "[call] {label} sound output did not open ({err})"
                    ));
                    Err(CaptureError::Device(err))
                }
            },
            move |device| {
                drop(device);
                drop(hold);
            },
        );
        wanted.volume = volume;
        wanted.muted = muted;
        self.audio.store(None);
        wanted.stage = Stage::Opening { thread, token };
    }

    fn publish(&self, token: &Arc<()>, audio: Arc<StreamAudio>) {
        let mut wanted = self.wanted.lock_or_recover();
        match std::mem::replace(&mut wanted.stage, Stage::Closed) {
            Stage::Opening { thread, token: own } if Arc::ptr_eq(&own, token) => {
                audio.gain().set_volume(wanted.volume);
                audio.gain().set_muted(wanted.muted);
                self.audio.store(Some(audio));
                wanted.stage = Stage::Open { _thread: thread };
            }
            other => wanted.stage = other,
        }
    }

    /// The open holding `token` failed: nothing is opening any more. An open
    /// that was replaced meanwhile leaves the newer stage alone.
    fn open_failed(&self, token: &Arc<()>) {
        let mut wanted = self.wanted.lock_or_recover();
        if matches!(&wanted.stage, Stage::Opening { token: own, .. } if Arc::ptr_eq(own, token)) {
            wanted.stage = Stage::Closed;
        }
    }

    /// How the current open went; `None` while it runs or when no output is
    /// wanted.
    #[cfg(test)]
    fn opened(&self) -> Option<Result<(), CaptureError>> {
        match &self.wanted.lock_or_recover().stage {
            Stage::Closed => None,
            Stage::Opening { thread, .. } | Stage::Open { _thread: thread } => thread.opened(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::engine::tgcalls::reaper::Reaper;
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// A device whose drop says it closed.
    struct Device(Sender<()>);

    impl Drop for Device {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    struct Held {
        release: Sender<()>,
        closed: Receiver<()>,
        audio: Arc<StreamAudio>,
        reaper: Reaper,
    }

    /// An open that blocks until released.
    fn blocked_open(slot: &Arc<SoundSlot>, volume: f32, muted: bool) -> Held {
        let (release, released) = channel::<()>();
        let (closes, closed) = channel();
        let audio = Arc::new(StreamAudio::new(48_000, 1.0, false));
        let opened = audio.clone();
        let reaper = Reaper::start();
        let hold = reaper.hold();
        slot.open_with("test".into(), volume, muted, hold, move |_, _| {
            let _ = released.recv();
            Ok((Device(closes), opened))
        });
        Held {
            release,
            closed,
            audio,
            reaper,
        }
    }

    fn wait_opened(slot: &SoundSlot) -> Result<(), CaptureError> {
        loop {
            if let Some(answer) = slot.opened() {
                return answer;
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_slow_output_holds_nobody_and_lands_when_it_opens() {
        let slot = Arc::new(SoundSlot::default());
        let open = blocked_open(&slot, 1.0, false);
        assert!(slot.audio().is_none(), "frames drop until it opens");
        let _ = open.release.send(());
        assert!(wait_opened(&slot).is_ok());
        let current = slot.audio();
        let current = current.as_ref().expect("the output landed");
        assert!(Arc::ptr_eq(current, &open.audio));
    }

    #[test]
    fn an_output_that_opens_after_the_call_ended_closes_and_never_lands() {
        let slot = Arc::new(SoundSlot::default());
        let open = blocked_open(&slot, 1.0, false);
        slot.clear();
        let _ = open.release.send(());
        assert_eq!(open.closed.recv(), Ok(()), "closed on its thread");
        assert!(slot.audio().is_none());
    }

    #[test]
    fn a_reopen_outruns_the_open_it_replaces() {
        let slot = Arc::new(SoundSlot::default());
        let first = blocked_open(&slot, 1.0, false);
        let second = blocked_open(&slot, 1.0, false);
        let _ = second.release.send(());
        assert!(wait_opened(&slot).is_ok());
        let _ = first.release.send(());
        assert_eq!(first.closed.recv(), Ok(()), "the replaced open closes");
        let current = slot.audio();
        let current = current.as_ref().expect("the output landed");
        assert!(Arc::ptr_eq(current, &second.audio));
    }

    #[test]
    fn a_mute_and_a_volume_set_while_it_opens_reach_it() {
        let slot = Arc::new(SoundSlot::default());
        let open = blocked_open(&slot, 1.0, false);
        slot.set_muted(true);
        slot.set_volume(0.25);
        let _ = open.release.send(());
        assert!(wait_opened(&slot).is_ok());
        let current = slot.audio();
        let current = current.as_ref().expect("the output landed");
        assert!(current.gain().muted());
        assert_eq!(current.gain().volume(), 0.25);
    }

    #[test]
    fn a_failed_open_leaves_the_call_silent() {
        let slot = Arc::new(SoundSlot::default());
        let reaper = Reaper::start();
        let hold = reaper.hold();
        slot.open_with::<Device>("test".into(), 1.0, false, hold, |_, _| {
            Err("no device".into())
        });
        reaper.wait_drained();
        assert!(matches!(slot.wanted.lock_or_recover().stage, Stage::Closed));
        assert!(slot.audio().is_none());
    }

    #[test]
    fn a_reopen_lands_after_a_failed_open() {
        let slot = Arc::new(SoundSlot::default());
        let failed = Reaper::start();
        slot.open_with::<Device>("test".into(), 1.0, false, failed.hold(), |_, _| {
            Err("no device".into())
        });
        failed.wait_drained();
        let open = blocked_open(&slot, 1.0, false);
        let _ = open.release.send(());
        assert!(wait_opened(&slot).is_ok());
        let current = slot.audio();
        let current = current.as_ref().expect("the output landed");
        assert!(Arc::ptr_eq(current, &open.audio));
    }

    #[test]
    fn quit_waits_for_a_closing_output() {
        let slot = Arc::new(SoundSlot::default());
        let open = blocked_open(&slot, 1.0, false);
        assert!(
            !open.reaper.is_drained(),
            "an open still running holds quit"
        );
        let _ = open.release.send(());
        assert!(wait_opened(&slot).is_ok());
        assert!(!open.reaper.is_drained(), "an open device holds quit");
        slot.clear();
        open.reaper.wait_drained();
        assert_eq!(open.closed.try_recv(), Ok(()), "closed before quit goes on");
    }
}
