//! Screenshot demo: the glass lab. A full-window backdrop with glass panels
//! at the rects of the owner's `NSGlassEffectView` reference app (window
//! 1200x800 points; AppKit's y=300 from the bottom is 270 from the top), so a
//! capture or a live run lines up with Apple's glass spot for spot. Every
//! panel drags with the mouse.
//!
//! `QUILL_DEMO_GLASS_BACKDROP` lists the backdrops, `:`-separated PNG files
//! or directories of them (taken in name order), each stretched to the
//! window; a grey ramp, black to white under the panels' span, always comes
//! last. "b", space or the right arrow shows the next backdrop, the left
//! arrow the one before. "f" (macOS) starts the reference app's flip test: a
//! ScreenCaptureKit capture of the window laid over the live lab every other
//! half second, so a faithful capture shows no change; any other key stops
//! it. `QUILL_DEMO_GLASS_SPOT=x,y` moves the panels the way the
//! reference app's `--capture` spots do: panel k to AppKit (x + 280k, y), y
//! from the bottom. `QUILL_DEMO_GLASS_SCREEN=<png>` (macOS) also saves what
//! the screen shows of the window, through ScreenCaptureKit like the
//! reference app's captures, beside the drawable's own capture.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::glass::{
    GlassProvider, GlassStyle, GlassSurface, GlassVariant, glass_style,
};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

register_demos![
    DemoSpec::chat_list("glass-lab")
        .setup(open_glass_lab)
        .timing(2500, 3500),
];

const LABELS: [&str; 7] = [
    "Mute notifications",
    "View profile",
    "Set Wallpaper",
    "Archive chat",
    "Add to folder",
    "Auto-Delete",
    "Send a gift",
];

/// The window's root becomes the lab once the app has built its first frame.
/// Without an app glass provider, the lab shows the kit's Regular glass.
fn open_glass_lab(_app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
    if !cx.has_global::<GlassProvider>() {
        cx.set_global(GlassProvider(kit_regular));
    }
    window.defer(cx, |window, cx| {
        window.replace_root(cx, |window, cx| {
            let lab = cx.new(|cx| GlassLab::new(window, cx));
            gpui_kit::component::Root::new(lab, window, cx)
        });
    });
}

fn kit_regular(_variant: GlassVariant, cx: &App) -> Option<GlassStyle> {
    let theme = cx.theme();
    Some(GlassStyle::regular(
        theme.is_dark(),
        theme.popover_foreground,
        theme.muted_foreground,
        false,
    ))
}

enum PanelBody {
    /// The reference app's label column: a title, then the seven items.
    Labels(&'static str),
    /// A real kit `PopupMenu`, which draws its own glass.
    Menu(Entity<PopupMenu>),
}

struct Panel {
    origin: Point<Pixels>,
    size: Size<Pixels>,
    body: PanelBody,
}

struct Drag {
    panel: usize,
    grab: Point<Pixels>,
}

enum Backdrop {
    Image(std::path::PathBuf),
    Ramp,
}

struct GlassLab {
    panels: Vec<Panel>,
    drag: Option<Drag>,
    backdrops: Vec<Backdrop>,
    shown: usize,
    focus: FocusHandle,
    #[cfg(target_os = "macos")]
    flip: Option<Flip>,
}

/// The flip test under way: the capture once it lands, and whether it shows.
#[cfg(target_os = "macos")]
struct Flip {
    shot: Option<std::sync::Arc<RenderImage>>,
    showing: bool,
    _run: Task<()>,
}

/// The backdrops `QUILL_DEMO_GLASS_BACKDROP` names, then the ramp.
fn backdrops() -> Vec<Backdrop> {
    let mut backdrops = Vec::new();
    let listed = std::env::var_os("QUILL_DEMO_GLASS_BACKDROP").unwrap_or_default();
    for path in std::env::split_paths(&listed) {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut images: Vec<std::path::PathBuf> = entries
                    .filter_map(|entry| Some(entry.ok()?.path()))
                    .filter(|file| file.extension().is_some_and(|ext| ext == "png"))
                    .collect();
                images.sort();
                backdrops.extend(images.into_iter().map(Backdrop::Image));
            }
            Err(_) if path.is_file() => backdrops.push(Backdrop::Image(path)),
            Err(_) => {}
        }
    }
    backdrops.push(Backdrop::Ramp);
    backdrops
}

