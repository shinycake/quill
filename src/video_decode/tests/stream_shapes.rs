use super::super::decode_loop::{Decoder, Turn};
use super::super::ffi::FfiDemuxer;
use super::super::{
    AUDIO_LOW_SECS, AUDIO_OUTPUT_LATENCY, AudioChunk, AudioTap, Demuxer, Item, MediaInfo,
    OpenOptions, Pixels, Player, SYNC_TOLERANCE, Shared, State, VideoFrame, lock,
};
use super::{decode_all, decoder, fixture, frame};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const FPS: f64 = 25.0;
const FRAME_SECS: f64 = 1.0 / FPS;
const RATE: u32 = 1000;
const CHUNK_SECS: f64 = 0.02;
const QUEUE: usize = 4;
const SHIM_HELD_PACKETS: usize = 1024;
const STOPS_EARLY_CLIP: &str = "clip-sound-stops-early.mp4";
const STOPS_EARLY_PICTURES: usize = 30;
const STOPS_EARLY_SOUND_SECS: f64 = 0.5;
const GAP_CLIP: &str = "clip-sound-gap.mp4";
const GAP_PICTURES: usize = 1105;
const GAP_SOUND_RESUMES_SECS: f64 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Packet {
    Picture(f64),
    Sound(f64),
}

impl Packet {
    fn reaches(&self, secs: f64) -> bool {
        match self {
            Packet::Picture(pts) => pts + FRAME_SECS > secs + 1e-9,
            Packet::Sound(pts) => pts + CHUNK_SECS > secs + 1e-9,
        }
    }
}

#[derive(Clone, Copy)]
struct Shape {
    picture_secs: f64,
    sound_secs: f64,
    sound_behind_secs: f64,
    declares_pictures: bool,
    duration: f64,
    held_packet_limit: usize,
    /// No sound from the first time up to the second.
    sound_gap: (f64, f64),
}

impl Shape {
    fn clip(picture_secs: f64, sound_secs: f64) -> Self {
        Self {
            picture_secs,
            sound_secs,
            sound_behind_secs: 0.0,
            declares_pictures: true,
            duration: picture_secs.max(sound_secs),
            held_packet_limit: 1024,
            sound_gap: (0.0, 0.0),
        }
    }

    fn packets(&self) -> Vec<Packet> {
        let pictures = (self.picture_secs * FPS).round() as usize;
        let sounds = (self.sound_secs / CHUNK_SECS).round() as usize;
        let mut keyed: Vec<(f64, Packet)> = (0..pictures)
            .map(|n| n as f64 * FRAME_SECS)
            .map(|pts| (pts, Packet::Picture(pts)))
            .chain(
                (0..sounds)
                    .map(|n| n as f64 * CHUNK_SECS)
                    .filter(|pts| {
                        let (from, to) = self.sound_gap;
                        !(from - 1e-9..to - 1e-9).contains(pts)
                    })
                    .map(|pts| (pts + self.sound_behind_secs, Packet::Sound(pts))),
            )
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        keyed.into_iter().map(|(_, packet)| packet).collect()
    }
}

fn frames_to_the_end(secs: f64) -> usize {
    ((secs + AUDIO_OUTPUT_LATENCY) / FRAME_SECS).ceil() as usize
}

fn sound(pts: f64) -> AudioChunk {
    AudioChunk {
        pts,
        samples: vec![0.25; (f64::from(RATE) * CHUNK_SECS) as usize],
    }
}

#[derive(Default)]
struct Counts {
    decoded_pictures: AtomicUsize,
    packets_read: AtomicUsize,
}

struct Filed {
    shape: Shape,
    packets: Vec<Packet>,
    read: usize,
    skip_before_secs: f64,
    held: VecDeque<f64>,
    counts: Arc<Counts>,
}

impl Filed {
    fn item(&self, packet: Packet) -> Item {
        match packet {
            Packet::Picture(pts) => {
                self.counts.decoded_pictures.fetch_add(1, Ordering::SeqCst);
                Item::Video(frame(pts))
            }
            Packet::Sound(pts) => Item::Audio(sound(pts)),
        }
    }

