//! Screenshots for the UI driver (`quill::agent::ui_driver::shot`): a frame
//! of the main window that holds only what the agent may see, read back
//! by the window's own renderer and encoded as a PNG.
//!
//! What a screenshot shows is decided while the frame is painted, by the
//! rule that decides a click: GPUI hands the paint filter the tags of the
//! elements being painted (`a11y_tag`, the regions, parts, lists and
//! `chat=<id>` rows the tree is cut by) and `redact::point_allowed`
//! answers. Anything painted outside the allowed regions, and every row of
//! a chat without a grant, never reaches the frame; the neutral
//! [`HIDDEN_FILL`] covers it, and the allowed parts sit on the theme's
//! background where theirs was left out. The tree the reply carries is read from
//! that same frame, after the surface was checked in the same update, and
//! the takeover overlay is left out of it. A normal frame is drawn again
//! before anything is presented, so the user never sees the filtered one.
//!
//! The readback is in-process on every platform (Metal on macOS,
//! Direct3D 11 on Windows, wgpu on Linux): no screen-recording permission,
//! no other window in the picture, and nothing runs until an agent asks.
#![deny(clippy::indexing_slicing)]

use super::agent_state::{active_account, is_secret_chat_id};
use super::app::QuillApp;
use super::chat_window::ScopedApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::agent::activity::{self, Action};
use quill::agent::grants::{self, ChatClass};
use quill::agent::ui_driver::policy;
use quill::agent::ui_driver::redact;
use quill::agent::ui_driver::shot::{self, Shot, ShotOptions, ShotScope};
use quill::agent::ui_driver::surface::{self, DriverScope, Surface};
use quill::agent::ui_driver::takeover;
use quill::agent::ui_driver::tree::{UiNode, UiTree};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// What a screenshot shows where the agent may not look: a mid grey that
/// reads as "hidden" on light and dark themes alike.
pub(super) const HIDDEN_FILL: [u8; 3] = [0x8a, 0x8a, 0x8a];

/// [`HIDDEN_FILL`] as a colour to paint.
pub(super) fn hidden_fill() -> Hsla {
    let [r, g, b] = HIDDEN_FILL;
    rgb(u32::from_be_bytes([0, r, g, b])).into()
}

