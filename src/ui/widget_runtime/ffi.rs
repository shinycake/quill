//! The only `unsafe` of the widget runtime: Quill's C ABI over
//! widget-sandbox and TwidgetCore (`native/widget/quill_widget.h`), behind a
//! safe [`Native`] handle. Every string the C side returns is freed here;
//! every callback catches a panic before it could cross into C++.
use super::backend::Resources;
use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

#[repr(C)]
struct Widget {
    _opaque: [u8; 0],
}

/// `twlite::FontMetrics`, CSS px.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub height: f32,
    pub x_height: f32,
    pub zero_width: f32,
    pub em_ascent: f32,
    pub em_descent: f32,
}

#[repr(C)]
struct Callbacks {
    ctx: *mut c_void,
    event: extern "C" fn(*mut c_void, *const c_char, usize),
    create_font: extern "C" fn(
        *mut c_void,
        *const c_char,
        usize,
        f32,
        i32,
        bool,
        *const c_char,
        usize,
        *mut FontMetrics,
    ) -> i32,
    delete_font: extern "C" fn(*mut c_void, i32),
    text_width: extern "C" fn(*mut c_void, i32, *const c_char, usize) -> f32,
    load_image: extern "C" fn(*mut c_void, i32, *const c_char, usize, *mut f32, *mut f32) -> bool,
    canvas_run: extern "C" fn(*mut c_void, i32, i32, i32, *const c_char, usize),
    canvas_read: extern "C" fn(*mut c_void, i32, i32, i32, i32, i32, *mut u8, usize) -> bool,
    canvas_release: extern "C" fn(*mut c_void, i32),
}

/// `qw_state`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct State {
    pub dirty: bool,
    pub wants_tick: bool,
    pub frame_requested: bool,
    pub next_tick_in: f64,
    pub px_width: u32,
    pub px_height: u32,
}

unsafe extern "C" {
    fn qw_resources_init(
        sandbox_js: *const u8,
        sandbox_js_len: usize,
        sandbox_qbc: *const u8,
        sandbox_qbc_len: usize,
        validator_js: *const u8,
        validator_js_len: usize,
        validator_qbc: *const u8,
        validator_qbc_len: usize,
    ) -> bool;
    fn qw_create(
        html: *const c_char,
        html_len: usize,
        options: *const c_char,
        options_len: usize,
        callbacks: *const Callbacks,
        error: *mut *mut c_char,
    ) -> *mut Widget;
    fn qw_start(widget: *mut Widget);
    fn qw_destroy(widget: *mut Widget);
    fn qw_set_visible(widget: *mut Widget, visible: bool);
    fn qw_set_theme(widget: *mut Widget, theme: *const c_char, len: usize);
    fn qw_set_data(widget: *mut Widget, data: *const c_char, len: usize);
    fn qw_set_width(widget: *mut Widget, width: f64);
    fn qw_respond(widget: *mut Widget, token: u64, failure: i32, result: *const c_char, len: usize);
    fn qw_pointer(
        widget: *mut Widget,
        action: i32,
        x: f64,
        y: f64,
        pointer: i32,
        primary: bool,
        mouse: bool,
        modifiers: i32,
        button: i32,
    );
    fn qw_hover(widget: *mut Widget, action: i32, x: f64, y: f64, modifiers: i32);
    fn qw_selected_text(widget: *mut Widget) -> *mut c_char;
    fn qw_clear_selection(widget: *mut Widget) -> bool;
    fn qw_cursor(widget: *mut Widget) -> *mut c_char;
    fn qw_get_state(widget: *mut Widget, out: *mut State);
    fn qw_tick(widget: *mut Widget, time_ms: f64);
    fn qw_paint(widget: *mut Widget) -> *mut c_char;
    fn qw_trim(widget: *mut Widget);
    fn qw_string_free(s: *mut c_char);
}

const SANDBOX_JS: &[u8] =
    include_bytes!("../../../third_party/widgets/sandbox/packages/desktop/WidgetJS/sandbox-bundle.js");
const SANDBOX_QBC: &[u8] =
    include_bytes!("../../../third_party/widgets/sandbox/packages/desktop/WidgetJS/sandbox-bundle.qbc");
const VALIDATOR_JS: &[u8] =
    include_bytes!("../../../third_party/widgets/sandbox/packages/desktop/WidgetJS/validator-bundle.js");
const VALIDATOR_QBC: &[u8] =
    include_bytes!("../../../third_party/widgets/sandbox/packages/desktop/WidgetJS/validator-bundle.qbc");

