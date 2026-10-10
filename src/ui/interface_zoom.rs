//! Interface scale for the whole UI (`parity:appearance-scale`).
//!
//! Telegram Desktop multiplies every style metric by `style::Scale`, so
//! text, avatars, row heights, paddings and icons all grow together. GPUI
//! has no such switch, and Quill's views size most things in fixed `px()`.
//! Instead of rewriting those call sites, Quill wraps GPUI's platform:
//! [`ZoomPlatform`] decorates the OS platform and every window it opens is
//! a [`ZoomWindow`], which tells GPUI the window is `zoom` times denser
//! than the display is and `zoom` times smaller in logical pixels:
//!
//! - `scale_factor()` reports `display scale × zoom`, so GPUI rasterizes
//!   glyphs, SVG icons and images at the real device resolution (sharp at
//!   any scale) and snaps layout to physical pixels;
//! - `content_size()`, `mouse_position()` and every input event position
//!   are divided by `zoom`, so layout and hit testing work in the zoomed
//!   coordinate space;
//! - geometry GPUI hands back to the OS (window resize, the Linux client
//!   inset and input region, the window menu position, the IME caret,
//!   macOS traffic lights, the minimum window size) is multiplied by
//!   `zoom`.
//!
//! The scene GPUI paints is already in device pixels, so drawing passes
//! straight through. Screen-space geometry (window bounds, displays) is
//! untouched: the window keeps its size on screen and its content scales.
//!
//! The decorator must forward every trait method, including the ones with
//! default bodies (the vendored platforms override some of them, e.g.
//! `frame_waker` for idle frames). `forwards_every_trait_method` checks
//! this against the GPUI source after a GPUI upgrade.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use futures_channel::oneshot;
use gpui_kit::{
    A11yCallbacks, Action, ActivityGuard, AnyWindowHandle, AppLifecyclePhase, BackgroundExecutor,
    Bounds, Capslock, ClipboardItem, ClipboardReadError, CursorStyle, Decorations,
    DispatchEventResult, ExternalDragPayload, FileDropEvent, ForegroundExecutor, GpuSpecs, Keymap,
    Menu, MenuItem, Modifiers, OwnedMenu, PathPromptOptions, Pixels, Platform, PlatformAtlas,
    PlatformDisplay, PlatformGestures, PlatformInput, PlatformInputHandler, PlatformKeyboardLayout,
    PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point, PromptButton, PromptLevel,
    RequestFrameOptions, ResizeEdge, Scene, ScreenCaptureSource, ScrollDelta, Size,
    SystemNotification, SystemNotificationResponse, SystemWindowTab, Task, TextInputConfiguration,
    TextInputStateChange, ThermalState, WindowAppearance, WindowBackgroundAppearance, WindowBounds,
    WindowButtonLayout, WindowControlArea, WindowControls, WindowDecorations, WindowInsets,
    WindowParams, WindowVisibility, px,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

type AnyResult<T> = gpui_kit::Result<T>;

/// The zoom factor for a stored interface scale percentage.
pub(crate) fn zoom_for_percent(pct: u16) -> f32 {
    f32::from(quill::settings::clamp_interface_scale(pct)) / 100.
}

/// Screenshot demos: `QUILL_DEMO_INTERFACE_SCALE=150` opens the demo at
/// that interface scale (percent, clamped like the setting).
pub(crate) fn demo_interface_scale() -> Option<u16> {
    let pct = std::env::var("QUILL_DEMO_INTERFACE_SCALE")
        .ok()?
        .trim()
        .parse::<u16>()
        .ok()?;
    Some(quill::settings::clamp_interface_scale(pct))
}

thread_local! {
    /// The zoom new windows open with. GPUI's platform lives on the main
    /// thread, and so does everything here.
    static ZOOM: Cell<f32> = const { Cell::new(1.) };
    /// Every open zoomed window, to re-lay them out when the zoom changes.
    static WINDOWS: RefCell<Vec<Weak<WindowZoom>>> = const { RefCell::new(Vec::new()) };
}

/// The current interface zoom (1.0 = 100%).
pub(crate) fn current_zoom() -> f32 {
    ZOOM.with(Cell::get)
}

/// Set the interface zoom before any window opens (startup).
pub(crate) fn set_initial_zoom(zoom: f32) {
    ZOOM.with(|z| z.set(sanitize(zoom)));
}

/// Change the interface zoom of every open window, live.
///
/// GPUI re-reads the window geometry through the resize callback, which
/// needs the window to be free: call this outside a window update (for
/// example from `App::defer`), or GPUI skips the relayout until the next
/// real resize.
pub(crate) fn set_zoom(zoom: f32) {
    let zoom = sanitize(zoom);
    if (current_zoom() - zoom).abs() < f32::EPSILON {
        return;
    }
    ZOOM.with(|z| z.set(zoom));
    let windows: Vec<Rc<WindowZoom>> = WINDOWS.with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|w| w.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    });
    for window in windows {
        window.zoom.set(zoom);
        window.fire_resize();
    }
}

