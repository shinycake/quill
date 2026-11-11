//! The live camera preview of the profile photo panel (Telegram Desktop's
//! `CameraBox`): ffmpeg reads the camera, mirrors it, crops the center
//! square and writes raw RGB frames to a pipe. A thread keeps the newest
//! frame; the panel takes it on its redraw clock, and "Take photo" saves
//! the frame on screen.
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// Side of the frames, in pixels (`kProfilePhotoSize` on desktop).
pub const LIVE_SIDE: u32 = 640;
/// Frames per second asked from the camera.
pub const LIVE_FPS: u32 = 15;

/// Bytes in one RGB24 frame of `side` x `side`.
pub fn frame_len(side: u32) -> usize {
    side as usize * side as usize * 3
}

/// ffmpeg arguments that stream the camera (`input` from
/// `media_tools::camera_input`) as mirrored, center-square
/// RGB24 frames on stdout.
pub fn stream_args(input: &[String], side: u32, fps: u32) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|a| a.to_string())
        .collect();
    args.extend(input.iter().cloned());
    args.push("-an".into());
    args.push("-vf".into());
    args.push(format!(
        "hflip,crop=min(iw\\,ih):min(iw\\,ih),scale={side}:{side},fps={fps}"
    ));
    args.extend(
        ["-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
            .iter()
            .map(|a| a.to_string()),
    );
    args
}

/// Read one whole frame into `buf`; false at the end of the stream (a
/// partial last frame is dropped).
pub fn read_frame(reader: &mut impl Read, buf: &mut Vec<u8>, side: u32) -> bool {
    buf.resize(frame_len(side), 0);
    reader.read_exact(buf).is_ok()
}

#[derive(Default)]
struct Latest {
    serial: u64,
    rgb: Vec<u8>,
}

/// Where a stream is, shared with its reader thread as one atomic value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum StreamStage {
    /// The reader is taking frames.
    Running = 0,
    /// ffmpeg ended by itself (camera busy, unplugged, no permission).
    Ended = 1,
    /// The owner dropped the stream and ffmpeg is being stopped.
    Stopping = 2,
}

struct SharedStage(AtomicU8);

impl SharedStage {
    fn new() -> Self {
        Self(AtomicU8::new(StreamStage::Running as u8))
    }

    fn get(&self) -> StreamStage {
        match self.0.load(Ordering::Acquire) {
            0 => StreamStage::Running,
            1 => StreamStage::Ended,
            _ => StreamStage::Stopping,
        }
    }

    /// `Running` moves to `to`; a stage that already left `Running` stays
    /// (an owner's stop is not turned into an end by the thread exiting).
    fn leave_running(&self, to: StreamStage) -> StreamStage {
        match self.0.compare_exchange(
            StreamStage::Running as u8,
            to as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => to,
            Err(_) => self.get(),
        }
    }
}

/// Take frames from `reader` into `latest` until the stream ends or the
/// owner stops it. Leaves `Running` for `Ended` when the stream ran out on
/// its own.
fn pump(reader: &mut impl Read, latest: &Mutex<Latest>, stage: &SharedStage) -> StreamStage {
    let mut buf = Vec::new();
    while stage.get() == StreamStage::Running && read_frame(reader, &mut buf, LIVE_SIDE) {
        let mut slot = latest.lock_or_recover();
        slot.serial = slot.serial.wrapping_add(1);
        std::mem::swap(&mut slot.rgb, &mut buf);
    }
    stage.leave_running(StreamStage::Ended)
}

/// A running camera stream. Dropping it stops ffmpeg.
pub struct CameraStream {
    latest: Arc<Mutex<Latest>>,
    stage: Arc<SharedStage>,
    child: Arc<Mutex<Option<Child>>>,
    log: PathBuf,
}