/// Hands the WidgetJS bundles to the C side, once per process.
fn resources_ready() -> bool {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *READY.get_or_init(|| {
        // SAFETY: four valid slices that outlive the call; the C side copies them.
        unsafe {
            qw_resources_init(
                SANDBOX_JS.as_ptr(),
                SANDBOX_JS.len(),
                SANDBOX_QBC.as_ptr(),
                SANDBOX_QBC.len(),
                VALIDATOR_JS.as_ptr(),
                VALIDATOR_JS.len(),
                VALIDATOR_QBC.as_ptr(),
                VALIDATOR_QBC.len(),
            )
        }
    })
}

/// What a callback reaches: the widget thread's inbox and the fonts,
/// images and canvases the renderer and the painter share.
pub struct Shared {
    pub events: Box<dyn Fn(String) + Send + Sync>,
    pub resources: Arc<Mutex<Resources>>,
}

/// One live widget runtime. Not `Sync`: one thread drives it.
pub struct Native {
    widget: *mut Widget,
    /// Owns what `Callbacks::ctx` points to; dropped after `qw_destroy`.
    shared: *mut Shared,
}

// SAFETY: the C side allows the handle to move between threads (only one
// calls at a time, which `&mut self` on every call guarantees); `Shared` is
// `Send + Sync`.
unsafe impl Send for Native {}

/// Takes and frees a string the C side made.
fn take_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated string from `dup_string`, freed once below.
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: allocated by the C side's malloc and not used after this.
    unsafe { qw_string_free(ptr) };
    Some(text)
}

impl Native {
    /// Creates and starts the runtime for `html`; the reason when it cannot.
    pub fn create(html: &str, options: &str, shared: Shared) -> Result<Self, String> {
        if !resources_ready() {
            return Err("the widget bundles could not be loaded".into());
        }
        let shared = Box::into_raw(Box::new(shared));
        let callbacks = Callbacks {
            ctx: shared.cast(),
            event: on_event,
            create_font: on_create_font,
            delete_font: on_delete_font,
            text_width: on_text_width,
            load_image: on_load_image,
            canvas_run: on_canvas_run,
            canvas_read: on_canvas_read,
            canvas_release: on_canvas_release,
        };
        let mut error: *mut c_char = std::ptr::null_mut();
        // SAFETY: valid buffers for the call; the C side copies the callbacks
        // struct and keeps only `ctx`, which lives until `Drop`.
        let widget = unsafe {
            qw_create(
                html.as_ptr().cast(),
                html.len(),
                options.as_ptr().cast(),
                options.len(),
                &callbacks,
                &mut error,
            )
        };
        if widget.is_null() {
            // SAFETY: no widget holds `shared`; take it back to free it.
            drop(unsafe { Box::from_raw(shared) });
            return Err(take_string(error).unwrap_or_else(|| "the widget could not start".into()));
        }
        // SAFETY: a live widget.
        unsafe { qw_start(widget) };
        Ok(Self { widget, shared })
    }

    fn with_text(&mut self, text: &str, call: unsafe extern "C" fn(*mut Widget, *const c_char, usize)) {
        // SAFETY: a live widget and a valid buffer for the call.
        unsafe { call(self.widget, text.as_ptr().cast(), text.len()) }
    }

    pub fn set_visible(&mut self, visible: bool) {
        // SAFETY: a live widget.
        unsafe { qw_set_visible(self.widget, visible) }
    }
    pub fn set_theme(&mut self, theme: &str) {
        self.with_text(theme, qw_set_theme);
    }
    pub fn set_data(&mut self, data: &str) {
        self.with_text(data, qw_set_data);
    }
    pub fn set_width(&mut self, width: f64) {
        // SAFETY: a live widget.
        unsafe { qw_set_width(self.widget, width) }
    }

    /// Answers request `token`: `Ok(result)` or a failure code 1..=3.
    pub fn respond(&mut self, token: u64, outcome: Result<&str, i32>) {
        let (failure, text) = match outcome {
            Ok(text) => (0, text),
            Err(code) => (code, ""),
        };
        // SAFETY: a live widget and a valid buffer for the call.
        unsafe { qw_respond(self.widget, token, failure, text.as_ptr().cast(), text.len()) }
    }