    fn read_on(&mut self) -> Option<Packet> {
        while let Some(packet) = self.packets.get(self.read).copied() {
            self.read += 1;
            self.counts.packets_read.fetch_add(1, Ordering::SeqCst);
            if packet.reaches(self.skip_before_secs) {
                return Some(packet);
            }
        }
        None
    }
}

impl Demuxer for Filed {
    fn info(&self) -> MediaInfo {
        MediaInfo {
            duration: Some(self.shape.duration),
            frame_rate: FPS,
            width: 1,
            height: 1,
            has_video: self.shape.declares_pictures,
            has_audio: self.shape.sound_secs > 0.0,
            sample_rate: RATE,
            channels: 1,
            ..MediaInfo::default()
        }
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        let packet = match self.held.pop_front() {
            Some(pts) => Some(Packet::Picture(pts)),
            None => self.read_on(),
        };
        Ok(packet.map(|packet| self.item(packet)))
    }

    fn next_sound(&mut self) -> Result<Option<AudioChunk>, String> {
        while self.held.len() < self.shape.held_packet_limit {
            match self.read_on() {
                Some(Packet::Picture(pts)) => self.held.push_back(pts),
                Some(Packet::Sound(pts)) => return Ok(Some(sound(pts))),
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        self.held.clear();
        self.read = 0;
        self.skip_before_secs = secs;
        Ok(())
    }
}

struct Rig {
    player: Player,
    decoder: Decoder,
    shared: Arc<Shared>,
    tap: Option<AudioTap>,
    counts: Arc<Counts>,
}

impl Rig {
    fn open(shape: Shape) -> Self {
        let counts = Arc::new(Counts::default());
        let demuxer = Filed {
            shape,
            packets: shape.packets(),
            read: 0,
            skip_before_secs: 0.0,
            held: VecDeque::new(),
            counts: counts.clone(),
        };
        Self::over(Box::new(demuxer), shape.sound_secs > 0.0, counts)
    }

    fn real(name: &str) -> Option<Self> {
        if !decoder() {
            return None;
        }
        let demuxer = FfiDemuxer::open(&fixture(name), &OpenOptions::viewer()).expect("open");
        Some(Self::over(Box::new(demuxer), true, Arc::default()))
    }

    fn over(demuxer: Box<dyn Demuxer>, want_audio: bool, counts: Arc<Counts>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                video_cap: QUEUE,
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let decoder = Decoder::start(demuxer, &shared, want_audio);
        let mut player = Player {
            shared: shared.clone(),
            first: true,
            last_pts: None,
            ended: false,
            thread: None,
        };
        let tap = player.take_audio_tap();
        let mut rig = Self {
            player,
            decoder,
            shared,
            tap,
            counts,
        };
        rig.settle();
        rig
    }

    fn settle(&mut self) {
        self.settle_until_idle();
    }

    /// Run turns until the thread would sleep; how long it would.
    fn settle_until_idle(&mut self) -> Duration {
        self.settle_at(Instant::now())
    }

    /// Run turns at `now` until the thread would sleep; how long it would.
    fn settle_at(&mut self, now: Instant) -> Duration {
        loop {
            match self.decoder.turn(&self.shared, now) {
                Turn::Worked => {}
                Turn::Idle(_, wait) => return wait,
                Turn::Stop => panic!("the player is alive"),
            }
        }
    }

    fn pictures_queued(&self) -> usize {
        lock(&self.shared.state).video.len()
    }

    fn sound_buffered(&self) -> f64 {
        lock(&self.shared.state)
            .audio
            .as_ref()
            .map_or(0.0, |audio| audio.buffered_secs())
    }

    fn decoded_pictures(&self) -> usize {
        self.counts.decoded_pictures.load(Ordering::SeqCst)
    }

    fn packets_read(&self) -> usize {
        self.counts.packets_read.load(Ordering::SeqCst)
    }

    fn pull_sound(&mut self, secs: f64) -> f64 {
        let Some(tap) = &mut self.tap else {
            return 0.0;
        };
        let rate = f64::from(tap.sample_rate()) * f64::from(tap.channels());
        let mut sounding = 0u32;
        for _ in 0..(secs * rate).round() as usize {
            tap.next_sample();
            sounding += u32::from(tap.sounding());
        }
        f64::from(sounding) / rate
    }

    fn play_out_sound(&mut self) -> f64 {
        let mut sound = 0.0;
        loop {
            let pulled = self.pull_sound(0.1);
            self.settle();
            sound += pulled;
            if pulled == 0.0 {
                return sound;
            }
        }
    }

    fn play_with_nothing_painting(&mut self, secs: f64) -> f64 {
        let mut sound = 0.0;
        for _ in 0..(secs / FRAME_SECS).round() as usize {
            sound += self.pull_sound(FRAME_SECS);
            self.settle();
            assert!(
                self.pictures_queued() <= QUEUE,
                "{} pictures queued",
                self.pictures_queued(),
            );
        }
        sound
    }

    fn start(&mut self) -> Instant {
        self.player.play();
        let t0 = Instant::now();
        assert!(
            self.player.take_frame_for(t0, t0).is_some(),
            "the first picture shows at once"
        );
        t0
    }

    fn run(&mut self, from: Instant, frames: usize) -> Run {
        let mut run = Run {
            now: from,
            shown: Vec::new(),
            sound: 0.0,
        };
        for _ in 0..frames {
            run.sound += self.pull_sound(FRAME_SECS);
            run.now += Duration::from_secs_f64(FRAME_SECS);
            if let Some(frame) = self.player.take_frame_for(run.now, run.now) {
                run.shown.push(frame.pts);
            }
            self.settle();
            assert!(
                self.pictures_queued() <= QUEUE,
                "{} pictures queued at {:?}",
                self.pictures_queued(),
                run.shown.last(),
            );
        }
        run
    }
}

struct Run {
    now: Instant,
    shown: Vec<f64>,
    sound: f64,
}

impl Run {
    fn last_shown(&self) -> f64 {
        self.shown.last().copied().unwrap_or(-1.0)
    }

    fn skipped_a_picture(&self) -> bool {
        self.shown
            .windows(2)
            .any(|pair| pair[1] - pair[0] > 1.5 * FRAME_SECS)
    }
}

#[test]
fn a_paused_clip_with_sound_wakes_its_decoder_at_most_once_a_second() {
    let mut rig = Rig::open(Shape::clip(60.0, 60.0));
    rig.start();
    rig.player.pause();
    let wait = rig.settle_until_idle();
    assert!(wait >= Duration::from_secs(1), "wakes every {wait:?}");
}

#[test]
fn a_normal_clip_keeps_its_picture_queue_while_the_sound_buffers() {
    let rig = Rig::open(Shape::clip(60.0, 60.0));
    assert!(rig.sound_buffered() >= AUDIO_LOW_SECS);
    assert_eq!(rig.decoded_pictures(), QUEUE);
}

#[test]
fn sound_that_stops_early_does_not_decode_the_rest_of_the_file() {
    let mut rig = Rig::open(Shape::clip(60.0, 1.0));
    assert!((rig.play_out_sound() - 1.0).abs() < 1e-6);
    let through_the_sound = FPS as usize + QUEUE;
    assert!(
        rig.decoded_pictures() <= through_the_sound,
        "{} of 1500 in the file",
        rig.decoded_pictures()
    );
    assert_eq!(rig.pictures_queued(), QUEUE);
}

#[test]
fn a_clip_with_no_sound_left_goes_idle_and_stays_idle() {
    let mut rig = Rig::open(Shape::clip(60.0, 1.0));
    rig.play_out_sound();
    let read = rig.packets_read();
    rig.settle();
    rig.settle();
    assert_eq!(rig.packets_read(), read);
}

#[test]
fn the_picture_keeps_going_after_the_sound_stops() {
    let mut rig = Rig::open(Shape::clip(60.0, 1.0));
    let t0 = rig.start();
    let run = rig.run(t0, 125);
    assert!(run.sound > 0.99, "{} s of sound", run.sound);
    assert!(
        run.last_shown() > 4.9,
        "picture stopped at {}",
        run.last_shown()
    );
    assert!(run.shown.windows(2).all(|pair| pair[1] > pair[0]));
}

#[test]
fn a_seek_past_the_sound_before_any_played_leaves_the_clock_running() {
    let mut rig = Rig::open(Shape::clip(4.0, 0.5));
    let t0 = rig.start();
    rig.player.seek(1.0);
    rig.settle();
    let run = rig.run(t0, 25);
    assert!(
        run.last_shown() > 1.9,
        "picture stopped at {}",
        run.last_shown()
    );
}

#[test]
fn a_clip_ends_on_time_after_its_sound_stopped_early() {
    let mut rig = Rig::open(Shape::clip(2.0, 0.5));
    let t0 = rig.start();
    let frames = frames_to_the_end(2.0);
    let run = rig.run(t0, frames - 1);
    rig.player.update_ended(run.now);
    assert!(!rig.player.ended, "{} of 2 s", run.last_shown());
    let run = rig.run(run.now, 1);
    rig.player.update_ended(run.now);
    assert!(rig.player.ended);
    assert!(rig.player.error().is_none());
}

#[test]
fn sound_filed_behind_its_pictures_plays_on_time() {
    let mut rig = Rig::open(Shape {
        sound_behind_secs: 2.0,
        ..Shape::clip(20.0, 20.0)
    });
    assert!(rig.sound_buffered() >= AUDIO_LOW_SECS, "reached at open");
    assert_eq!(rig.decoded_pictures(), QUEUE);
    let t0 = rig.start();
    let run = rig.run(t0, 125);
    let expected = 5.0 - FRAME_SECS;
    assert!(
        (run.last_shown() - expected).abs() < 0.1,
        "{}",
        run.last_shown()
    );
    assert!((run.sound - 5.0).abs() < 1e-6, "{} s of sound", run.sound);
}

#[test]
fn sound_too_far_behind_to_reach_arrives_once_the_clock_moves_the_pictures() {
    let mut rig = Rig::open(Shape {
        sound_behind_secs: 2.0,
        held_packet_limit: 10,
        ..Shape::clip(10.0, 10.0)
    });
    assert_eq!(rig.sound_buffered(), 0.0, "out of reach at open");
    assert_eq!(rig.decoded_pictures(), QUEUE);
    let t0 = rig.start();
    let run = rig.run(t0, 100);
    assert!(run.sound > 0.0, "the sound stayed out of reach");
    assert!(
        run.last_shown() > 1.0,
        "picture stopped at {}",
        run.last_shown()
    );
    assert!(!run.skipped_a_picture(), "{:?}", run.shown);
}

#[test]
fn a_painted_clip_shows_every_picture() {
    let mut rig = Rig::open(Shape::clip(20.0, 20.0));
    let t0 = rig.start();
    let run = rig.run(t0, 250);
    assert!(run.last_shown() > 9.8, "{}", run.last_shown());
    assert!(!run.skipped_a_picture(), "{:?}", run.shown);
}

#[test]
fn sound_keeps_playing_while_nothing_paints() {
    let mut rig = Rig::open(Shape::clip(90.0, 90.0));
    rig.player.play();
    let sound = rig.play_with_nothing_painting(60.0);
    assert!((sound - 60.0).abs() < 1e-6, "{sound} s of sound");
}

#[test]
fn a_clip_nobody_paints_ends_when_its_time_runs_out() {
    let mut rig = Rig::open(Shape::clip(2.0, 2.0));
    let t0 = rig.start();
    rig.play_with_nothing_painting(1.0);
    rig.player.update_ended(t0 + Duration::from_secs(1));
    assert!(!rig.player.ended, "still playing at 1 s");
    rig.play_with_nothing_painting(1.5);
    rig.player.update_ended(t0 + Duration::from_secs_f64(2.5));
    assert!(
        rig.player.ended,
        "{} pictures queued",
        rig.pictures_queued()
    );
    assert!(rig.player.error().is_none());
}

#[test]
fn a_silent_clip_nobody_paints_keeps_its_picture_current() {
    let mut rig = Rig::open(Shape::clip(10.0, 0.0));
    let t0 = rig.start();
    let mut now = t0;
    let mut wakes = 0;
    while now < t0 + Duration::from_secs(5) {
        now += rig.settle_at(now);
        wakes += 1;
        assert!(rig.pictures_queued() <= QUEUE);
    }
    assert!(wakes <= 5 * FPS as usize + 2, "{wakes} wakes in 5 s");
    let shown = rig
        .player
        .take_frame_for(now, now)
        .expect("a picture is due");
    let position = rig.player.position_at(now);
    assert!(
        (position - shown.pts).abs() <= FRAME_SECS + SYNC_TOLERANCE,
        "{} at {position}",
        shown.pts
    );
}

#[test]
fn sound_after_a_gap_plays_at_its_time() {
    let mut rig = Rig::open(Shape {
        sound_gap: (1.0, 2.0),
        ..Shape::clip(4.0, 4.0)
    });
    rig.player.play();
    let mut play = |secs: f64| {
        for _ in 0..(secs / 0.1).round() as usize {
            rig.pull_sound(0.1);
            rig.settle();
        }
        rig.player.heard().expect("sound plays")
    };
    let heard = play(1.5);
    let expected = 1.5 - AUDIO_OUTPUT_LATENCY;
    assert!((heard - expected).abs() < 0.01, "{heard}");
    let heard = play(1.0);
    assert!((heard - (expected + 1.0)).abs() < 0.01, "{heard}");
}

#[test]
fn the_picture_is_current_when_painting_comes_back() {
    let mut rig = Rig::open(Shape::clip(90.0, 90.0));
    let t0 = rig.start();
    rig.play_with_nothing_painting(60.0);
    let back = t0 + Duration::from_secs(60);
    let shown = rig
        .player
        .take_frame_for(back, back)
        .expect("a picture is due");
    let position = rig.player.position_at(back);
    assert!(
        (position - shown.pts).abs() <= FRAME_SECS,
        "{} at {position}",
        shown.pts
    );
}

#[test]
fn the_last_picture_waits_for_paint_after_the_sound_passed_it() {
    let mut rig = Rig::open(Shape::clip(2.0, 3.0));
    let t0 = rig.start();
    rig.play_with_nothing_painting(3.0);
    let back = t0 + Duration::from_secs(3);
    let last = rig
        .player
        .take_frame_for(back, back)
        .expect("a picture is due");
    assert_eq!(last.pts, 49.0 * FRAME_SECS);
}

#[test]
fn the_picture_catches_up_after_the_display_stalled() {
    let mut rig = Rig::open(Shape::clip(5.0, 5.0));
    let t0 = rig.start();
    rig.pull_sound(0.5);
    let now = t0 + Duration::from_secs_f64(0.5);
    let mut shown = 0.0;
    for _ in 0..QUEUE {
        rig.settle();
        if let Some(frame) = rig.player.take_frame_for(now, now) {
            shown = frame.pts;
        }
    }
    let position = rig.player.position_at(now);
    assert!(
        (shown - position).abs() <= FRAME_SECS,
        "{shown} at {position}"
    );
}

#[test]
fn a_seek_past_the_last_picture_ends_the_clip() {
    let mut rig = Rig::open(Shape::clip(1.0, 0.0));
    let t0 = rig.start();
    rig.player.seek(1.0);
    rig.settle();
    rig.player.update_ended(t0);
    assert!(rig.player.ended);
    assert!(rig.player.error().is_none());
    assert_eq!(rig.player.position_at(t0), 1.0);
}

#[test]
fn a_seek_past_the_last_picture_can_play_again_from_the_start() {
    let mut rig = Rig::open(Shape::clip(1.0, 0.0));
    let t0 = rig.start();
    rig.player.seek(1.0);
    rig.settle();
    rig.player.update_ended(t0);
    rig.player.play();
    rig.settle();
    let first = rig.player.take_frame_for(t0, t0).map(|frame| frame.pts);
    assert_eq!(first, Some(0.0));
}

#[test]
fn a_seek_where_pictures_ran_out_before_the_duration_ends_there() {
    let mut rig = Rig::open(Shape {
        duration: 3.0,
        ..Shape::clip(1.0, 0.0)
    });
    let t0 = rig.start();
    rig.player.seek(2.0);
    rig.settle();
    rig.player.update_ended(t0);
    assert!(rig.player.ended);
    assert_eq!(rig.player.position_at(t0), 3.0);
}

#[test]
fn a_seek_into_sound_with_no_pictures_runs_on_the_sound_then_ends() {
    let mut rig = Rig::open(Shape {
        duration: 3.0,
        ..Shape::clip(1.0, 3.0)
    });
    let t0 = rig.start();
    rig.player.seek(2.0);
    rig.settle();
    rig.player.update_ended(t0);
    assert!(!rig.player.ended, "the sound is still playing");
    let t1 = t0 + Duration::from_millis(500);
    rig.pull_sound(0.5);
    let heard = rig.player.position_at(t1);
    assert!((heard - 2.43).abs() < 0.05, "{heard}");
    rig.pull_sound(1.0);
    rig.settle();
    rig.player.update_ended(t1);
    assert!(rig.player.ended);
}

#[test]
fn a_clip_with_no_pictures_fails() {
    let mut rig = Rig::open(Shape::clip(0.0, 0.0));
    assert!(rig.player.error().is_some());
    rig.player.play();
    rig.settle();
    assert!(!rig.player.starved());
}

#[test]
fn a_clip_with_sound_but_no_pictures_fails() {
    let rig = Rig::open(Shape::clip(0.0, 1.0));
    assert!(rig.player.error().is_some());
}

#[test]
fn a_clip_with_long_sound_and_no_pictures_fails_once_the_file_is_read() {
    let mut rig = Rig::open(Shape::clip(0.0, 60.0));
    assert!(rig.player.error().is_none());
    assert!(rig.play_out_sound() > 59.0);
    assert!(rig.player.error().is_some());
}

#[test]
fn a_seek_does_not_hide_that_a_clip_has_no_pictures() {
    let mut rig = Rig::open(Shape {
        duration: 3.0,
        ..Shape::clip(0.0, 0.0)
    });
    rig.player.seek(1.0);
    rig.settle();
    assert!(rig.player.error().is_some());
}

#[test]
fn a_seek_to_the_end_before_any_picture_showed_is_not_a_failure() {
    let mut rig = Rig::open(Shape::clip(1.0, 0.0));
    assert!(rig.player.error().is_none());
    rig.player.seek(1.0);
    rig.settle();
    assert!(rig.player.error().is_none());
    rig.player.update_ended(Instant::now());
    assert!(rig.player.ended);
}

#[test]
fn ffmpeg_keeps_showing_pictures_after_the_sound_stopped_and_ends() {
    let Some(mut rig) = Rig::real(STOPS_EARLY_CLIP) else {
        return;
    };
    let t0 = rig.start();
    let total = rig.player.duration().expect("a duration");
    let run = rig.run(t0, frames_to_the_end(total));
    let after_the_first = STOPS_EARLY_PICTURES - 1;
    assert_eq!(run.shown.len(), after_the_first, "{:?}", run.shown);
    assert!(run.shown.windows(2).all(|pair| pair[1] > pair[0]));
    let sound_off = (run.sound - STOPS_EARLY_SOUND_SECS).abs();
    assert!(sound_off < 0.05, "{} s of sound", run.sound);
    rig.player.update_ended(run.now);
    assert!(rig.player.ended);
    assert!(rig.player.error().is_none());
}

fn pts_and_pixels(frames: &[VideoFrame]) -> Vec<(f64, &Pixels)> {
    frames
        .iter()
        .map(|frame| (frame.pts, &frame.pixels))
        .collect()
}

#[test]
fn ffmpeg_reaching_for_sound_loses_no_picture_and_no_sound() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let options = OpenOptions::viewer();
    let (_, pictures, samples) = decode_all(&clip, &options);
    let mut demuxer = FfiDemuxer::open(&clip, &options).expect("open");
    let mut reached = 0;
    while let Some(chunk) = demuxer.next_sound().expect("sound") {
        reached += chunk.samples.len();
    }
    assert_eq!(reached, samples);
    let mut after = Vec::new();
    while let Some(item) = demuxer.next().expect("decode") {
        match item {
            Item::Video(frame) => after.push(frame),
            Item::Audio(_) => panic!("the sound was all reached"),
        }
    }
    assert_eq!(pts_and_pixels(&after), pts_and_pixels(&pictures));
}

#[test]
fn ffmpeg_sound_past_too_many_pictures_is_out_of_reach_until_some_move() {
    if !decoder() {
        return;
    }
    let clip = fixture(GAP_CLIP);
    let mut demuxer = FfiDemuxer::open(&clip, &OpenOptions::viewer()).expect("open");
    while demuxer.next_sound().expect("sound").is_some() {}
    let mut pictures = 0;
    let late = loop {
        match demuxer.next().expect("decode") {
            Some(Item::Video(_)) => pictures += 1,
            Some(Item::Audio(_)) => continue,
            None => panic!("the late sound never came in reach"),
        }
        if let Some(chunk) = demuxer.next_sound().expect("sound") {
            break chunk;
        }
    };
    assert!(late.pts > GAP_SOUND_RESUMES_SECS, "{}", late.pts);
    let must_move = GAP_PICTURES - SHIM_HELD_PACKETS;
    assert!(
        (1..=must_move).contains(&pictures),
        "{pictures} pictures taken"
    );
    while let Some(item) = demuxer.next().expect("decode") {
        pictures += usize::from(matches!(item, Item::Video(_)));
    }
    assert_eq!(pictures, GAP_PICTURES);
}

#[test]
fn ffmpeg_a_seek_past_the_end_shows_the_last_picture_then_ends() {
    let Some(mut rig) = Rig::real("clip-h264-aac.mp4") else {
        return;
    };
    let t0 = rig.start();
    let total = rig.player.duration().expect("a duration");
    rig.player.seek(total);
    rig.settle();
    let last = rig.player.take_frame_for(t0, t0).map(|frame| frame.pts);
    assert_eq!(last.map(|pts| (pts * 10.0).round()), Some(9.0), "{last:?}");
    rig.settle();
    rig.play_out_sound();
    rig.player.update_ended(t0);
    assert!(rig.player.ended);
    assert_eq!(rig.player.position_at(t0), total);
    assert!(rig.player.error().is_none());
}

fn corrupted_copy(name: &str, into: &Path) {
    let mut bytes = std::fs::read(fixture(name)).expect("read the fixture");
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[at..at + 4].try_into().expect("four bytes")) as usize;
        if &bytes[at + 4..at + 8] == b"mdat" {
            bytes[at + 8..at + size].fill(0xA5);
        }
        at += size.max(8);
    }
    std::fs::write(into, bytes).expect("write the copy");
}

#[test]
fn ffmpeg_gives_up_on_a_stream_of_corrupt_packets() {
    if !decoder() {
        return;
    }
    let copy = std::env::temp_dir().join(format!("quill-corrupt-{}.mp4", std::process::id()));
    corrupted_copy(STOPS_EARLY_CLIP, &copy);
    let mut demuxer = FfiDemuxer::open(&copy, &OpenOptions::inline(96)).expect("the file opens");
    let outcome = loop {
        match demuxer.next() {
            Ok(Some(_)) => {}
            Ok(None) => break Ok(()),
            Err(err) => break Err(err),
        }
    };
    drop(demuxer);
    let _ = std::fs::remove_file(&copy);
    assert!(outcome.is_err(), "ran to the end without a word");
}
