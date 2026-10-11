//! The live camera preview of the profile photo panel (Telegram Desktop's
//! `CameraBox`): ffmpeg reads the camera, mirrors it, crops the center
//! square and writes raw RGB frames to a pipe. A thread keeps the newest
//! frame; the panel takes it on its redraw clock, and "Take photo" saves
//! the frame on screen.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
/// `profile_photo_sources::video_only_input`) as mirrored, center-square
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

/// A running camera stream. Dropping it stops ffmpeg.
pub struct CameraStream {
    latest: Arc<Mutex<Latest>>,
    done: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    log: PathBuf,
}

impl CameraStream {
    /// Start ffmpeg on the default camera. `log` receives its errors.
    pub fn start(log: &Path) -> Result<Self, String> {
        if !crate::media_tools::is_installed("ffmpeg") {
            return Err(crate::media_tools::ffmpeg_missing_message("The camera"));
        }
        let input = crate::media_tools::capture_input()?;
        let args = stream_args(
            &crate::profile_photo_sources::video_only_input(&input.args),
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
        let done = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(Some(child)));
        {
            let (latest, done, stop) = (latest.clone(), done.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                while !stop.load(Ordering::Relaxed) && read_frame(&mut stdout, &mut buf, LIVE_SIDE)
                {
                    if let Ok(mut slot) = latest.lock() {
                        slot.serial += 1;
                        std::mem::swap(&mut slot.rgb, &mut buf);
                    }
                }
                done.store(true, Ordering::Relaxed);
            });
        }
        Ok(Self {
            latest,
            done,
            stop,
            child,
            log: log.to_path_buf(),
        })
    }

    /// The newest frame if it is newer than `after` (the serial the caller
    /// last drew): its serial and `LIVE_SIDE` x `LIVE_SIDE` RGB24 bytes.
    pub fn frame_after(&self, after: u64) -> Option<(u64, Vec<u8>)> {
        let slot = self.latest.lock().ok()?;
        (slot.serial > after && !slot.rgb.is_empty()).then(|| (slot.serial, slot.rgb.clone()))
    }

    /// ffmpeg ended (camera busy, unplugged, no permission).
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// Why the stream ended, for the panel.
    pub fn failure(&self) -> String {
        crate::media_tools::capture_failure(&self.log, true)
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.child.lock()
            && let Some(mut child) = child.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{frame_len, read_frame, stream_args};
    use std::io::Cursor;

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