impl GlassLab {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            LABELS.iter().fold(menu.min_w(px(212.)), |menu, label| {
                menu.menu(*label, Box::new(NoAction))
            })
        });
        let (x, y) = std::env::var("QUILL_DEMO_GLASS_SPOT")
            .ok()
            .and_then(|spot| {
                let (x, y) = spot.split_once(',')?;
                Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
            })
            .unwrap_or((60., 300.));
        let panel = |k: f32, width: f32, body| Panel {
            origin: point(px(x + 280. * k), px(800. - y - 230.)),
            size: size(px(width), px(230.)),
            body,
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        #[cfg(target_os = "macos")]
        if let Some(path) = std::env::var_os("QUILL_DEMO_GLASS_SCREEN") {
            screen_copy::save_later(window, cx, path.into());
        }
        Self {
            panels: vec![
                panel(0., 240., PanelBody::Labels("REGULAR")),
                panel(1., 240., PanelBody::Labels("REGULAR")),
                panel(2., 220., PanelBody::Menu(menu)),
            ],
            drag: None,
            backdrops: backdrops(),
            shown: 0,
            focus,
            #[cfg(target_os = "macos")]
            flip: None,
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        #[cfg(target_os = "macos")]
        {
            self.flip = None;
            if key == "f" {
                self.start_flip(window, cx);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
        let count = self.backdrops.len().max(1);
        match key {
            "b" | "space" | "right" => self.shown = (self.shown + 1) % count,
            "left" => self.shown = (self.shown + count - 1) % count,
            _ => {}
        }
        cx.notify();
    }

    /// Captures the window once the frame without an earlier overlay is on
    /// screen, then flips the capture on and off every half second.
    #[cfg(target_os = "macos")]
    fn start_flip(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(target) = screen_copy::target(window) else {
            return;
        };
        let run = cx.spawn(async move |lab, cx| {
            let executor = cx.background_executor().clone();
            executor.timer(std::time::Duration::from_millis(150)).await;
            let shot = executor
                .spawn(async move { screen_copy::capture(target) })
                .await;
            let Some(shot) = shot.map(screen_copy::render_image) else {
                eprintln!("glass lab: flip capture failed");
                return;
            };
            let mut shot = Some(shot);
            loop {
                let flipped = lab.update(cx, |lab, cx| {
                    if let Some(flip) = lab.flip.as_mut() {
                        match shot.take() {
                            Some(shot) => flip.shot = Some(shot),
                            None => flip.showing = !flip.showing,
                        }
                        cx.notify();
                    }
                });
                if flipped.is_err() {
                    return;
                }
                executor.timer(std::time::Duration::from_millis(500)).await;
            }
        });
        self.flip = Some(Flip {
            shot: None,
            showing: false,
            _run: run,
        });
    }

    /// The flip test's capture and its label, laid over everything.
    #[cfg(target_os = "macos")]
    fn flip_overlay(&self) -> Option<AnyElement> {
        let flip = self.flip.as_ref()?;
        let shot = flip.shot.clone()?;
        let label = if flip.showing {
            "CAPTURE (WINDOW)"
        } else {
            "LIVE"
        };
        Some(
            div()
                .absolute()
                .size_full()
                .when(flip.showing, |this| {
                    this.child(
                        img(ImageSource::Render(shot))
                            .absolute()
                            .size_full()
                            .object_fit(ObjectFit::Fill),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left(px(6.))
                        .bottom(px(6.))
                        .w(px(260.))
                        .h(px(20.))
                        .bg(black())
                        .text_color(white())
                        .text_size(px(14.))
                        .font_weight(FontWeight::BOLD)
                        .child(label),
                )
                .into_any_element(),
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn flip_overlay(&self) -> Option<AnyElement> {
        None
    }

    fn backdrop(&self) -> AnyElement {
        match self.backdrops.get(self.shown).unwrap_or(&Backdrop::Ramp) {
            Backdrop::Image(path) => img(path.clone())
                .absolute()
                .size_full()
                .object_fit(ObjectFit::Fill)
                .into_any_element(),
            Backdrop::Ramp => {
                const STEPS: usize = 16;
                let band = 780. / STEPS as f32;
                div()
                    .absolute()
                    .size_full()
                    .children((0..STEPS).map(|step| {
                        let grey = step as f32 / (STEPS - 1) as f32;
                        div()
                            .absolute()
                            .left(px(60. + band * step as f32))
                            .top_0()
                            .w(px(band))
                            .h_full()
                            .bg(hsla(0., 0., grey, 1.))
                    }))
                    .into_any_element()
            }
        }
    }

    fn panel(&self, index: usize, panel: &Panel, cx: &mut Context<Self>) -> AnyElement {
        let body = match &panel.body {
            PanelBody::Menu(menu) => div().child(menu.clone()).into_any_element(),
            PanelBody::Labels(title) => {
                let foreground = cx.theme().popover_foreground;
                let column = div()
                    .size_full()
                    .text_size(px(13.))
                    .text_color(foreground)
                    .children(std::iter::once(*title).chain(LABELS).enumerate().map(
                        |(row, label)| {
                            // Where AppKit's 13pt label draws the same
                            // glyphs: 2pt of line-fragment padding in, its
                            // 18pt frame's text sitting high.
                            div()
                                .absolute()
                                .left(px(16.))
                                .top(px(9.5 + 26. * row as f32))
                                .child(label)
                        },
                    ));
                match glass_style(GlassVariant::Regular, cx) {
                    Some(style) => GlassSurface::new(style, column).into_any_element(),
                    None => column
                        .bg(cx.theme().popover)
                        .rounded(px(16.))
                        .into_any_element(),
                }
            }
        };
        let fixed = matches!(panel.body, PanelBody::Labels(_));
        div()
            .absolute()
            .left(panel.origin.x)
            .top(panel.origin.y)
            .when(fixed, |this| this.w(panel.size.width).h(panel.size.height))
            .when(!fixed, |this| this.w(panel.size.width))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |lab, event: &MouseDownEvent, _, _| {
                    if let Some(panel) = lab.panels.get(index) {
                        lab.drag = Some(Drag {
                            panel: index,
                            grab: event.position - panel.origin,
                        });
                    }
                }),
            )
            .child(body)
            .into_any_element()
    }
}

impl Render for GlassLab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panels: Vec<AnyElement> = self
            .panels
            .iter()
            .enumerate()
            .map(|(index, panel)| self.panel(index, panel, cx))
            .collect();
        div()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|lab, event: &KeyDownEvent, window, cx| lab.on_key(event, window, cx)),
            )
            .on_mouse_move(cx.listener(|lab, event: &MouseMoveEvent, _, cx| {
                let Some(drag) = &lab.drag else {
                    return;
                };
                if !event.dragging() {
                    lab.drag = None;
                    return;
                }
                let origin = event.position - drag.grab;
                if let Some(panel) = lab.panels.get_mut(drag.panel) {
                    panel.origin = origin;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|lab, _: &MouseUpEvent, _, _| lab.drag = None),
            )
            .child(self.backdrop())
            .children(panels)
            .children(self.flip_overlay())
    }
}

/// What the screen shows of the lab's window, taken by ScreenCaptureKit (no
/// permission needed for the app's own window) the way the reference app's
/// `capture()` and flip test take it: the whole window at its backing scale,
/// cropped to the content below the title bar.
#[cfg(target_os = "macos")]
mod screen_copy {
    use super::GlassLab;
    use gpui_kit::{AppContext, Context, RenderImage, Window};
    use objc2_app_kit::{NSView, NSWindow};
    use quill::calls::capture::screen::{ScreenBackend, ScreenSource, SourceId, SourceKind};
    use quill::calls::capture::screen_mac::MacScreen;
    use smallvec::SmallVec;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    /// The window to capture: its number and its content's device size.
    #[derive(Clone, Copy)]
    pub(super) struct Target {
        number: u32,
        content: (u32, u32),
    }

    pub(super) fn target(window: &Window) -> Option<Target> {
        let scale = window.scale_factor();
        let size = window.viewport_size();
        Some(Target {
            number: window_number(window)?,
            content: (
                (f32::from(size.width) * scale).round() as u32,
                (f32::from(size.height) * scale).round() as u32,
            ),
        })
    }

    /// Blocks on ScreenCaptureKit; run it off the main thread.
    pub(super) fn capture(target: Target) -> Option<image::RgbaImage> {
        let source = ScreenSource {
            id: SourceId(format!("window:{}", target.number)),
            kind: SourceKind::Window,
            title: String::new(),
            size: (0, 0),
        };
        let shot = MacScreen.thumbnail(&source, (16384, 16384))?;
        let mut whole = image::RgbaImage::from_raw(shot.width, shot.height, shot.rgba)?;
        let (width, height) = target.content;
        let titlebar = whole.height().checked_sub(height)?;
        let width = width.min(whole.width());
        Some(image::imageops::crop(&mut whole, 0, titlebar, width, height).to_image())
    }

    /// The capture as GPUI draws it (BGRA).
    pub(super) fn render_image(mut shot: image::RgbaImage) -> Arc<RenderImage> {
        for pixel in shot.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
            shot,
        )])))
    }

    pub(super) fn save_later(window: &Window, cx: &mut Context<GlassLab>, path: PathBuf) {
        let Some(target) = target(window) else {
            return;
        };
        let executor = cx.background_executor().clone();
        cx.background_spawn(async move {
            executor.timer(Duration::from_millis(1800)).await;
            match capture(target).map(|shot| shot.save(&path)) {
                Some(Ok(())) => eprintln!("glass lab: screen copy saved to {}", path.display()),
                _ => eprintln!("glass lab: screen copy failed"),
            }
        })
        .detach();
    }

    fn window_number(window: &Window) -> Option<u32> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: GPUI's AppKit handle points at the window's live NSView.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let window: objc2::rc::Retained<NSWindow> = view.window()?;
        u32::try_from(window.windowNumber()).ok()
    }
}
