//! The real decode loop and player fed the shapes real files come in. The
//! test thread takes the decode thread's turns itself and supplies the time,
//! so nothing here sleeps or waits.

use super::super::decode_loop::{Decoder, Turn};
use super::super::{
    AUDIO_LOW_SECS, AudioChunk, AudioTap, Clock, Demuxer, Item, MediaInfo, Player, Shared, State,
    lock,
};
use super::frame;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const FPS: f64 = 25.0;
const FRAME_SECS: f64 = 1.0 / FPS;
const RATE: u32 = 1000;
const CHUNK_SECS: f64 = 0.02;
const QUEUE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Packet {
    Picture(f64),
    Sound(f64),
}

impl Packet {
    /// Whether this packet still covers `secs`, or comes after it.
    fn reaches(&self, secs: f64) -> bool {
        match self {
            Packet::Picture(pts) => pts + FRAME_SECS > secs + 1e-9,
            Packet::Sound(pts) => pts + CHUNK_SECS > secs + 1e-9,
        }
    }
}

/// What a file holds and where its sound sits.
#[derive(Clone, Copy)]
struct Shape {
    picture_secs: f64,
    sound_secs: f64,
    /// Seconds the sound sits behind the pictures of the same time.
    sound_behind: f64,
    /// The header says there is a picture track, whether or not it has
    /// any pictures in it.
    declares_pictures: bool,
    /// The header's duration.
    duration: f64,
}

impl Shape {
    /// Pictures for `picture_secs`, sound side by side for `sound_secs`.
    fn clip(picture_secs: f64, sound_secs: f64) -> Self {
        Self {
            picture_secs,
            sound_secs,
            sound_behind: 0.0,
            declares_pictures: true,
            duration: picture_secs.max(sound_secs),
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
                    .map(|pts| (pts + self.sound_behind, Packet::Sound(pts))),
            )
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        keyed.into_iter().map(|(_, packet)| packet).collect()
    }
}

#[derive(Default)]
struct Counts {
    decoded_pictures: AtomicUsize,
    packets_read: AtomicUsize,
}

/// A demuxer that reads `packets` in file order.
struct Filed {
    shape: Shape,
    packets: Vec<Packet>,
    read: usize,
    /// Packets ending before this were seeked past.
    from: f64,
    counts: Arc<Counts>,
}

impl Filed {
    fn item(&self, packet: Packet) -> Item {
        match packet {
            Packet::Picture(pts) => {
                self.counts.decoded_pictures.fetch_add(1, Ordering::SeqCst);
                Item::Video(frame(pts))
            }
            Packet::Sound(pts) => Item::Audio(AudioChunk {
                pts,
                samples: vec![0.25; (f64::from(RATE) * CHUNK_SECS) as usize],
            }),
        }
    }

    fn read_on(&mut self) -> Option<Packet> {
        while let Some(packet) = self.packets.get(self.read).copied() {
            self.read += 1;
            self.counts.packets_read.fetch_add(1, Ordering::SeqCst);
            if packet.reaches(self.from) {
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
        Ok(self.read_on().map(|packet| self.item(packet)))
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        self.read = 0;
        self.from = secs;
        Ok(())
    }
}

/// A player and the decode loop that serves it, side by side on one thread.
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
            from: 0.0,
            counts: counts.clone(),
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                video_cap: QUEUE,
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let want_audio = shape.sound_secs > 0.0;
        let decoder = Decoder::start(Box::new(demuxer), &shared, want_audio);
        let mut player = Player {
            shared: shared.clone(),
            clock: Clock::new(),
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

    /// Take decode turns until the loop has nothing left to do.
    fn settle(&mut self) {
        loop {
            match self.decoder.turn(&self.shared) {
                Turn::Worked => {}
                Turn::Idle(..) => return,
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

    /// Pull `secs` of output the way the audio device does: silence when
    /// nothing is queued. Returns the seconds of real sound.
    fn pull_sound(&mut self, secs: f64) -> f64 {
        let Some(tap) = &mut self.tap else {
            return 0.0;
        };
        let mut sounding = 0u32;
        for _ in 0..(secs * f64::from(RATE)).round() as usize {
            tap.next_sample();
            sounding += u32::from(tap.sounding());
        }
        f64::from(sounding) / f64::from(RATE)
    }

    /// Pull until the decode loop has no sound left to give, with nobody
    /// taking pictures. Returns the seconds of real sound.
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

    /// Show the first picture and start the clock.
    fn start(&mut self) -> Instant {
        self.player.play();
        let t0 = Instant::now();
        let first = self.player.take_frame_for(t0, t0).map(|frame| frame.pts);
        assert_eq!(first, Some(0.0), "the first picture shows at once");
        t0
    }

    /// Run the display at 25 fps from `from` for `frames`, with the sound
    /// device pulling and the decode thread keeping up. Returns the time
    /// reached, the pictures shown and the seconds of real sound pulled.
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
    assert_eq!(rig.decoded_pictures(), QUEUE, "of 1500 in the file");
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
    assert!(run.last_shown() > 4.9, "picture stopped at {}", run.last_shown());
    assert!(run.shown.windows(2).all(|pair| pair[1] > pair[0]));
}

#[test]
fn a_clip_ends_on_time_after_its_sound_stopped_early() {
    let mut rig = Rig::open(Shape::clip(2.0, 0.5));
    let t0 = rig.start();
    let run = rig.run(t0, 49);
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
        sound_behind: 2.0,
        ..Shape::clip(20.0, 20.0)
    });
    assert!(rig.sound_buffered() >= AUDIO_LOW_SECS, "reached at open");
    assert_eq!(rig.decoded_pictures(), QUEUE);
    let t0 = rig.start();
    let run = rig.run(t0, 125);
    let expected = 5.0 - FRAME_SECS;
    assert!((run.last_shown() - expected).abs() < 0.1, "{}", run.last_shown());
    assert!((run.sound - 5.0).abs() < 1e-6, "{} s of sound", run.sound);
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