    pub fn pointer(&mut self, action: i32, x: f64, y: f64, primary: bool, modifiers: i32, button: i32) {
        // SAFETY: a live widget.
        unsafe { qw_pointer(self.widget, action, x, y, 1, primary, true, modifiers, button) }
    }
    pub fn hover(&mut self, action: i32, x: f64, y: f64, modifiers: i32) {
        // SAFETY: a live widget.
        unsafe { qw_hover(self.widget, action, x, y, modifiers) }
    }
    pub fn selected_text(&mut self) -> String {
        // SAFETY: a live widget; the string is taken and freed.
        take_string(unsafe { qw_selected_text(self.widget) }).unwrap_or_default()
    }
    pub fn clear_selection(&mut self) -> bool {
        // SAFETY: a live widget.
        unsafe { qw_clear_selection(self.widget) }
    }
    pub fn cursor(&mut self) -> String {
        // SAFETY: a live widget; the string is taken and freed.
        take_string(unsafe { qw_cursor(self.widget) }).unwrap_or_default()
    }
    pub fn state(&mut self) -> State {
        let mut state = State::default();
        // SAFETY: a live widget and a valid out pointer.
        unsafe { qw_get_state(self.widget, &mut state) };
        state
    }
    pub fn tick(&mut self, time_ms: f64) {
        // SAFETY: a live widget.
        unsafe { qw_tick(self.widget, time_ms) }
    }
    /// The display list to paint, as JSON; `None` when nothing changed.
    pub fn paint(&mut self) -> Option<String> {
        // SAFETY: a live widget; the string is taken and freed.
        take_string(unsafe { qw_paint(self.widget) })
    }
    pub fn trim(&mut self) {
        // SAFETY: a live widget.
        unsafe { qw_trim(self.widget) }
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: the widget is live until here; after `qw_destroy` returns
        // no callback can reach `shared`, so it is freed last.
        unsafe {
            qw_destroy(self.widget);
            drop(Box::from_raw(self.shared));
        }
    }
}

/// `ctx` as the `Shared` it is.
fn shared<'a>(ctx: *mut c_void) -> Option<&'a Shared> {
    // SAFETY: `ctx` is the `Shared` boxed in `Native::create`, alive until
    // after `qw_destroy`, which waits for every callback.
    unsafe { ctx.cast::<Shared>().as_ref() }
}

fn text<'a>(ptr: *const c_char, len: usize) -> &'a str {
    if ptr.is_null() || len == 0 {
        return "";
    }
    // SAFETY: the C side passes `len` readable bytes valid for the call.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

/// Runs a callback body; a panic becomes `fallback`, never an unwind into C++.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

fn with_resources<T>(ctx: *mut c_void, fallback: T, body: impl FnOnce(&mut Resources) -> T) -> T {
    let Some(shared) = shared(ctx) else {
        return fallback;
    };
    guard(fallback, || {
        let mut resources = quill::locks::lock_or_recover(&shared.resources);
        body(&mut resources)
    })
}

extern "C" fn on_event(ctx: *mut c_void, json: *const c_char, len: usize) {
    guard((), || {
        if let Some(shared) = shared(ctx) {
            (shared.events)(text(json, len).to_owned());
        }
    });
}

#[allow(clippy::too_many_arguments)]
extern "C" fn on_create_font(
    ctx: *mut c_void,
    families: *const c_char,
    families_len: usize,
    size: f32,
    weight: i32,
    italic: bool,
    features: *const c_char,
    features_len: usize,
    out: *mut FontMetrics,
) -> i32 {
    with_resources(ctx, 0, |resources| {
        let (font, metrics) = resources.fonts.create(
            text(families, families_len),
            size,
            weight,
            italic,
            text(features, features_len),
        );
        // SAFETY: the C side passes a valid out pointer for the call.
        if let Some(out) = unsafe { out.as_mut() } {
            *out = metrics;
        }
        font
    })
}

extern "C" fn on_delete_font(ctx: *mut c_void, font: i32) {
    with_resources(ctx, (), |resources| resources.fonts.delete(font));
}

extern "C" fn on_text_width(ctx: *mut c_void, font: i32, value: *const c_char, len: usize) -> f32 {
    with_resources(ctx, 0.0, |resources| resources.fonts.width(font, text(value, len)))
}

extern "C" fn on_load_image(
    ctx: *mut c_void,
    id: i32,
    url: *const c_char,
    len: usize,
    width: *mut f32,
    height: *mut f32,
) -> bool {
    with_resources(ctx, false, |resources| {
        let Some((w, h)) = resources.images.load(id, text(url, len)) else {
            return false;
        };
        // SAFETY: valid out pointers for the call.
        unsafe {
            if let Some(out) = width.as_mut() {
                *out = w;
            }
            if let Some(out) = height.as_mut() {
                *out = h;
            }
        }
        true
    })
}

extern "C" fn on_canvas_run(ctx: *mut c_void, id: i32, width: i32, height: i32, ops: *const c_char, len: usize) {
    with_resources(ctx, (), |resources| resources.run_canvas(id, width, height, text(ops, len)));
}

#[allow(clippy::too_many_arguments)]
extern "C" fn on_canvas_read(
    ctx: *mut c_void,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    rgba: *mut u8,
    capacity: usize,
) -> bool {
    with_resources(ctx, false, |resources| {
        if rgba.is_null() {
            return false;
        }
        // SAFETY: the C side passes `capacity` writable bytes for the call.
        let out = unsafe { std::slice::from_raw_parts_mut(rgba, capacity) };
        resources.canvases.read(id, x, y, w, h, out)
    })
}

extern "C" fn on_canvas_release(ctx: *mut c_void, id: i32) {
    with_resources(ctx, (), |resources| resources.canvases.release(id));
}
