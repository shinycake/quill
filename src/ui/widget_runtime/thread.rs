//! The runtime thread: owns the native widget from `qw_create` to
//! `qw_destroy`, applies the UI's commands, ticks on its own only when the
//! widget named a time (`next_tick_in`) and otherwise sleeps, and paints
//! whenever the document changed.
use super::backend::Resources;
use super::ffi::{Native, Shared};
use super::{Frame, Inbox, Input, Launch, Outbox, RuntimeEvent, WIDGETS, clock_ms, paint};
use gpui_kit::RenderImage;
use serde_json::Value;
use smallvec::SmallVec;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_skia::{Color, Pixmap, Rect};

/// What the thread keeps between paints.
pub(super) struct Canvas {
    pub(super) pixmap: Option<Pixmap>,
    background: [u8; 3],
    /// The next paint is a whole one (a new background, a trim).
    whole: bool,
    /// The mouse moved: read the cursor again.
    hovered: bool,
    /// A window shows it. Off screen it does not paint (round 18 C #2: a
    /// hidden widget's 1 Hz timers painted and redrew the history it left);
    /// coming back paints a whole frame.
    shown: bool,
}

impl Canvas {
    pub(super) fn new(background: [u8; 3]) -> Self {
        Self {
            pixmap: None,
            background,
            whole: true,
            hovered: false,
            shown: true,
        }
    }
}

/// How far the UI has heard the widget get. `Stopped` is final: a dead
/// widget is never reported as started afterwards.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ToldStage {
    #[default]
    Loading,
    Started,
    Stopped,
}

/// What the thread has told the UI already, to say only changes.
#[derive(Default)]
struct Told {
    stage: ToldStage,
    height: f32,
}

pub(super) fn run(
    launch: Launch,
    inbox: Sender<Inbox>,
    rx: Receiver<Inbox>,
    outbox: Arc<Mutex<Outbox>>,
) {
    let resources = Arc::new(Mutex::new(Resources::default()));
    let events = Shared {
        events: Box::new(move |json| {
            let _ = inbox.send(Inbox::Native(json));
        }),
        resources: resources.clone(),
    };
    let publish = |event: RuntimeEvent| {
        quill::locks::lock_or_recover(&outbox).events.push(event);
        WIDGETS.poke();
    };
    let mut native = match Native::create(&launch.html, &launch.options, events) {
        Ok(native) => native,
        Err(reason) => {
            publish(RuntimeEvent::Failed(reason));
            return;
        }
    };
    let mut canvas = Canvas::new(launch.background);
    let mut told = Told::default();
    let mut deadline: Option<Instant> = None;
    loop {
        let first = match deadline {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            },
        };
        let mut stop = false;
        match first {
            Some(message) => stop |= apply(&mut native, &mut canvas, &mut told, message, &publish),
            // The time the widget named came: one frame.
            None => native.tick(clock_ms()),
        }
        // Everything else waiting, before one paint for all of it.
        while !stop && let Ok(message) = rx.try_recv() {
            stop |= apply(&mut native, &mut canvas, &mut told, message, &publish);
        }
        if stop {
            break;
        }
        let state = native.state();
        if canvas.shown && (state.dirty || canvas.whole) {
            let frame = paint_frame(&mut native, &resources, &mut canvas);
            if let Some(frame) = frame {
                quill::locks::lock_or_recover(&outbox).frame = Some(frame);
                WIDGETS.poke();
            }
        }
        let wants_frames = state.wants_tick || state.frame_requested;
        deadline = (!wants_frames && state.next_tick_in.is_finite() && state.next_tick_in >= 0.0)
            .then(|| Instant::now() + Duration::from_secs_f64(state.next_tick_in.min(3600.0)));
        let cursor = std::mem::take(&mut canvas.hovered).then(|| native.cursor());
        let mut out = quill::locks::lock_or_recover(&outbox);
        if let Some(cursor) = cursor
            && out.cursor != cursor
        {
            out.cursor = cursor;
            WIDGETS.poke();
        }
        if out.wants_frames != wants_frames {
            out.wants_frames = wants_frames;
            WIDGETS.poke();
        }
    }
    // `qw_destroy` runs here, on the thread that made it.
    drop(native);
}