thread_local! {
    /// Set while the filtered frame is drawn: the takeover overlay is not
    /// part of what the agent sees.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

/// Whether the frame being drawn is a screenshot's.
pub(super) fn capturing() -> bool {
    CAPTURING.with(Cell::get)
}

impl QuillApp {
    /// Whether a chat's rows may show, by the rule `driver_may_open_chat`
    /// uses, as owned data the paint filter can keep: the grants once, the
    /// class of every chat the session knows.
    fn shot_grants(&self) -> impl Fn(i64) -> bool + 'static {
        let grants = grants::for_account(&active_account());
        let classes: HashMap<i64, ChatClass> = self
            .session()
            .map(|session| {
                session
                    .chats
                    .keys()
                    .map(|chat_id| (*chat_id, self.driver_chat_class(*chat_id)))
                    .collect()
            })
            .unwrap_or_default();
        move |chat_id| {
            let class = classes.get(&chat_id).copied().unwrap_or_else(|| {
                if is_secret_chat_id(chat_id) {
                    ChatClass::Secret
                } else {
                    ChatClass::Other
                }
            });
            grants.scope_for(chat_id, class).is_some()
        }
    }

    /// The paint filter of a screenshot: what is painted inside the tags
    /// GPUI hands it passes when a click there would. What it refuses is
    /// [`HIDDEN_FILL`]; a part it shows inside a refused one (a granted
    /// row in the chat list) sits on the theme's background, since the
    /// backgrounds around it were left out.
    fn shot_filter(&self, scope: DriverScope, background: Hsla) -> PaintFilter {
        let granted = self.shot_grants();
        PaintFilter {
            allows: Rc::new(move |tags: &[SharedString]| {
                let innermost_first: Vec<String> =
                    tags.iter().rev().map(|tag| tag.to_string()).collect();
                redact::point_allowed(&innermost_first, scope, &granted)
            }),
            hidden: hidden_fill(),
            shown: background,
        }
    }

    /// One line in the activity log per screenshot.
    fn log_screenshot(&self, agent: &str, surface: &Surface, options: ShotOptions) {
        let chat_id = surface.open_chat().unwrap_or(0);
        let chat_title = self
            .session()
            .and_then(|session| session.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        activity::record(Action {
            time: 0,
            account: active_account(),
            agent: agent.to_string(),
            tool: "ui_screenshot".to_string(),
            chat_id,
            chat_title,
            summary: if options.full_resolution {
                "screenshot (full resolution)".to_string()
            } else {
                "screenshot".to_string()
            },
            ok: true,
            declined: false,
            id: 0,
            message_ids: Vec::new(),
            undone: false,
        });
    }
}

/// A screenshot of the window as it is now, with the tree of the frame it
/// shows. `Err` carries the note the agent gets instead (a blocked
/// surface, a screen reader, a window that is gone, a renderer without
/// readback).
pub(super) async fn capture(
    app: &ScopedApp,
    cx: &mut AsyncWindowContext,
    agent: &str,
    options: ShotOptions,
) -> Result<(UiTree, Shot), String> {
    let (rgba, tree, scope) = capture_frame(app, cx, agent, options)?;
    let width = tree.width;
    let shot = cx
        .background_executor()
        .spawn(async move { encode(rgba, options, width, scope) })
        .await?;
    Ok((tree, shot))
}

/// The synchronous part: check, draw the filtered frame, read the tree and
/// the pixels back, draw a normal frame again. Nothing else runs on the UI
/// thread in between, so the frame, the tree and the surface agree.
fn capture_frame(
    app: &ScopedApp,
    cx: &mut AsyncWindowContext,
    agent: &str,
    options: ShotOptions,
) -> Result<(image::RgbaImage, UiTree, ShotScope), String> {
    let (before, scope, filter) = app
        .update_in(cx, |app, window, cx| {
            if window.is_a11y_system_active() {
                return Err(surface::screen_reader_active());
            }
            // Paused or stopped, the window is the user's.
            if let Some(note) = takeover::refusal() {
                return Err(note);
            }
            if cx.has_active_drag() {
                return Err(shot::dragging());
            }
            let surface = app.driver_surface();
            if let Some(note) = surface.read_refusal() {
                return Err(note);
            }
            window.set_a11y_forced(true);
            let scope = app.driver_scope();
            Ok((
                surface,
                scope,
                app.shot_filter(scope, cx.theme().background),
            ))
        })
        .map_err(|_| policy::window_closed())??;
    let read = cx
        .update(|window, cx| {
            CAPTURING.with(|on| on.set(true));
            window.set_paint_filter(Some(filter));
            window.draw(cx).clear(cx);
            let read = read_frame(window);
            window.set_paint_filter(None);
            CAPTURING.with(|on| on.set(false));
            read
        })
        .map_err(|_| policy::window_closed());
    let pixels_read = matches!(read, Ok(Ok(_)));
    let tree = app
        .update_in(cx, |app, window, _| {
            let (now, tree) = app.agent_tree(window)?;
            if !now.same_as(&before) {
                return Err(surface::changed_refusal(&before, &now));
            }
            let tree =
                tree.ok_or_else(|| policy::no_frame(super::agent_driver::window_state(window)))?;
            if pixels_read {
                app.log_screenshot(agent, &now, options);
            }
            Ok(tree)
        })
        .map_err(|_| policy::window_closed());
    // The filtered frame must never be presented: draw a full one now.
    let _ = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let rgba = read??;
    let tree = tree??;
    let scope = match scope {
        DriverScope::Granted => ShotScope::Granted,
        DriverScope::Everything => ShotScope::AllChats,
    };
    Ok((rgba, tree, scope))
}

/// The filtered frame's pixels, from the window's renderer.
#[cfg(not(test))]
fn read_frame(window: &Window) -> Result<image::RgbaImage, String> {
    window
        .capture_frame()
        .map_err(|error| capture_failed(&error.to_string()))
}

/// Under the test platform there is no GPU: the frame's quads are painted
/// on the CPU instead ([`raster`]).
#[cfg(test)]
fn read_frame(window: &Window) -> Result<image::RgbaImage, String> {
    Ok(raster::raster_quads(window))
}

fn capture_failed(why: &str) -> String {
    format!(
        "Quill could not capture its window ({why}). Use read_ui_tree; screenshots may not work on this system."
    )
}

/// The readback shrunk unless `full_resolution`, flattened onto
/// [`HIDDEN_FILL`], as an RGB PNG. `window_width` is the window's width in
/// logical pixels, for the scale. At most one smaller copy of the pixels
/// is made: the shrink happens first and the flatten works in place.
pub(super) fn encode(
    rgba: image::RgbaImage,
    options: ShotOptions,
    window_width: f32,
    scope: ShotScope,
) -> Result<Shot, String> {
    let (width, height) = shot::output_size(rgba.width(), rgba.height(), options.full_resolution);
    // The readback is premultiplied, which is what `resize` filters, so
    // shrinking before the flatten gives the same picture. The full-size
    // readback is dropped at the end of this block.
    let rgba = {
        let full = rgba;
        if (width, height) == full.dimensions() {
            full
        } else {
            image::imageops::resize(&full, width, height, image::imageops::FilterType::Triangle)
        }
    };
    let rgb = flatten(rgba);
    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    image::ImageEncoder::write_image(encoder, &rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| capture_failed(&error.to_string()))?;
    let scale = if window_width > 0.0 {
        width as f32 / window_width
    } else {
        1.0
    };
    Ok(Shot {
        png,
        width,
        height,
        scale,
        scope,
    })
}

/// Every pixel over [`HIDDEN_FILL`], as RGB rows, in the readback's own
/// buffer: what the filter kept out of the frame was never drawn, so it is
/// transparent in the readback. Every renderer's readback is premultiplied
/// (Metal and Direct3D blend straight alpha onto a transparent target,
/// which premultiplies; wgpu renders the capture in the window's own alpha
/// mode, and either mode leaves it premultiplied), so a pixel's colour
/// already carries its alpha.
fn flatten(rgba: image::RgbaImage) -> Vec<u8> {
    let mut raw = rgba.into_raw();
    let pixels = raw.len() / 4;
    let [fr, fg, fb] = HIDDEN_FILL;
    // Pixel `index` is read from `4 * index` and written to `3 * index`:
    // the write never reaches a pixel not yet read.
    for index in 0..pixels {
        let from = index * 4;
        let Some(&[r, g, b, a]) = raw
            .get(from..from + 4)
            .and_then(|pixel| <&[u8; 4]>::try_from(pixel).ok())
        else {
            break;
        };
        let rest = 255 - u16::from(a);
        let over = |channel: u8, fill: u8| {
            let value = u16::from(channel) + (u16::from(fill) * rest + 127) / 255;
            u8::try_from(value.min(255)).unwrap_or(u8::MAX)
        };
        let to = index * 3;
        if let Some(target) = raw.get_mut(to..to + 3) {
            target.copy_from_slice(&[over(r, fr), over(g, fg), over(b, fb)]);
        }
    }
    raw.truncate(pixels * 3);
    raw
}

/// The tree an input command answers with, and the screenshot asked for:
/// with one, the tree is the screenshot's and `added` is counted again
/// from `before` (the tree before the input).
pub(super) async fn after_input(
    app: &ScopedApp,
    cx: &mut AsyncWindowContext,
    agent: &str,
    options: Option<ShotOptions>,
    before: &UiTree,
    tree: UiTree,
    added: Vec<UiNode>,
) -> (UiTree, Vec<UiNode>, Option<Result<Shot, String>>) {
    let Some(options) = options else {
        return (tree, added, None);
    };
    match capture(app, cx, agent, options).await {
        Ok((shot_tree, shot)) => {
            let added = shot_tree.new_since(before).into_iter().cloned().collect();
            (shot_tree, added, Some(Ok(shot)))
        }
        Err(note) => (tree, added, Some(Err(note))),
    }
}

/// The test platform's stand-in for a GPU readback: the frame's solid
/// quads painted on the CPU (no text, images or borders), and the quads
/// kept for assertions (`agent_driver_shot_tests`).
#[cfg(test)]
pub(super) mod raster {
    use gpui_kit::Window;

    #[cfg(feature = "demo-capture")]
    thread_local! {
        static CAPTURED: std::cell::RefCell<Vec<gpui_kit::Quad>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    /// The quads of the last frame [`raster_quads`] read.
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn captured() -> Vec<gpui_kit::Quad> {
        CAPTURED.with(|quads| quads.borrow().clone())
    }

    pub(super) fn raster_quads(window: &Window) -> image::RgbaImage {
        let scale = window.scale_factor();
        let size = window.viewport_size();
        let side = |logical: f32| (logical * scale).round().max(1.0) as u32;
        #[allow(unused_mut)]
        let mut image =
            image::RgbaImage::new(side(size.width.as_f32()), side(size.height.as_f32()));
        #[cfg(feature = "demo-capture")]
        {
            let quads = window.painted_quads();
            for quad in &quads {
                paint(&mut image, quad);
            }
            CAPTURED.with(|captured| *captured.borrow_mut() = quads);
        }
        image
    }

    /// A solid quad over `image`, clipped to its content mask, blended
    /// premultiplied as the renderers do.
    #[cfg(feature = "demo-capture")]
    fn paint(image: &mut image::RgbaImage, quad: &gpui_kit::Quad) {
        let Some(color) = quad.background.as_solid() else {
            return;
        };
        let color = gpui_kit::Rgba::from(color);
        let clip = quad.bounds.intersect(&quad.content_mask.bounds);
        let edge = |at: f32, limit: u32| (at.max(0.0).round() as u32).min(limit);
        let (x0, y0) = (
            edge(clip.origin.x.0, image.width()),
            edge(clip.origin.y.0, image.height()),
        );
        let x1 = edge(clip.origin.x.0 + clip.size.width.0, image.width());
        let y1 = edge(clip.origin.y.0 + clip.size.height.0, image.height());
        let alpha = color.a.clamp(0.0, 1.0);
        let source = [color.r, color.g, color.b].map(|c| c.clamp(0.0, 1.0) * alpha * 255.0);
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(pixel) = image.get_pixel_mut_checked(x, y) {
                    let [r, g, b, a] = pixel.0;
                    let under = |channel: u8, top: f32| {
                        (top + f32::from(channel) * (1.0 - alpha))
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                    let [sr, sg, sb] = source;
                    pixel.0 = [
                        under(r, sr),
                        under(g, sg),
                        under(b, sb),
                        under(a, alpha * 255.0),
                    ];
                }
            }
        }
    }
}