impl CameraStream {
    /// Start ffmpeg on the picked camera (`choice`; else the default). `log` receives its errors.
    pub fn start(
        log: &Path,
        choice: &crate::capture_choice::CaptureChoice,
    ) -> Result<Self, String> {
        if !crate::media_tools::is_installed("ffmpeg") {
            return Err(crate::media_tools::ffmpeg_missing_message("The camera"));
        }
        let args = stream_args(
            &crate::media_tools::camera_input(choice)?,
            LIVE_SIDE,
            LIVE_FPS,
        );
        let log_file = std::fs::File::create(log).map_err(|e| e.to_string())?;
        let mut child = crate::media_tools::command("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log_file)
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdout = child.stdout.take().ok_or("Couldn't start the camera.")?;
        let latest = Arc::new(Mutex::new(Latest::default()));
        let stage = Arc::new(SharedStage::new());
        let child = Arc::new(Mutex::new(Some(child)));
        let spawned = {
            let (latest, stage, child) = (latest.clone(), stage.clone(), child.clone());
            crate::thread_guard::spawn_guarded("quill-camera", move || {
                if pump(&mut stdout, &latest, &stage) == StreamStage::Ended {
                    // ffmpeg is done: reap it now, not when the panel drops.
                    let process = child.lock_or_recover().take();
                    if let Some(process) = process {
                        crate::child_reap::kill_and_reap(process);
                    }
                }
            })
        };
        if let Err(error) = spawned {
            // Nothing would read the camera: stop ffmpeg now.
            // Take the child out first so the lock is not held while
            // ffmpeg is killed and reaped.
            let process = child.lock_or_recover().take();
            if let Some(mut process) = process {
                let _ = process.kill();
                let _ = process.wait();
            }
            return Err(error.to_string());
        }
        Ok(Self {
            latest,
            stage,
            child,
            log: log.to_path_buf(),
        })
    }

    /// The newest frame if it is newer than `after` (the serial the caller
    /// last drew): its serial and `LIVE_SIDE` x `LIVE_SIDE` RGB24 bytes.
    pub fn frame_after(&self, after: u64) -> Option<(u64, Vec<u8>)> {
        let slot = self.latest.lock_or_recover();
        (slot.serial > after && !slot.rgb.is_empty()).then(|| (slot.serial, slot.rgb.clone()))
    }

    /// ffmpeg ended (camera busy, unplugged, no permission).
    pub fn finished(&self) -> bool {
        self.stage.get() == StreamStage::Ended
    }

    /// Why the stream ended, for the panel.
    pub fn failure(&self) -> String {
        crate::media_tools::capture_failure(&self.log, true)
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stage.leave_running(StreamStage::Stopping);
        let child = self.child.lock_or_recover().take();
        if let Some(child) = child {
            crate::child_reap::kill_and_reap(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LIVE_SIDE, Latest, SharedStage, StreamStage, frame_len, pump, read_frame, stream_args,
    };
    use std::io::Cursor;
    use std::sync::Mutex;

    #[test]
    fn a_stream_that_runs_out_ends_and_a_stopped_one_stays_stopping() {
        let frames = vec![7u8; frame_len(LIVE_SIDE) * 2];
        let latest = Mutex::new(Latest::default());
        let stage = SharedStage::new();
        assert_eq!(
            pump(&mut Cursor::new(frames.clone()), &latest, &stage),
            StreamStage::Ended
        );
        assert_eq!(stage.get(), StreamStage::Ended);
        assert_eq!(latest.lock().map(|slot| slot.serial).unwrap_or(0), 2);
        let stopped = SharedStage::new();
        stopped.leave_running(StreamStage::Stopping);
        assert_eq!(
            pump(&mut Cursor::new(frames), &latest, &stopped),
            StreamStage::Stopping,
            "the owner's stop is not an end"
        );
        assert_eq!(latest.lock().map(|slot| slot.serial).unwrap_or(0), 2);
    }

    #[test]
    fn frames_are_read_whole_and_a_partial_tail_is_dropped() {
        let side = 2;
        let n = frame_len(side);
        assert_eq!(n, 12);
        let mut data = vec![1u8; n];
        data.extend(vec![2u8; n]);
        data.extend(vec![3u8; 5]);
        let mut reader = Cursor::new(data);
        let mut buf = Vec::new();
        assert!(read_frame(&mut reader, &mut buf, side));
        assert_eq!(buf, vec![1u8; n]);
        assert!(read_frame(&mut reader, &mut buf, side));
        assert_eq!(buf, vec![2u8; n]);
        assert!(!read_frame(&mut reader, &mut buf, side));
    }

    #[test]
    fn stream_command_mirrors_crops_and_pipes_rgb() {
        let input: Vec<String> = ["-f", "v4l2", "-i", "/dev/video0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let args = stream_args(&input, 640, 15);
        let at = |flag: &str| {
            let i = args.iter().position(|a| a == flag).unwrap();
            args[i + 1].clone()
        };
        assert_eq!(at("-i"), "/dev/video0");
        assert_eq!(
            at("-vf"),
            "hflip,crop=min(iw\\,ih):min(iw\\,ih),scale=640:640,fps=15"
        );
        assert_eq!(at("-pix_fmt"), "rgb24");
        assert_eq!(args.last().unwrap(), "-");
        assert!(args.contains(&"-an".to_string()));
    }
}
