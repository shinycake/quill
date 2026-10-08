//! Linux: MPRIS (`org.mpris.MediaPlayer2`) on the session bus, the interface
//! desktop environments, `playerctl` and media keys use. Same zbus stack GPUI
//! and the tray (`ksni`) already link.
//!
//! Everything touching the bus happens on one worker thread, so a missing or
//! slow session bus can never stall the UI: [`set`] only sends the new state
//! over a channel. Without a bus (headless, containers) the worker gives up
//! quietly and the media keys are simply not wired.

use super::{Command, NowPlaying, push_command};
use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use zbus::blocking::connection::Builder;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.quill";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

/// What the interfaces answer from; written by the worker.
#[derive(Default)]
struct Shared {
    info: Option<NowPlaying>,
    at: Option<Instant>,
}

impl Shared {
    fn position_us(&self) -> i64 {
        let Some(info) = &self.info else { return 0 };
        let moved = match (info.playing, self.at) {
            (true, Some(at)) => at.elapsed().as_secs_f64() * info.rate,
            _ => 0.0,
        };
        ((info.position_secs + moved).max(0.0) * 1e6) as i64
    }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Root;

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}

    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "Quill".into()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    shared: Arc<Mutex<Shared>>,
}

fn metadata(info: &NowPlaying) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let mut put = |key: &str, value: Value<'_>| {
        if let Ok(owned) = OwnedValue::try_from(value) {
            map.insert(key.to_string(), owned);
        }
    };
    if let Ok(track) = ObjectPath::try_from("/org/quill/track/current") {
        put("mpris:trackid", Value::ObjectPath(track));
    }
    put(
        "mpris:length",
        Value::I64((info.duration_secs * 1e6) as i64),
    );
    put("xesam:title", Value::from(info.title.clone()));
    if !info.artist.is_empty() {
        put("xesam:artist", Value::from(vec![info.artist.clone()]));
    }
    map
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        push_command(Command::Next);
    }

    fn previous(&self) {
        push_command(Command::Previous);
    }

    fn pause(&self) {
        push_command(Command::Pause);
    }

    fn play_pause(&self) {
        push_command(Command::Toggle);
    }

    fn stop(&self) {
        push_command(Command::Stop);
    }

    fn play(&self) {
        push_command(Command::Play);
    }

    fn seek(&self, offset_us: i64) {
        let at_us = lock(&self.shared).position_us();
        push_command(Command::SeekTo(((at_us + offset_us).max(0)) as f64 / 1e6));
    }

    fn set_position(&self, _track_id: ObjectPath<'_>, position_us: i64) {
        push_command(Command::SeekTo(position_us.max(0) as f64 / 1e6));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        match &lock(&self.shared).info {
            Some(info) if info.playing => "Playing",
            Some(_) => "Paused",
            None => "Stopped",
        }
        .into()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        lock(&self.shared).info.as_ref().map_or(1.0, |i| i.rate)
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.5
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        2.5
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        lock(&self.shared)
            .info
            .as_ref()
            .map(metadata)
            .unwrap_or_default()
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        lock(&self.shared).position_us()
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        lock(&self.shared).info.as_ref().is_some_and(|i| i.can_next)
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        lock(&self.shared)
            .info
            .as_ref()
            .is_some_and(|i| i.can_previous)
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        lock(&self.shared).info.is_some()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        lock(&self.shared).info.is_some()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        lock(&self.shared).info.is_some()
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

static WORKER: OnceLock<Sender<Option<NowPlaying>>> = OnceLock::new();

pub(super) fn set(info: Option<&NowPlaying>) {
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<Option<NowPlaying>>();
        let spawned = std::thread::Builder::new()
            .name("mpris".into())
            .spawn(move || run(rx));
        if spawned.is_err() {
            // No thread: the receiver is gone, sends below just fail.
        }
        tx
    });
    let _ = sender.send(info.cloned());
}

fn run(rx: std::sync::mpsc::Receiver<Option<NowPlaying>>) {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let connection = Builder::session()
        .and_then(|b| b.name(NAME))
        .and_then(|b| b.serve_at(PATH, Root))
        .and_then(|b| {
            b.serve_at(
                PATH,
                Player {
                    shared: shared.clone(),
                },
            )
        })
        .and_then(Builder::build);
    // No session bus (or the name is taken): drain and ignore.
    let Ok(connection) = connection else {
        while rx.recv().is_ok() {}
        return;
    };
    while let Ok(info) = rx.recv() {
        // Coalesce: only the latest state matters.
        let info = rx.try_iter().last().unwrap_or(info);
        let (before, seek) = {
            let mut state = lock(&shared);
            let before = state.info.clone();
            let seek = match (&before, &info) {
                (Some(old), Some(new)) => {
                    old.title == new.title
                        && super::position_jumped(
                            old,
                            state.at.map_or_else(Default::default, |at| at.elapsed()),
                            new,
                        )
                }
                _ => false,
            };
            state.info = info.clone();
            state.at = Some(Instant::now());
            (before, seek)
        };
        let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
        let status = match &info {
            Some(i) if i.playing => "Playing",
            Some(_) => "Paused",
            None => "Stopped",
        };
        changed.insert("PlaybackStatus", Value::from(status));
        changed.insert(
            "CanGoNext",
            Value::from(info.as_ref().is_some_and(|i| i.can_next)),
        );
        changed.insert(
            "CanGoPrevious",
            Value::from(info.as_ref().is_some_and(|i| i.can_previous)),
        );
        changed.insert("Rate", Value::from(info.as_ref().map_or(1.0, |i| i.rate)));
        if before
            .as_ref()
            .map(|b| (&b.title, &b.artist, b.duration_secs.round() as i64))
            != info
                .as_ref()
                .map(|i| (&i.title, &i.artist, i.duration_secs.round() as i64))
        {
            let meta = info.as_ref().map(metadata).unwrap_or_default();
            changed.insert("Metadata", Value::from(meta));
        }
        let _ = connection.emit_signal(
            None::<&str>,
            PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(PLAYER, changed, Vec::<&str>::new()),
        );
        if seek {
            let position = lock(&shared).position_us();
            let _ = connection.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(position,));
        }
    }
}