fn sanitize(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(0.5, 4.)
    } else {
        1.
    }
}

/// GPUI logical pixels → the platform's logical pixels.
fn out(v: Pixels, zoom: f32) -> Pixels {
    px(f32::from(v) * zoom)
}

/// The platform's logical pixels → GPUI logical pixels.
fn inn(v: Pixels, zoom: f32) -> Pixels {
    px(f32::from(v) / zoom)
}

fn point_in(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    p.map(|v| inn(v, zoom))
}

fn point_out(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    p.map(|v| out(v, zoom))
}

/// Half the height of a macOS traffic-light button frame, in points.
const TRAFFIC_LIGHT_HALF: f32 = 8.;

/// Where to put the macOS traffic lights so they stay centered on the
/// zoomed title bar: the buttons themselves don't scale, so the point
/// that scales is their center, not their corner.
fn traffic_light_out(p: Point<Pixels>, zoom: f32) -> Point<Pixels> {
    let y = (f32::from(p.y) + TRAFFIC_LIGHT_HALF) * zoom - TRAFFIC_LIGHT_HALF;
    Point::new(out(p.x, zoom), px(y))
}

fn size_in(s: Size<Pixels>, zoom: f32) -> Size<Pixels> {
    s.map(|v| inn(v, zoom))
}

fn size_out(s: Size<Pixels>, zoom: f32) -> Size<Pixels> {
    s.map(|v| out(v, zoom))
}

fn bounds_in(b: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    b.map(|v| inn(v, zoom))
}

fn bounds_out(b: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    b.map(|v| out(v, zoom))
}

/// Map a platform input event into the zoomed coordinate space: positions
/// and pixel scroll deltas shrink by `zoom` (content follows the fingers on
/// a trackpad); line deltas, keys and pinch amounts are unchanged.
pub(crate) fn zoom_input(input: PlatformInput, zoom: f32) -> PlatformInput {
    if (zoom - 1.).abs() < f32::EPSILON {
        return input;
    }
    let p = |p: Point<Pixels>| point_in(p, zoom);
    match input {
        PlatformInput::MouseDown(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseDown(e)
        }
        PlatformInput::MouseUp(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseUp(e)
        }
        PlatformInput::MousePressure(mut e) => {
            e.position = p(e.position);
            PlatformInput::MousePressure(e)
        }
        PlatformInput::MouseMove(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseMove(e)
        }
        PlatformInput::MouseExited(mut e) => {
            e.position = p(e.position);
            PlatformInput::MouseExited(e)
        }
        PlatformInput::ScrollWheel(mut e) => {
            e.position = p(e.position);
            if let ScrollDelta::Pixels(delta) = e.delta {
                e.delta = ScrollDelta::Pixels(p(delta));
            }
            PlatformInput::ScrollWheel(e)
        }
        PlatformInput::Pinch(mut e) => {
            e.position = p(e.position);
            PlatformInput::Pinch(e)
        }
        PlatformInput::LongPress(mut e) => {
            e.start_position = p(e.start_position);
            e.position = p(e.position);
            PlatformInput::LongPress(e)
        }
        PlatformInput::TouchDrag(mut e) => {
            e.start_position = p(e.start_position);
            e.position = p(e.position);
            PlatformInput::TouchDrag(e)
        }
        PlatformInput::Touch(mut e) => {
            e.position = p(e.position);
            e.predicted_position = e.predicted_position.map(p);
            PlatformInput::Touch(e)
        }
        PlatformInput::FileDrop(event) => PlatformInput::FileDrop(match event {
            FileDropEvent::Entered { position, paths } => FileDropEvent::Entered {
                position: p(position),
                paths,
            },
            FileDropEvent::Pending { position } => FileDropEvent::Pending {
                position: p(position),
            },
            FileDropEvent::Submit { position } => FileDropEvent::Submit {
                position: p(position),
            },
            other => other,
        }),
        other @ (PlatformInput::KeyDown(_)
        | PlatformInput::KeyUp(_)
        | PlatformInput::ModifiersChanged(_)) => other,
    }
}

type ResizeCallback = Box<dyn FnMut(Size<Pixels>, f32)>;

/// Per-window zoom state shared between the window wrapper, its callbacks
/// and [`set_zoom`].
struct WindowZoom {
    zoom: Cell<f32>,
    /// GPUI's resize callback; [`set_zoom`] calls it so GPUI re-reads the
    /// viewport and scale factor.
    resize: RefCell<Option<ResizeCallback>>,
    /// The platform's last reported content size and display scale.
    raw_size: Cell<Size<Pixels>>,
    raw_scale: Cell<f32>,
    /// macOS traffic-light position in GPUI pixels, and the zoom it was
    /// last applied at.
    traffic_light: Cell<Option<Point<Pixels>>>,
    traffic_light_zoom: Cell<f32>,
}