/// Applies one message; true when the runtime must stop.
fn apply(
    native: &mut Native,
    canvas: &mut Canvas,
    told: &mut Told,
    message: Inbox,
    publish: &impl Fn(RuntimeEvent),
) -> bool {
    match message {
        Inbox::Native(json) => {
            for event in events(&json, told) {
                publish(event);
            }
        }
        Inbox::Visible(visible) => {
            native.set_visible(visible);
            canvas.shown = visible;
            canvas.whole |= visible;
        }
        Inbox::Theme { theme, background } => {
            native.set_theme(&theme);
            if canvas.background != background {
                canvas.background = background;
                canvas.whole = true;
            }
        }
        Inbox::Data(data) => native.set_data(&data),
        Inbox::Width(width) => native.set_width(width),
        Inbox::Respond { token, outcome } => {
            native.respond(token, outcome.as_deref().map_err(|code| *code))
        }
        Inbox::Input(Input::Pointer {
            action,
            x,
            y,
            modifiers,
            button,
        }) => native.pointer(action, x, y, button == 0, modifiers, button),
        Inbox::Input(Input::Hover {
            action,
            x,
            y,
            modifiers,
        }) => {
            native.hover(action, x, y, modifiers);
            canvas.hovered = true;
        }
        Inbox::Tick(time) => native.tick(time),
        Inbox::Trim => {
            native.trim();
            canvas.pixmap = None;
            canvas.whole = true;
        }
        Inbox::Selection { clear } => {
            let text = native.selected_text();
            if clear {
                native.clear_selection();
            }
            publish(RuntimeEvent::Selection(text));
        }
        Inbox::Stop => return true,
    }
    false
}

/// The UI's events in one native event.
fn events(json: &str, told: &mut Told) -> Vec<RuntimeEvent> {
    let Ok(event) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let text = |key: &str| {
        event
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut out = Vec::new();
    match event.get("type").and_then(Value::as_str).unwrap_or("") {
        "change" => {
            if let Some(dead) = event.get("dead").and_then(Value::as_str)
                && told.stage != ToldStage::Stopped
            {
                told.stage = ToldStage::Stopped;
                out.push(RuntimeEvent::Stopped(dead.to_owned()));
            }
            let height = event.get("height").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            if height > 0.0 && (height - told.height).abs() >= 0.5 {
                told.height = height;
                out.push(RuntimeEvent::Height(height));
            }
            if event.get("rendered").and_then(Value::as_bool) == Some(true)
                && told.stage == ToldStage::Loading
            {
                told.stage = ToldStage::Started;
                out.push(RuntimeEvent::Started);
            }
        }
        "action" => match event.get("kind").and_then(Value::as_str) {
            Some("openLink") => out.push(RuntimeEvent::Link {
                telegram: false,
                url: text("url"),
            }),
            Some("openTelegramLink") => out.push(RuntimeEvent::Link {
                telegram: true,
                url: text("url"),
            }),
            // Haptics do nothing on a desktop; `ready` needs nothing.
            _ => {}
        },
        "storage" => out.push(RuntimeEvent::Storage {
            area: text("area"),
            values: event
                .get("values")
                .and_then(Value::as_object)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "gesture" => out.push(RuntimeEvent::Gesture {
            hold: event.get("hold").and_then(Value::as_bool).unwrap_or(false),
        }),
        "request" => {
            let token = event.get("token").and_then(Value::as_f64).unwrap_or(0.0);
            if token >= 1.0 {
                out.push(RuntimeEvent::Request {
                    token: token as u64,
                    data: text("data"),
                });
            }
        }
        "copy" => out.push(RuntimeEvent::Copy(text("text"))),
        "watch" => out.push(RuntimeEvent::Watch(
            event.get("on").and_then(Value::as_bool).unwrap_or(false),
        )),
        _ => {}
    }
    out
}

/// Paints the document if it changed; the frame to show, if any.
pub(super) fn paint_frame(
    native: &mut Native,
    resources: &Mutex<Resources>,
    canvas: &mut Canvas,
) -> Option<Frame> {
    let json = native.paint()?;
    let frame: Value = serde_json::from_str(&json).ok()?;
    let width = u32::try_from(frame.get("w")?.as_u64()?).ok()?;
    let height = u32::try_from(frame.get("h")?.as_u64()?).ok()?;
    let dpr = frame
        .get("dpr")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .max(0.25) as f32;
    let list = frame.get("list")?.as_array()?;
    let fits = canvas
        .pixmap
        .as_ref()
        .is_some_and(|pixmap| pixmap.width() == width && pixmap.height() == height);
    if !fits {
        canvas.pixmap = Some(Pixmap::new(width, height)?);
        canvas.whole = true;
    }
    let damage: Option<Vec<Rect>> = if canvas.whole {
        None
    } else {
        frame.get("damage").and_then(Value::as_array).map(|rects| {
            rects
                .iter()
                .filter_map(|r| {
                    let r: Vec<f32> = r
                        .as_array()?
                        .iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect();
                    Rect::from_xywh(*r.first()?, *r.get(1)?, *r.get(2)?, *r.get(3)?)
                })
                .collect()
        })
    };
    if damage.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    canvas.whole = false;
    let pixmap = canvas.pixmap.as_mut()?;
    let [r, g, b] = canvas.background;
    {
        let resources = quill::locks::lock_or_recover(resources);
        paint::paint(
            &resources,
            list,
            dpr,
            damage.as_deref(),
            Color::from_rgba8(r, g, b, 255),
            pixmap,
        );
    }
    // Opaque over the bubble, so premultiplied is straight: only the
    // channel order changes for GPUI (BGRA).
    let mut bgra = pixmap.data().to_vec();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(Frame {
        image: Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
            buffer,
        )]))),
        width: width as f32 / dpr,
        height: height as f32 / dpr,
    })
}
