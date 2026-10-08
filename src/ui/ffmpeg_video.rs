//! The in-process FFmpeg player (`quill::video_decode`) behind
//! `NativeVideo` on Linux and Windows: the same calls as the macOS AVPlayer
//! backend, with pictures handed out as GPUI images and the soundtrack
//! played through the app's audio output (`audio::StreamSound`).
//!
//! Every new picture becomes a new `RenderImage`; the one it replaces is
//! retired to `image_budget`, which drops it from the GPU atlas once no
//! frame can show it, so a playing clip holds a picture or two on the GPU.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::RenderImage;
use quill::video_decode::{OpenOptions, Player, VideoFrame};

use super::audio::StreamSound;

/// What a clip is opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    /// A muted loop in the history, drawn at most this many px on its
    /// longer side (twice the tile, for HiDPI).
    Inline { max_edge: u32 },
    /// The media viewer: full size, with sound.
    Viewer,
}

impl Purpose {
    fn options(self) -> OpenOptions {
        match self {
            Purpose::Inline { max_edge } => OpenOptions::inline(max_edge.max(16)),
            Purpose::Viewer => OpenOptions::viewer(),
        }
    }
}

pub(super) struct FfmpegVideo {
    path: PathBuf,
    options: OpenOptions,
    player: Player,
    sound: Option<StreamSound>,
    volume: f32,
    rate: f64,
    playing: bool,
    image: Option<Arc<RenderImage>>,
}

impl FfmpegVideo {
    /// Open a local file; playback starts paused. The file is opened and
    /// decoded on a background thread.
    pub(super) fn open(path: &Path, purpose: Purpose) -> Result<Self, String> {
        let options = purpose.options();
        let player = Player::open(path, options.clone())?;
        Ok(Self {
            path: path.to_path_buf(),
            options,
            player,
            sound: None,
            volume: 1.0,
            rate: 1.0,
            playing: false,
            image: None,
        })
    }

    /// Connect the soundtrack once the decoder found one.
    fn attach_sound(&mut self) {
        if self.sound.is_some() {
            return;
        }
        if let Some(tap) = self.player.take_audio_tap() {
            self.sound = StreamSound::start(tap, self.volume, self.rate, self.playing);
        }
    }

    pub(super) fn play(&mut self) {
        self.player.play();
        self.playing = true;
        self.attach_sound();
        if let Some(sound) = &self.sound {
            sound.play();
        }
    }

    pub(super) fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
        if let Some(sound) = &self.sound {
            sound.pause();
        }
    }

    pub(super) fn is_playing(&mut self) -> bool {
        self.attach_sound();
        let playing = self.player.is_playing();
        if !playing && self.playing {
            // Ran out: stop pulling silence.
            self.playing = false;
            if let Some(sound) = &self.sound {
                sound.pause();
            }
        }
        playing
    }

    pub(super) fn seek(&mut self, secs: f64) {
        self.player.seek(secs);
        if let Some(sound) = &self.sound {
            sound.flush();
        }
    }

    /// 0.0 (muted) – 1.0. A clip opened muted (an inline loop) reopens with
    /// its soundtrack the first time it is turned up, where it was.
    pub(super) fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if self.volume > 0.0 && !self.options.want_audio {
            self.options.want_audio = true;
            let position = self.player.position();
            if let Ok(mut player) = Player::open(&self.path, self.options.clone()) {
                player.set_rate(self.rate);
                player.seek(position);
                if self.playing {
                    player.play();
                }
                self.player = player;
                self.sound = None;
            }
        }
        if let Some(sound) = &self.sound {
            sound.set_volume(self.volume);
        }
    }

    pub(super) fn set_rate(&mut self, rate: f32) {
        self.rate = f64::from(rate);
        self.player.set_rate(self.rate);
        if let Some(sound) = &self.sound {
            sound.set_speed(self.rate);
        }
    }

    pub(super) fn position_secs(&self) -> f64 {
        self.player.position()
    }

    pub(super) fn duration_secs(&self) -> Option<f64> {
        self.player.duration()
    }

    pub(super) fn error(&self) -> Option<String> {
        self.player.error()
    }

    /// The picture to show now: a new one when it is due, else the last.
    pub(super) fn frame(&mut self) -> Option<Arc<RenderImage>> {
        self.attach_sound();
        if let Some(frame) = self.player.take_frame()
            && let Some(image) = render_image(frame)
            && let Some(old) = self.image.replace(image)
        {
            super::image_budget::retire_all([old]);
        }
        self.image.clone()
    }
}

impl Drop for FfmpegVideo {
    fn drop(&mut self) {
        if let Some(old) = self.image.take() {
            super::image_budget::retire_all([old]);
        }
    }
}

/// A decoded BGRA picture as a GPUI image (no copy: the buffer moves in).
fn render_image(frame: VideoFrame) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

#[cfg(test)]
mod tests {
    use super::{Purpose, render_image};
    use quill::video_decode::VideoFrame;

    #[test]
    fn inline_clips_open_muted_and_bounded_viewer_clips_with_sound() {
        let inline = Purpose::Inline { max_edge: 640 }.options();
        assert!(!inline.want_audio);
        assert_eq!((inline.max_width, inline.max_height), (640, 640));
        // A degenerate tile still decodes something visible.
        assert_eq!(Purpose::Inline { max_edge: 0 }.options().max_width, 16);
        assert!(Purpose::Viewer.options().want_audio);
    }

    #[test]
    fn decoded_pictures_become_images_without_reordering_channels() {
        let frame = VideoFrame {
            pts: 0.0,
            width: 2,
            height: 1,
            bgra: vec![1, 2, 3, 255, 4, 5, 6, 255],
        };
        let image = render_image(frame).expect("image");
        assert_eq!(image.as_bytes(0), Some(&[1, 2, 3, 255, 4, 5, 6, 255][..]));
        // A buffer of the wrong size is rejected, not misdrawn.
        let short = VideoFrame {
            pts: 0.0,
            width: 2,
            height: 2,
            bgra: vec![0; 4],
        };
        assert!(render_image(short).is_none());
    }
}