impl WindowZoom {
    /// State for a newly opened window, registered for [`set_zoom`].
    fn register(
        zoom: f32,
        raw_size: Size<Pixels>,
        raw_scale: f32,
        traffic_light: Option<Point<Pixels>>,
    ) -> Rc<Self> {
        let state = Rc::new(WindowZoom {
            zoom: Cell::new(zoom),
            resize: RefCell::new(None),
            raw_size: Cell::new(raw_size),
            raw_scale: Cell::new(raw_scale),
            traffic_light: Cell::new(traffic_light),
            traffic_light_zoom: Cell::new(zoom),
        });
        WINDOWS.with(|list| {
            let mut list = list.borrow_mut();
            list.retain(|w| w.strong_count() > 0);
            list.push(Rc::downgrade(&state));
        });
        state
    }

    fn fire_resize(&self) {
        // Take the callback out while it runs: GPUI's handler reads the
        // window back through the wrapper, and a nested zoom change must
        // not find the cell borrowed.
        let Some(mut callback) = self.resize.borrow_mut().take() else {
            return;
        };
        let zoom = self.zoom.get();
        callback(
            size_in(self.raw_size.get(), zoom),
            self.raw_scale.get() * zoom,
        );
        let mut slot = self.resize.borrow_mut();
        if slot.is_none() {
            *slot = Some(callback);
        }
    }
}

/// A platform whose windows follow the interface zoom.
pub(crate) struct ZoomPlatform {
    inner: Rc<dyn Platform>,
}

impl ZoomPlatform {
    pub(crate) fn new(inner: Rc<dyn Platform>) -> Self {
        Self { inner }
    }
}

/// A window whose content is drawn `zoom` times larger.
pub(crate) struct ZoomWindow {
    inner: Box<dyn PlatformWindow>,
    state: Rc<WindowZoom>,
}

impl ZoomWindow {
    fn zoom(&self) -> f32 {
        self.state.zoom.get()
    }

    /// Re-apply zoom-dependent native geometry after a zoom change.
    fn sync_native_geometry(&self) {
        let zoom = self.zoom();
        if (self.state.traffic_light_zoom.get() - zoom).abs() < f32::EPSILON {
            return;
        }
        self.state.traffic_light_zoom.set(zoom);
        let traffic_light = self.state.traffic_light.get();
        #[cfg(target_os = "macos")]
        if let Some(position) = traffic_light {
            self.inner
                .set_traffic_light_position(traffic_light_out(position, zoom));
        }
        // Only macOS has traffic lights.
        #[cfg(not(target_os = "macos"))]
        let _ = traffic_light;
    }
}

impl HasWindowHandle for ZoomWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.inner.window_handle()
    }
}

impl HasDisplayHandle for ZoomWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.inner.display_handle()
    }
}

