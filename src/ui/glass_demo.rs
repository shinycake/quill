//! Screenshot demo for backdrop glass (`Window::paint_backdrop_lens`, the
//! vendored GPUI's `scene/backdrop_lens.rs`): `ready-glass` opens the
//! README showcase conversation and lays glass over it, live: a clear and
//! a tinted button over the chat rows, a tab bar over the photo whose
//! selection lens sits between two tabs (glass over glass), a frosted card
//! over the photo's edge, and two loupes over a message: one enlarging the
//! frame's pixels, one drawing the text under it again three times finer
//! (`LensDetail::Rendered`, Metal; elsewhere it copies). The gooey shapes
//! of stage 5 are here too: a select whose list hangs off its button by a
//! neck (`LensBlob`) and an action button whose arrow is a droplet melting
//! off the body (`LensDroplet`), its labels inked from the glass under
//! each glyph (`Window::with_lens_ink`). No product UI uses the glass yet.
//!
//! `QUILL_DEMO_GLASS=off` paints the same labels without the glass, to see
//! (and time) the frame under it. The looks are the presets of
//! quill-tools/gpui-liquid-glass (a port of bilalgursen/liquid-glass, MIT).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::showcase_demo as sc;
use gpui_kit::component::Root;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::{
    AnyView, App, AppContext, Bounds, Context, Font, FontWeight, Hsla, InteractiveElement,
    IntoElement, LensBend, LensBlob, LensBorder, LensDetail, LensDroplet, LensInk, LensMaterial,
    LensNeck, LensStyle, LensTune, ParentElement, Pixels, Render, Rgba, SharedString, Styled,
    TextAlign, TextRun, Window, canvas, div, point, px, size,
};
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-glass",
        "screenshot demo — backdrop glass over the showcase chat"
    )
    .setup(QuillApp::demo_ready_glass)
    .timing(2500, 3500)
];

impl QuillApp {
    fn demo_ready_glass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            sc::apply_ready_showcase(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                sc::Scene::Hikers,
            );
        }
        let glass = std::env::var("QUILL_DEMO_GLASS").as_deref() != Ok("off");
        window.defer(cx, move |window, cx| wrap_root(glass, window, cx));
    }
}

/// Puts the overlay between the window's kit `Root` and the shell, so the
/// glass paints after the whole app and the capture still finds a `Root`.
fn wrap_root(glass: bool, window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<Root>() else {
        return;
    };
    let inner = root.read(cx).view().clone();
    let overlay = cx.new(|_| GlassOverlay { inner, glass });
    window.replace_root(cx, |window, cx| Root::new(overlay, window, cx));
}

struct GlassOverlay {
    inner: AnyView,
    glass: bool,
}

struct Palette {
    text: Hsla,
    muted: Hsla,
    highlight: Hsla,
    surface: Hsla,
    card: Hsla,
    card_line: f32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                text: rgb(0.957, 0.961, 0.969, 1.0),
                muted: rgb(0.957, 0.961, 0.969, 0.63),
                highlight: rgb(0.17, 0.175, 0.2, 1.0),
                surface: rgb(0.06, 0.065, 0.08, 1.0),
                card: rgb(0.0, 0.0, 0.0, 1.0),
                card_line: 0.25,
            }
        } else {
            Self {
                text: rgb(0.078, 0.086, 0.102, 1.0),
                muted: rgb(0.078, 0.086, 0.102, 0.66),
                highlight: rgb(0.97, 0.975, 0.985, 1.0),
                surface: rgb(0.93, 0.94, 0.96, 1.0),
                card: rgb(1.0, 1.0, 1.0, 1.0),
                card_line: 0.6,
            }
        }
    }
}

