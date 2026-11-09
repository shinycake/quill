//! The runtime of one message widget: widget-sandbox's QuickJS sandbox and
//! TwidgetCore's litehtml renderer (C++, `native/widget/`), driven from one
//! thread of its own, and Quill's painter (tiny-skia, cosmic-text) turning
//! the renderer's display lists into frames GPUI draws. The UI talks to it
//! with [`Runtime`]: commands in, frames and events out, the [`WIDGETS`]
//! signal when there is something to take. Design and limits:
//! docs/decisions/message-widgets.md.
#![deny(clippy::indexing_slicing)]

mod backend;
mod canvas;
mod effects;
mod ffi;
mod fonts;
mod geometry;
mod images;
mod paint;
mod thread;

use gpui_kit::RenderImage;
use quill::wake::Signal;
use std::collections::BTreeMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

/// A runtime has frames or events for the UI.
pub static WIDGETS: Signal = Signal::new();

/// What a widget starts with.
pub struct Launch {
    pub html: String,
    /// `qw_create`'s options as JSON (`native/widget/quill_widget.cpp`).
    pub options: String,
    /// The bubble the widget is drawn on, opaque RGB: frames are flattened
    /// onto it.
    pub background: [u8; 3],
}

/// A pointer press, move or release over the widget, CSS px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// `action` 0 down, 1 move, 2 up, 3 cancel; `button` 0 primary, 2 secondary.
    Pointer {
        action: i32,
        x: f64,
        y: f64,
        modifiers: i32,
        button: i32,
    },
    /// `action` 0 enter/move, 1 leave.
    Hover { action: i32, x: f64, y: f64, modifiers: i32 },
}

enum Inbox {
    Native(String),
    Visible(bool),
    Theme { theme: String, background: [u8; 3] },
    Data(String),
    Width(f64),
    Respond { token: u64, outcome: Result<String, i32> },
    Input(Input),
    Tick(f64),
    Trim,
    Selection { clear: bool },
    Stop,
}

/// What a runtime tells the UI, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    /// The widget's scripts ran and it drew.
    Started,
    /// The height it wants, CSS px (already within 50–600).
    Height(f32),
    /// It stopped on a limit (CPU, memory, the validator); its last frame stays.
    Stopped(String),
    /// It could not start.
    Failed(String),
    /// `openLink` (`telegram` false) or `openTelegramLink`, after a press.
    Link { telegram: bool, url: String },
    /// Its storage area after a commit.
    Storage { area: String, values: BTreeMap<String, String> },
    /// What it claims of the press in progress: `hold` = a long press is its own.
    Gesture { hold: bool },
    /// A query for the bot; answer with [`Runtime::respond`].
    Request { token: u64, data: String },
    /// `copyText`.
    Copy(String),
    /// It wants (or stops wanting) its public data polled while visible.
    Watch(bool),
    /// The selected text, as asked with [`Runtime::selection`].
    Selection(String),
}

/// A painted frame: an opaque BGRA picture at device px, and its CSS size.
#[derive(Clone)]
pub struct Frame {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
}

/// What the runtime thread left for the UI.
#[derive(Default)]
pub struct Outbox {
    /// The newest frame since the last take.
    pub frame: Option<Frame>,
    pub events: Vec<RuntimeEvent>,
    /// The widget animates: tick it on every display frame.
    pub wants_frames: bool,
    /// The CSS cursor under the mouse.
    pub cursor: String,
}

/// The UI's handle on one runtime; dropping it stops the runtime.
pub struct Runtime {
    inbox: Sender<Inbox>,
    outbox: Arc<Mutex<Outbox>>,
}

impl Runtime {
    /// Starts `launch` on a thread of its own.
    pub fn start(launch: Launch) -> Self {
        let (inbox, rx) = channel();
        let outbox = Arc::new(Mutex::new(Outbox::default()));
        let runtime = Self {
            inbox: inbox.clone(),
            outbox: outbox.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("quill-widget".into())
            .spawn(move || thread::run(launch, inbox, rx, outbox.clone()));
        if let Err(err) = spawned {
            let mut out = quill::locks::lock_or_recover(&runtime.outbox);
            out.events.push(RuntimeEvent::Failed(err.to_string()));
            WIDGETS.poke();
        }
        runtime
    }

    fn send(&self, message: Inbox) {
        let _ = self.inbox.send(message);
    }

    pub fn set_visible(&self, visible: bool) {
        self.send(Inbox::Visible(visible));
    }
    /// The theme (`WidgetTheme` JSON) and the bubble color under it.
    pub fn set_theme(&self, theme: String, background: [u8; 3]) {
        self.send(Inbox::Theme { theme, background });
    }
    /// New `chat_data` / `public_data` (`WidgetData` JSON).
    pub fn set_data(&self, data: String) {
        self.send(Inbox::Data(data));
    }
    pub fn set_width(&self, width: f64) {
        self.send(Inbox::Width(width));
    }
    pub fn respond(&self, token: u64, outcome: Result<String, i32>) {
        self.send(Inbox::Respond { token, outcome });
    }
    pub fn input(&self, input: Input) {
        self.send(Inbox::Input(input));
    }
    /// One display frame, `time_ms` on the widget's clock.
    pub fn tick(&self, time_ms: f64) {
        self.send(Inbox::Tick(time_ms));
    }
    /// Off screen: drop what can be made again.
    pub fn trim(&self) {
        self.send(Inbox::Trim);
    }
    /// Asks for the selected text ([`RuntimeEvent::Selection`]); `clear` drops the selection after.
    pub fn selection(&self, clear: bool) {
        self.send(Inbox::Selection { clear });
    }

    /// What the runtime left since the last take.
    pub fn take(&self) -> Outbox {
        let mut out = quill::locks::lock_or_recover(&self.outbox);
        Outbox {
            frame: out.frame.take(),
            events: std::mem::take(&mut out.events),
            wants_frames: out.wants_frames,
            cursor: out.cursor.clone(),
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.send(Inbox::Stop);
    }
}

/// Milliseconds on the widgets' clock (`performance.now()`), from the
/// first call in the process.
pub fn clock_ms() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod smoke_tests;