impl Platform for ZoomPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.inner.background_executor()
    }
    fn foreground_executor(&self) -> ForegroundExecutor {
        self.inner.foreground_executor()
    }
    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.inner.text_system()
    }
    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        self.inner.run(on_finish_launching)
    }
    fn quit(&self) {
        self.inner.quit()
    }
    fn restart(&self, binary_path: Option<PathBuf>, arguments: Vec<OsString>) {
        self.inner.restart(binary_path, arguments)
    }
    fn activate(&self, ignoring_other_apps: bool) {
        self.inner.activate(ignoring_other_apps)
    }
    fn hide(&self) {
        self.inner.hide()
    }
    fn hide_other_apps(&self) {
        self.inner.hide_other_apps()
    }
    fn unhide_other_apps(&self) {
        self.inner.unhide_other_apps()
    }
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        self.inner.displays()
    }
    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.inner.primary_display()
    }
    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.inner.active_window()
    }
    fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        self.inner.window_stack()
    }
    fn is_screen_capture_supported(&self) -> bool {
        self.inner.is_screen_capture_supported()
    }
    fn screen_capture_sources(
        &self,
    ) -> oneshot::Receiver<AnyResult<Vec<Rc<dyn ScreenCaptureSource>>>> {
        self.inner.screen_capture_sources()
    }
    fn open_window(
        &self,
        handle: AnyWindowHandle,
        mut options: WindowParams,
    ) -> AnyResult<Box<dyn PlatformWindow>> {
        let zoom = current_zoom();
        // The minimum size and traffic lights are in GPUI pixels; the
        // window bounds are screen geometry and stay as they are.
        options.window_min_size = options.window_min_size.map(|s| size_out(s, zoom));
        let traffic_light = options
            .titlebar
            .as_ref()
            .and_then(|t| t.traffic_light_position);
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.traffic_light_position = titlebar
                .traffic_light_position
                .map(|p| traffic_light_out(p, zoom));
        }
        let inner = self.inner.open_window(handle, options)?;
        let state = WindowZoom::register(
            zoom,
            inner.content_size(),
            inner.scale_factor(),
            traffic_light,
        );
        Ok(Box::new(ZoomWindow { inner, state }))
    }
    fn window_appearance(&self) -> WindowAppearance {
        self.inner.window_appearance()
    }
    fn set_window_appearance(&self, appearance: Option<WindowAppearance>) {
        self.inner.set_window_appearance(appearance)
    }
    fn button_layout(&self) -> Option<WindowButtonLayout> {
        self.inner.button_layout()
    }
    fn open_url(&self, url: &str) {
        self.inner.open_url(url)
    }
    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.inner.on_open_urls(callback)
    }
    fn register_url_scheme(&self, url: &str) -> Task<AnyResult<()>> {
        self.inner.register_url_scheme(url)
    }
    fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<AnyResult<Option<Vec<PathBuf>>>> {
        self.inner.prompt_for_paths(options)
    }
    fn prompt_for_new_path(
        &self,
        directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<AnyResult<Option<PathBuf>>> {
        self.inner.prompt_for_new_path(directory, suggested_name)
    }
    fn can_select_mixed_files_and_dirs(&self) -> bool {
        self.inner.can_select_mixed_files_and_dirs()
    }
    fn reveal_path(&self, path: &Path) {
        self.inner.reveal_path(path)
    }
    fn open_with_system(&self, path: &Path) {
        self.inner.open_with_system(path)
    }
    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.inner.on_quit(callback)
    }
    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_reopen(callback)
    }
    fn on_system_sleep(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_system_sleep(callback)
    }
    fn on_system_wake(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_system_wake(callback)
    }
    fn on_app_lifecycle(&self, callback: Box<dyn FnMut(AppLifecyclePhase)>) {
        self.inner.on_app_lifecycle(callback)
    }
    fn on_memory_warning(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_memory_warning(callback)
    }
    fn gestures(&self) -> Option<Rc<dyn PlatformGestures>> {
        self.inner.gestures()
    }
    fn set_menus(&self, menus: Vec<Menu>, keymap: &Keymap) {
        self.inner.set_menus(menus, keymap)
    }
    fn get_menus(&self) -> Option<Vec<OwnedMenu>> {
        self.inner.get_menus()
    }
    fn set_dock_menu(&self, menu: Vec<MenuItem>, keymap: &Keymap) {
        self.inner.set_dock_menu(menu, keymap)
    }
    fn perform_dock_menu_action(&self, action: usize) {
        self.inner.perform_dock_menu_action(action)
    }
    fn add_recent_document(&self, path: &Path) {
        self.inner.add_recent_document(path)
    }
    fn update_jump_list(
        &self,
        menus: Vec<MenuItem>,
        entries: Vec<smallvec::SmallVec<[PathBuf; 2]>>,
    ) -> Task<Vec<smallvec::SmallVec<[PathBuf; 2]>>> {
        self.inner.update_jump_list(menus, entries)
    }
    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {
        self.inner.on_app_menu_action(callback)
    }
    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_will_open_app_menu(callback)
    }
    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        self.inner.on_validate_app_menu_command(callback)
    }
    fn thermal_state(&self) -> ThermalState {
        self.inner.thermal_state()
    }
    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_thermal_state_change(callback)
    }
    fn prevent_idle_sleep(&self, reason: &str) -> Task<AnyResult<ActivityGuard>> {
        self.inner.prevent_idle_sleep(reason)
    }
    fn set_app_identity(&self, identifier: &str, name: &str) {
        self.inner.set_app_identity(identifier, name)
    }
    fn show_system_notification(&self, notification: SystemNotification) {
        self.inner.show_system_notification(notification)
    }
    fn dismiss_system_notification(&self, tag: &str) {
        self.inner.dismiss_system_notification(tag)
    }
    fn on_system_notification_response(
        &self,
        callback: Box<dyn FnMut(SystemNotificationResponse)>,
    ) {
        self.inner.on_system_notification_response(callback)
    }
    fn compositor_name(&self) -> &'static str {
        self.inner.compositor_name()
    }
    fn app_path(&self) -> AnyResult<PathBuf> {
        self.inner.app_path()
    }
    fn path_for_auxiliary_executable(&self, name: &str) -> AnyResult<PathBuf> {
        self.inner.path_for_auxiliary_executable(name)
    }
    fn set_cursor_style(&self, style: CursorStyle) {
        self.inner.set_cursor_style(style)
    }
    fn hide_cursor_until_mouse_moves(&self) {
        self.inner.hide_cursor_until_mouse_moves()
    }
    fn is_cursor_visible(&self) -> bool {
        self.inner.is_cursor_visible()
    }
    fn should_auto_hide_scrollbars(&self) -> bool {
        self.inner.should_auto_hide_scrollbars()
    }
    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.inner.read_from_clipboard()
    }
    fn write_to_clipboard(&self, item: ClipboardItem) {
        self.inner.write_to_clipboard(item)
    }
    fn read_from_clipboard_async(&self) -> Task<Result<Option<ClipboardItem>, ClipboardReadError>> {
        self.inner.read_from_clipboard_async()
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn read_from_primary(&self) -> Option<ClipboardItem> {
        self.inner.read_from_primary()
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn write_to_primary(&self, item: ClipboardItem) {
        self.inner.write_to_primary(item)
    }
    #[cfg(target_os = "macos")]
    fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> {
        self.inner.read_from_find_pasteboard()
    }
    #[cfg(target_os = "macos")]
    fn write_to_find_pasteboard(&self, item: ClipboardItem) {
        self.inner.write_to_find_pasteboard(item)
    }
    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<AnyResult<()>> {
        self.inner.write_credentials(url, username, password)
    }
    fn read_credentials(&self, url: &str) -> Task<AnyResult<Option<(String, Vec<u8>)>>> {
        self.inner.read_credentials(url)
    }
    fn delete_credentials(&self, url: &str) -> Task<AnyResult<()>> {
        self.inner.delete_credentials(url)
    }
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        self.inner.keyboard_layout()
    }
    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        self.inner.keyboard_mapper()
    }
    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_keyboard_layout_change(callback)
    }
}