fn rgb(r: f32, g: f32, b: f32, a: f32) -> Hsla {
    Rgba { r, g, b, a }.into()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The kit's clear button; `press` 0..1 deepens the bevel and shadow.
fn clear_button(press: f32, frost: f32, pal: &Palette) -> LensStyle {
    let pc = press.clamp(0.0, 1.0);
    LensStyle {
        unit: 0.0,
        bevel: lerp(gpui_kit::LENS_DESIGN_HEIGHT * 0.42, 62.0, pc),
        band: None,
        shadow: 0.12 * lerp(0.55, 1.0, pc),
        shadow_softness: 34.0 * lerp(0.6, 1.0, pc),
        shadow_offset: 22.0 * lerp(0.5, 1.0, pc),
        strength: 1.0,
        color: pal.highlight,
        surface: pal.surface,
        material: LensMaterial::Clear {
            bend: LensBend::Edge {
                refract: lerp(44.0, 37.0, pc),
                magnify: 0.0,
                dispersion: 0.12,
                blur_center: 0.35,
                blur_edge: 2.2,
            },
            frost,
            blur: px(0.0),
            blur_mix: 1.0,
        },
        border: None,
        tune: LensTune::default(),
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    }
}

/// Tinted glass of its own colour, thinning towards the rim.
fn tinted(color: Hsla, pal: &Palette) -> LensStyle {
    LensStyle {
        color,
        material: LensMaterial::Tinted { edge_clear: 0.2 },
        ..clear_button(0.0, 0.0, pal)
    }
}

/// The tabs' selection lens while it travels (`open` 0..1).
fn selection(open: f32, height: f32, pal: &Palette) -> LensStyle {
    const OPTION_UNITS: f32 = 80.0;
    const OPTICS: f32 = OPTION_UNITS * 1.35 / 120.0;
    let pc = open.clamp(0.0, 1.0);
    let frost = 1.0 - pc;
    LensStyle {
        unit: height / OPTION_UNITS,
        bevel: lerp(OPTION_UNITS * 0.42, 62.0 * OPTICS, pc),
        shadow_softness: 34.0 * OPTICS * lerp(0.6, 1.0, pc),
        shadow_offset: 22.0 * OPTICS * lerp(0.5, 1.0, pc),
        strength: pc,
        material: LensMaterial::Clear {
            bend: LensBend::Edge {
                refract: lerp(6.0, 37.0, pc) * OPTICS * lerp(0.06, 1.0, pc),
                magnify: 0.12 * pc,
                dispersion: 0.12 * pc,
                blur_center: lerp(0.35, 4.0, frost),
                blur_edge: lerp(2.2, 4.0, frost),
            },
            frost,
            blur: px(0.0),
            blur_mix: 1.0,
        },
        ..clear_button(open, 0.0, pal)
    }
}

/// The kit's frosted card: a radial pull of `bend` px at the rim, a 6 px
/// frost, a cover of the card colour and a thin line.
fn card(bend: f32, pal: &Palette) -> LensStyle {
    LensStyle {
        shadow: 0.0,
        shadow_softness: 34.0,
        shadow_offset: 22.0,
        strength: 1e-4,
        color: pal.card,
        material: LensMaterial::Clear {
            bend: LensBend::Radial { amount: px(bend) },
            frost: 0.55,
            blur: px(6.0),
            blur_mix: 1.0,
        },
        border: Some(LensBorder {
            color: rgb(1.0, 1.0, 1.0, pal.card_line),
            width: px(1.3),
        }),
        tune: LensTune {
            bend_depth: 1.35,
            bend_curve: 9.75,
            ..LensTune::default()
        },
        ..clear_button(0.0, 0.0, pal)
    }
}

/// A clear lens that about doubles its middle, lit from the lower right;
/// `detail` says whether it enlarges the frame or draws it again finer.
fn magnifier(detail: LensDetail, pal: &Palette) -> LensStyle {
    let mut style = clear_button(0.0, 0.0, pal);
    if let LensMaterial::Clear {
        bend: LensBend::Edge { magnify, .. },
        ..
    } = &mut style.material
    {
        *magnify = 1.0;
    }
    style.light = point(0.6, 0.8);
    style.light_mix = 0.5;
    style.detail = detail;
    style
}

/// Gap between a separated droplet and the body, as a share of the height
/// (the action button of quill-tools/gpui-liquid-glass).
const DROPLET_GAP: f32 = 0.25;

fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The action button's glass in a `w` x `h` box: the body, and its right
/// end a droplet `apart` 0..1 of the way off it. Returns the body (in the
/// box) and the style; the droplet's centre is `h / 2` from the box's
/// right edge.
fn action_glass(w: f32, h: f32, apart: f32, pal: &Palette) -> (Bounds<Pixels>, LensStyle) {
    let t = apart.clamp(0.0, 1.06);
    let pull = h * (1.0 + DROPLET_GAP) * t;
    let body = Bounds::new(point(px(0.0), px(0.0)), size(px(w - pull), px(h)));
    let mut style = clear_button(0.0, 0.25, pal);
    style.droplet = (t > 0.001).then(|| LensDroplet {
        offset_x: px(w / 2.0 - h / 2.0 + pull / 2.0),
        radius: px(h / 2.0),
        softness: px(h * 0.34 * smooth(0.0, 0.15, t) * (1.0 - smooth(0.8, 1.0, t))),
    });
    (body, style)
}

/// The select's frosted button with its list swelling out below on a neck,
/// `drop` below the button's centre, `list` its size.
fn select_glass(button_w: f32, drop: f32, list: (f32, f32), pal: &Palette) -> LensStyle {
    let mut style = card(0.0, pal);
    style.shadow = 0.1;
    style.strength = 1.0;
    style.blob = Some(LensBlob {
        offset: point(px(0.0), px(drop)),
        half_size: size(px(list.0 / 2.0), px(list.1 / 2.0)),
        corner_radius: px(22.0),
        softness: px(24.0),
        neck: Some(LensNeck {
            x: px(0.0),
            width: px(button_w * 0.3),
        }),
    });
    style
}

/// Text centred in its box; over glass with `ink`, each glyph takes the
/// ink that reads on the glass under it.
fn label(
    text: &'static str,
    font_size: f32,
    ink: Option<LensInk>,
    color: Hsla,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let font = Font {
                weight: FontWeight::SEMIBOLD,
                ..window.text_style().font()
            };
            let run = TextRun {
                len: text.len(),
                font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                SharedString::new_static(text),
                px(font_size),
                &[run],
                None,
            );
            let line_height = px(font_size * 1.3);
            let origin = point(
                bounds.center().x - line.width / 2.0,
                bounds.center().y - line_height / 2.0,
            );
            let mut paint = |window: &mut Window| {
                line.paint(origin, line_height, TextAlign::Left, None, window, cx)
            };
            let painted = match ink {
                Some(ink) => window.with_lens_ink(ink, paint),
                None => paint(window),
            };
            if let Err(error) = painted {
                log::debug!("glass demo label: {error}");
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

const TABS: [&str; 4] = ["Chats", "Calls", "Stories", "Settings"];
const TAB_W: f32 = 84.0;
const TAB_H: f32 = 48.0;

impl GlassOverlay {
    /// Glass filling its parent, or nothing when the demo is run without.
    fn lens(&self, radius: f32, style: LensStyle) -> impl IntoElement {
        let glass = self.glass;
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                if glass {
                    window.paint_backdrop_lens(bounds, px(radius), &style);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// Glass on `body`, a box inside its parent.
    fn lens_on(&self, body: Bounds<Pixels>, radius: f32, style: LensStyle) -> impl IntoElement {
        let glass = self.glass;
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                if glass {
                    let body = Bounds::new(bounds.origin + body.origin, body.size);
                    window.paint_backdrop_lens(body, px(radius), &style);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// The ink of labels on clear glass, while there is glass.
    fn ink(&self) -> Option<LensInk> {
        self.glass.then(|| LensInk {
            on_light: Palette::new(false).text,
            on_dark: Palette::new(true).text,
        })
    }

    /// The action button mid-melt: its arrow a droplet halfway off the
    /// body, the gooey bridge between them still holding.
    fn action(&self, left: f32, top: f32, pal: &Palette) -> gpui_kit::Div {
        const W: f32 = 210.0;
        const H: f32 = 48.0;
        let (body, style) = action_glass(W, H, 0.6, pal);
        let ink = self.ink();
        placed(left, top, W, H)
            .child(self.lens_on(body, 999.0, style))
            .child(
                placed(0.0, 0.0, body.size.width.into(), H)
                    .child(label("Send", 16.0, ink, pal.text)),
            )
            .child(placed(W - H, 0.0, H, H).child(label("\u{2192}", 18.0, ink, pal.text)))
    }

    /// The select with its list hanging off the button by a neck.
    fn select(&self, left: f32, top: f32, pal: &Palette) -> gpui_kit::Div {
        const W: f32 = 160.0;
        const H: f32 = 44.0;
        const GAP: f32 = 10.0;
        const ROW: f32 = 36.0;
        let options = ["Viewer", "Editor", "Owner"];
        let list = (W, ROW * options.len() as f32 + 12.0);
        let drop = H / 2.0 + GAP + list.1 / 2.0;
        let ink = self.ink();
        let body = Bounds::new(point(px(0.0), px(0.0)), size(px(W), px(H)));
        let mut select = placed(left, top, W, H)
            .child(self.lens_on(body, 16.0, select_glass(W, drop, list, pal)))
            .child(label("Role", 15.0, ink, pal.text));
        for (i, option) in options.into_iter().enumerate() {
            let y = H + GAP + 6.0 + i as f32 * ROW;
            select = select.child(placed(0.0, y, W, ROW).child(label(option, 14.0, ink, pal.text)));
        }
        select
    }

    fn button(
        &self,
        left: f32,
        top: f32,
        label: &str,
        text: Hsla,
        style: LensStyle,
    ) -> gpui_kit::Div {
        placed(left, top, 148.0, 52.0)
            .flex()
            .items_center()
            .justify_center()
            .child(self.lens(999.0, style))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(text)
                    .child(label.to_owned()),
            )
    }

    fn tabs(&self, left: f32, top: f32, pal: &Palette) -> gpui_kit::Div {
        let travel = TAB_W * 0.55;
        let lens_h = TAB_H - 8.0;
        let mut bar = placed(left, top, TAB_W * TABS.len() as f32 + 8.0, TAB_H)
            .child(self.lens(999.0, clear_button(0.0, 0.35, pal)))
            .child(
                placed(4.0 + travel, 4.0, TAB_W, lens_h)
                    .child(self.lens(999.0, selection(1.0, lens_h, pal))),
            );
        for (i, name) in TABS.iter().enumerate() {
            bar = bar.child(
                placed(4.0 + i as f32 * TAB_W, 0.0, TAB_W, TAB_H)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if i == 1 { pal.text } else { pal.muted })
                    .child(*name),
            );
        }
        bar
    }

    fn card(&self, left: f32, top: f32, pal: &Palette) -> gpui_kit::Div {
        placed(left, top, 320.0, 190.0)
            .child(self.lens(26.0, card(0.065 * 800.0, pal)))
            .child(
                div()
                    .size_full()
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(div().text_size(px(13.0)).text_color(pal.muted).child("Frosted card"))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(pal.text)
                            .child("Weekend hike"),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(pal.muted)
                            .child("The rim pulls the chat toward the centre, the face frosts it, and a thin line catches the light."),
                    ),
            )
    }
}

/// An absolutely placed box, in window points.
fn placed(left: f32, top: f32, width: f32, height: f32) -> gpui_kit::Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(width))
        .h(px(height))
}

impl Render for GlassOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::new(cx.theme().is_dark());
        let blue = rgb(0.04, 0.52, 0.96, 1.0);
        let white = rgb(1.0, 1.0, 1.0, 1.0);
        div()
            .id("glass-demo")
            .size_full()
            .relative()
            .child(self.inner.clone())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(self.button(
                        10.0,
                        300.0,
                        "Compose",
                        pal.text,
                        clear_button(0.0, 0.6, &pal),
                    ))
                    .child(self.button(170.0, 300.0, "Save", white, tinted(blue, &pal)))
                    .child(self.tabs(390.0, 172.0, &pal))
                    .child(self.card(560.0, 236.0, &pal))
                    .child(self.select(385.0, 236.0, &pal))
                    .child(self.action(12.0, 470.0, &pal))
                    .child(
                        placed(768.0, 435.0, 124.0, 124.0)
                            .child(self.lens(62.0, magnifier(LensDetail::Copied, &pal))),
                    )
                    .child(placed(1030.0, 435.0, 124.0, 124.0).child(
                        self.lens(62.0, magnifier(LensDetail::Rendered { density: 3 }, &pal)),
                    )),
            )
    }
}

#[cfg(test)]
#[path = "backdrop_lens_tests.rs"]
mod backdrop_lens_tests;
