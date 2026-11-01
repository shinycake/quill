//! A call's sound output. tgcalls has no devices, so Quill plays the call's
//! sound through its own output, which is opened, held and closed on a thread
//! of its own: a device open takes about 100 ms and much longer on Bluetooth,
//! and the engine runs on the UI thread. Frames are dropped until it opens.
use crate::calls::capture::CaptureError;
use crate::calls::capture::device_thread::DeviceThread;
use crate::calls::stream_audio::{StreamAudio, StreamPlayer};
use crate::locks::LockExt;
use arc_swap::{ArcSwapOption, Guard};
use std::sync::{Arc, Mutex};

/// What the output should be right now. Kept here so that a choice made
/// while the device opens reaches it, and an open that is no longer wanted
/// is told apart by its generation.
struct Wanted {
    generation: u64,
    volume: f32,
    muted: bool,
}

pub(super) struct SoundSlot {
    audio: ArcSwapOption<StreamAudio>,
    wanted: Mutex<Wanted>,
}

impl Default for SoundSlot {
    fn default() -> Self {
        Self {
            audio: ArcSwapOption::empty(),
            wanted: Mutex::new(Wanted {
                generation: 0,
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

    /// Stops the sound at once; an open still running will not land.
    pub(super) fn clear(&self) {
        let mut wanted = self.wanted.lock_or_recover();
        wanted.generation = wanted.generation.wrapping_add(1);
        self.audio.store(None);
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

    /// Opens the output named `device` in place of the current one. Dropping
    /// the answer closes the device on its thread. A failed open leaves the
    /// call silent and logs why, under `label`.
    pub(super) fn open(
        self: &Arc<Self>,
        label: String,
        device: Option<String>,
        volume: f32,
        muted: bool,
    ) -> DeviceThread {
        self.open_with(label, volume, muted, move |volume, muted| {
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
        open: impl FnOnce(f32, bool) -> Result<(D, Arc<StreamAudio>), String> + Send + 'static,
    ) -> DeviceThread {
        let generation = {
            let mut wanted = self.wanted.lock_or_recover();
            wanted.generation = wanted.generation.wrapping_add(1);
            wanted.volume = volume;
            wanted.muted = muted;
            self.audio.store(None);
            wanted.generation
        };
        let slot = Arc::clone(self);
        DeviceThread::open(
            "quill-call-sound",
            &crate::wake::UPDATES,
            move || match open(volume, muted) {
                Ok((device, audio)) => {
                    slot.publish(generation, audio);
                    Ok(device)
                }
                Err(err) => {
                    crate::advanced::debug_mode::call_log(&format!(
                        "[call] {label} sound output did not open ({err})"
                    ));
                    Err(CaptureError::Device(err))
                }
            },
            drop,
        )
    }

    fn publish(&self, generation: u64, audio: Arc<StreamAudio>) {
        let wanted = self.wanted.lock_or_recover();
        if wanted.generation == generation {
            audio.gain().set_volume(wanted.volume);
            audio.gain().set_muted(wanted.muted);
            self.audio.store(Some(audio));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        answer: DeviceThread,
        closed: Receiver<()>,
        audio: Arc<StreamAudio>,
    }

    /// An open that blocks until released.
    fn blocked_open(slot: &Arc<SoundSlot>, volume: f32, muted: bool) -> Held {
        let (release, released) = channel::<()>();
        let (closes, closed) = channel();
        let audio = Arc::new(StreamAudio::new(48_000, 1.0, false));
        let opened = audio.clone();
        let answer = slot.open_with("test".into(), volume, muted, move |_, _| {
            let _ = released.recv();
            Ok((Device(closes), opened))
        });
        Held {
            release,
            answer,
            closed,
            audio,
        }
    }

    fn wait_opened(answer: &DeviceThread) -> Result<(), CaptureError> {
        loop {
            if let Some(answer) = answer.opened() {
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
        assert!(wait_opened(&open.answer).is_ok());
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
        assert!(wait_opened(&open.answer).is_ok());
        assert!(slot.audio().is_none());
        drop(open.answer);
        assert_eq!(open.closed.recv(), Ok(()), "closed on its thread");
    }

    #[test]
    fn a_reopen_outruns_the_open_it_replaces() {
        let slot = Arc::new(SoundSlot::default());
        let first = blocked_open(&slot, 1.0, false);
        let second = blocked_open(&slot, 1.0, false);
        let _ = second.release.send(());
        assert!(wait_opened(&second.answer).is_ok());
        let _ = first.release.send(());
        assert!(wait_opened(&first.answer).is_ok());
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
        assert!(wait_opened(&open.answer).is_ok());
        let current = slot.audio();
        let current = current.as_ref().expect("the output landed");
        assert!(current.gain().muted());
        assert_eq!(current.gain().volume(), 0.25);
    }

    #[test]
    fn a_failed_open_leaves_the_call_silent() {
        let slot = Arc::new(SoundSlot::default());
        let answer =
            slot.open_with::<Device>("test".into(), 1.0, false, |_, _| Err("no device".into()));
        assert!(wait_opened(&answer).is_err());
        assert!(slot.audio().is_none());
    }
}