impl PlatformWindow for ZoomWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.inner.bounds()
    }
    fn is_maximized(&self) -> bool {
        self.inner.is_maximized()
    }
    fn window_bounds(&self) -> WindowBounds {
        self.inner.window_bounds()
    }
    fn content_size(&self) -> Size<Pixels> {
        size_in(self.inner.content_size(), self.zoom())
    }
    fn visual_viewport_bounds(&self) -> Bounds<Pixels> {
        bounds_in(self.inner.visual_viewport_bounds(), self.zoom())
    }
    fn on_visual_viewport_changed(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_visual_viewport_changed(callback)
    }
    fn prepare_frame(&self) -> bool {
        self.inner.prepare_frame()
    }
    fn resize(&mut self, size: Size<Pixels>) {
        let zoom = self.zoom();
        self.inner.resize(size_out(size, zoom))
    }
    fn scale_factor(&self) -> f32 {
        self.inner.scale_factor() * self.zoom()
    }
    fn appearance(&self) -> WindowAppearance {
        self.inner.appearance()
    }
    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.inner.display()
    }
    fn mouse_position(&self) -> Point<Pixels> {
        point_in(self.inner.mouse_position(), self.zoom())
    }
    fn modifiers(&self) -> Modifiers {
        self.inner.modifiers()
    }
    fn capslock(&self) -> Capslock {
        self.inner.capslock()
    }
    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.inner.set_input_handler(input_handler)
    }
    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.inner.take_input_handler()
    }
    fn set_text_input_configuration(&mut self, configuration: TextInputConfiguration) {
        self.inner.set_text_input_configuration(configuration)
    }
    fn prompt(
        &self,
        level: PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        self.inner.prompt(level, msg, detail, answers)
    }
    fn activate(&self) {
        self.inner.activate()
    }
    fn request_attention(&self) {
        self.inner.request_attention()
    }
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
    fn visibility(&self) -> WindowVisibility {
        self.inner.visibility()
    }
    fn is_hovered(&self) -> bool {
        self.inner.is_hovered()
    }
    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.inner.background_appearance()
    }
    fn set_title(&mut self, title: &str) {
        self.inner.set_title(title)
    }
    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.inner.set_background_appearance(background_appearance)
    }
    fn minimize(&self) {
        self.inner.minimize()
    }
    fn zoom(&self) {
        self.inner.zoom()
    }
    fn toggle_fullscreen(&self) {
        self.inner.toggle_fullscreen()
    }
    fn is_fullscreen(&self) -> bool {
        self.inner.is_fullscreen()
    }
    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        self.inner.frame_waker()
    }
    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.inner.on_request_frame(callback)
    }
    fn on_input(&self, mut callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        let state = Rc::downgrade(&self.state);
        self.inner.on_input(Box::new(move |input| {
            let zoom = state.upgrade().map_or(1., |s| s.zoom.get());
            callback(zoom_input(input, zoom))
        }))
    }
    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner.on_active_status_change(callback)
    }
    fn on_visibility_change(&self, callback: Box<dyn FnMut(WindowVisibility)>) {
        self.inner.on_visibility_change(callback)
    }
    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner.on_hover_status_change(callback)
    }
    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        *self.state.resize.borrow_mut() = Some(callback);
        let state = Rc::downgrade(&self.state);
        self.inner.on_resize(Box::new(move |size, scale| {
            if let Some(state) = state.upgrade() {
                state.raw_size.set(size);
                state.raw_scale.set(scale);
                state.fire_resize();
            }
        }))
    }
    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_moved(callback)
    }
    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.inner.on_should_close(callback)
    }
    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.inner.on_hit_test_window_control(callback)
    }
    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.inner.on_close(callback)
    }
    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_appearance_changed(callback)
    }
    fn on_button_layout_changed(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_button_layout_changed(callback)
    }
    fn draw(&self, scene: &Scene) {
        self.sync_native_geometry();
        self.inner.draw(scene)
    }
    fn schedule_frame(&self) {
        self.inner.schedule_frame()
    }
    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.inner.sprite_atlas()
    }
    fn is_subpixel_rendering_supported(&self) -> bool {
        self.inner.is_subpixel_rendering_supported()
    }
    fn get_title(&self) -> String {
        self.inner.get_title()
    }
    fn tabbed_windows(&self) -> Option<Vec<SystemWindowTab>> {
        self.inner.tabbed_windows()
    }
    fn tab_bar_visible(&self) -> bool {
        self.inner.tab_bar_visible()
    }
    fn set_edited(&mut self, edited: bool) {
        self.inner.set_edited(edited)
    }
    fn set_document_path(&self, path: Option<&Path>) {
        self.inner.set_document_path(path)
    }
    fn toggle_simple_fullscreen(&self) {
        self.inner.toggle_simple_fullscreen()
    }
    fn is_simple_fullscreen(&self) -> bool {
        self.inner.is_simple_fullscreen()
    }
    #[cfg(target_os = "macos")]
    fn set_traffic_light_position(&self, position: Point<Pixels>) {
        self.state.traffic_light.set(Some(position));
        self.state.traffic_light_zoom.set(self.zoom());
        self.inner
            .set_traffic_light_position(traffic_light_out(position, self.zoom()))
    }
    fn show_character_palette(&self) {
        self.inner.show_character_palette()
    }
    fn titlebar_double_click(&self, is_resizable: bool, is_minimizable: bool) {
        self.inner
            .titlebar_double_click(is_resizable, is_minimizable)
    }
    fn on_move_tab_to_new_window(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_move_tab_to_new_window(callback)
    }
    fn on_merge_all_windows(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_merge_all_windows(callback)
    }
    fn on_select_previous_tab(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_select_previous_tab(callback)
    }
    fn on_select_next_tab(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_select_next_tab(callback)
    }
    fn on_toggle_tab_bar(&self, callback: Box<dyn FnMut()>) {
        self.inner.on_toggle_tab_bar(callback)
    }
    fn merge_all_windows(&self) {
        self.inner.merge_all_windows()
    }
    fn move_tab_to_new_window(&self) {
        self.inner.move_tab_to_new_window()
    }
    fn toggle_window_tab_overview(&self) {
        self.inner.toggle_window_tab_overview()
    }
    fn set_tabbing_identifier(&self, identifier: Option<String>) {
        self.inner.set_tabbing_identifier(identifier)
    }
    fn native_window_state(&self) -> Option<Vec<u8>> {
        self.inner.native_window_state()
    }
    fn restore_native_window_state(&self, state: &[u8]) {
        self.inner.restore_native_window_state(state)
    }
    #[cfg(target_os = "windows")]
    fn get_raw_handle(&self) -> windows::Win32::Foundation::HWND {
        self.inner.get_raw_handle()
    }
    fn inner_window_bounds(&self) -> WindowBounds {
        self.inner.inner_window_bounds()
    }
    fn request_decorations(&self, decorations: WindowDecorations) {
        self.inner.request_decorations(decorations)
    }
    fn show_window_menu(&self, position: Point<Pixels>) {
        self.inner
            .show_window_menu(point_out(position, self.zoom()))
    }
    fn start_window_move(&self) {
        self.inner.start_window_move()
    }
    fn can_start_external_drag(&self) -> bool {
        self.inner.can_start_external_drag()
    }
    fn start_external_drag(&self, payload: &ExternalDragPayload) -> bool {
        self.inner.start_external_drag(payload)
    }
    fn start_window_resize(&self, edge: ResizeEdge) {
        self.inner.start_window_resize(edge)
    }
    fn set_exclusive_zone(&self, zone: Pixels) {
        self.inner.set_exclusive_zone(out(zone, self.zoom()))
    }
    fn set_input_region(&self, region: Option<&[Bounds<Pixels>]>) {
        let zoom = self.zoom();
        let scaled: Option<Vec<Bounds<Pixels>>> =
            region.map(|r| r.iter().map(|b| bounds_out(*b, zoom)).collect());
        self.inner.set_input_region(scaled.as_deref())
    }
    fn window_decorations(&self) -> Decorations {
        self.inner.window_decorations()
    }
    fn set_app_id(&mut self, app_id: &str) {
        self.inner.set_app_id(app_id)
    }
    fn map_window(&mut self) -> AnyResult<()> {
        self.inner.map_window()
    }
    fn window_controls(&self) -> WindowControls {
        self.inner.window_controls()
    }
    fn set_client_inset(&self, inset: Pixels) {
        self.inner.set_client_inset(out(inset, self.zoom()))
    }
    fn gpu_specs(&self) -> Option<GpuSpecs> {
        self.inner.gpu_specs()
    }
    fn update_ime_position(&self, bounds: Bounds<Pixels>) {
        self.inner
            .update_ime_position(bounds_out(bounds, self.zoom()))
    }
    fn insets(&self) -> WindowInsets {
        let zoom = self.zoom();
        let insets = self.inner.insets();
        WindowInsets {
            safe_area: insets.safe_area.map(|v| inn(*v, zoom)),
            ime: insets.ime.map(|v| inn(*v, zoom)),
        }
    }
    fn on_insets_changed(&self, mut callback: Box<dyn FnMut(WindowInsets)>) {
        let state = Rc::downgrade(&self.state);
        self.inner.on_insets_changed(Box::new(move |insets| {
            let zoom = state.upgrade().map_or(1., |s| s.zoom.get());
            callback(WindowInsets {
                safe_area: insets.safe_area.map(|v| inn(*v, zoom)),
                ime: insets.ime.map(|v| inn(*v, zoom)),
            })
        }))
    }
    fn set_back_handler(&self, callback: Box<dyn FnMut()>) {
        self.inner.set_back_handler(callback)
    }
    fn set_back_enabled(&self, enabled: bool) {
        self.inner.set_back_enabled(enabled)
    }
    fn show_soft_keyboard(&self) {
        self.inner.show_soft_keyboard()
    }
    fn hide_soft_keyboard(&self) {
        self.inner.hide_soft_keyboard()
    }
    fn text_input_state_changed(&self, change: TextInputStateChange) {
        self.inner.text_input_state_changed(change)
    }
    fn play_system_bell(&self) {
        self.inner.play_system_bell()
    }
    fn a11y_init(&self, callbacks: A11yCallbacks) {
        self.inner.a11y_init(callbacks)
    }
    fn a11y_tree_update(&self, tree_update: gpui_kit::accesskit::TreeUpdate) {
        self.inner.a11y_tree_update(tree_update)
    }
    fn a11y_update_window_bounds(&self) {
        self.inner.a11y_update_window_bounds()
    }
    #[cfg(feature = "demo-capture")]
    fn render_to_image(&self, scene: &Scene) -> AnyResult<image::RgbaImage> {
        self.inner.render_to_image(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{MouseButton, MouseDownEvent, ScrollWheelEvent, point};

    #[test]
    fn zoom_for_percent_clamps_like_the_setting() {
        assert_eq!(zoom_for_percent(100), 1.);
        assert_eq!(zoom_for_percent(150), 1.5);
        assert_eq!(zoom_for_percent(0), 1.);
        assert_eq!(zoom_for_percent(999), 3.);
    }

    #[test]
    fn geometry_round_trips_through_the_zoom() {
        let b = Bounds::new(point(px(30.), px(60.)), Size::new(px(300.), px(90.)));
        assert_eq!(bounds_in(bounds_out(b, 1.5), 1.5), b);
        assert_eq!(
            size_in(Size::new(px(1500.), px(900.)), 1.5),
            Size::new(px(1000.), px(600.))
        );
        assert_eq!(
            point_out(point(px(10.), px(20.)), 2.),
            point(px(20.), px(40.))
        );
        // kit's title bar: lights at (9, 9) on a 34 px bar, centered at 17.
        // At 200% the bar is 68 points high and their center moves to 34.
        let lights = traffic_light_out(point(px(9.), px(9.)), 2.);
        assert_eq!(lights, point(px(18.), px(26.)));
        assert_eq!(f32::from(lights.y) + TRAFFIC_LIGHT_HALF, 34.);
        assert_eq!(
            traffic_light_out(point(px(9.), px(9.)), 1.),
            point(px(9.), px(9.))
        );
    }

    #[test]
    fn pointer_events_shrink_into_zoomed_space() {
        let down = PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: point(px(300.), px(150.)),
            ..Default::default()
        });
        let PlatformInput::MouseDown(e) = zoom_input(down, 1.5) else {
            panic!("event kind changed");
        };
        assert_eq!(e.position, point(px(200.), px(100.)));

        let scroll = PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: point(px(40.), px(40.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
            ..Default::default()
        });
        let PlatformInput::ScrollWheel(e) = zoom_input(scroll, 2.) else {
            panic!("event kind changed");
        };
        assert_eq!(e.position, point(px(20.), px(20.)));
        let ScrollDelta::Pixels(delta) = e.delta else {
            panic!("delta kind changed");
        };
        assert_eq!(delta, point(px(0.), px(-15.)));

        let lines = PlatformInput::ScrollWheel(ScrollWheelEvent {
            delta: ScrollDelta::Lines(point(0., 3.)),
            ..Default::default()
        });
        let PlatformInput::ScrollWheel(e) = zoom_input(lines, 2.) else {
            panic!("event kind changed");
        };
        assert!(matches!(e.delta, ScrollDelta::Lines(l) if l == point(0., 3.)));
    }

    #[test]
    fn file_drops_shrink_and_exits_pass_through() {
        let drop = PlatformInput::FileDrop(FileDropEvent::Submit {
            position: point(px(90.), px(30.)),
        });
        let PlatformInput::FileDrop(FileDropEvent::Submit { position }) = zoom_input(drop, 3.)
        else {
            panic!("event kind changed");
        };
        assert_eq!(position, point(px(30.), px(10.)));
        assert!(matches!(
            zoom_input(PlatformInput::FileDrop(FileDropEvent::Exited), 3.),
            PlatformInput::FileDrop(FileDropEvent::Exited)
        ));
    }

    #[test]
    fn zoom_change_relays_out_open_windows_and_skips_closed_ones() {
        set_initial_zoom(1.);
        let seen: Rc<RefCell<Vec<(Size<Pixels>, f32)>>> = Rc::default();
        let state = WindowZoom::register(1., Size::new(px(1200.), px(800.)), 2., None);
        let log = seen.clone();
        *state.resize.borrow_mut() = Some(Box::new(move |size, scale| {
            log.borrow_mut().push((size, scale));
        }));
        let closed = WindowZoom::register(1., Size::new(px(10.), px(10.)), 1., None);
        drop(closed);

        set_zoom(1.5);
        assert_eq!(current_zoom(), 1.5);
        assert_eq!(state.zoom.get(), 1.5);
        assert_eq!(
            seen.borrow().as_slice(),
            &[(Size::new(px(800.), px(800. / 1.5)), 3.)]
        );
        // The callback is back in place for the next change; an unchanged
        // zoom doesn't re-lay out.
        set_zoom(1.5);
        assert_eq!(seen.borrow().len(), 1);
        set_zoom(1.);
        assert_eq!(
            seen.borrow().last(),
            Some(&(Size::new(px(1200.), px(800.)), 2.))
        );
        assert!(WINDOWS.with(|l| l.borrow().iter().all(|w| w.strong_count() > 0)));
    }

    #[test]
    fn sanitize_rejects_nonsense() {
        assert_eq!(sanitize(f32::NAN), 1.);
        assert_eq!(sanitize(10.), 4.);
        assert_eq!(sanitize(1.25), 1.25);
    }

    /// Every method of GPUI's `Platform` and `PlatformWindow` traits must be
    /// forwarded, defaulted ones included (a vendored platform may override
    /// a default, like `frame_waker`). After a GPUI upgrade, this compares
    /// the trait source in the cargo registry with this file.
    #[test]
    fn forwards_every_trait_method() {
        let Some(source) = gpui_platform_source() else {
            eprintln!("gpui source not found in the cargo registry; skipping");
            return;
        };
        let this = include_str!("interface_zoom.rs");
        // Methods that only exist for GPUI's own tests, and the Wayland
        // layer-shell edge (behind a GPUI feature Quill doesn't enable).
        let skip = ["as_test", "set_exclusive_edge"];
        let mut missing = Vec::new();
        let names: Vec<String> = ["pub trait Platform: 'static", "pub trait PlatformWindow:"]
            .iter()
            .flat_map(|t| trait_methods(&source, t))
            .collect();
        // Both traits together have well over a hundred methods; far fewer
        // means the parser lost track of the trait bodies.
        assert!(names.len() > 120, "parsed only {} methods", names.len());
        for name in names {
            if skip.contains(&name.as_str()) {
                continue;
            }
            if !this.contains(&format!("fn {name}(")) {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "interface_zoom.rs doesn't forward: {missing:?}"
        );
    }

    fn trait_methods(source: &str, header: &str) -> Vec<String> {
        let Some(start) = source.find(header) else {
            panic!("trait `{header}` not found in gpui's platform.rs");
        };
        let mut depth = 0i32;
        let mut body = String::new();
        for ch in source[start..].chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            // Only direct members of the trait body.
            if depth == 1 {
                body.push(ch);
            } else if depth > 1 {
                body.push(' ');
            }
        }
        body.split("fn ")
            .skip(1)
            .filter_map(|rest| {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (!name.is_empty()).then_some(name)
            })
            .collect()
    }

    fn gpui_platform_source() -> Option<String> {
        let lock = include_str!("../../Cargo.lock");
        let version = lock
            .split("[[package]]")
            .find(|p| p.contains("name = \"gpui-pre\"\n"))?
            .lines()
            .find_map(|l| l.strip_prefix("version = \""))?
            .trim_end_matches('"')
            .to_string();
        let home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
        let registry = home.join("registry/src");
        std::fs::read_dir(registry)
            .ok()?
            .flatten()
            .find_map(|index| {
                std::fs::read_to_string(
                    index
                        .path()
                        .join(format!("gpui-pre-{version}/src/platform.rs")),
                )
                .ok()
            })
    }
}
