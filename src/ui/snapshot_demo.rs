//! Screenshot demo for snapshot transforms (`snapshot`, the vendored GPUI's
//! `window/snapshot.rs` and `scene/snapshot.rs`;
//! docs/decisions/snapshot-transitions.md). The Floating layout's menu
//! pop and page cross draw through them.
//!
//! `ready-snapshot` lays out one card, with large and small text, emoji, an
//! image, a shadow and a rounded clip, drawn live and then through a
//! snapshot: at identity (it must match the live card), at .8 about each of
//! its four corners, moved, at half opacity, without a backing colour, and
//! through a snapshot dropped after its first frame and captured again.
//! `QUILL_SNAPSHOTS=off` draws every card live, the fallback every owner
//! has.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::component::Root;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::{
    App, AppContext, BoxShadow, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, Render, Rgba, Snapshot, SnapshotDraw, Styled, Window, div, img, point, px,
    snapshot,
};

register_demos![
    DemoSpec::chats(
        "ready-snapshot",
        "screenshot demo — a card drawn live and through snapshots"
    )
    .setup(QuillApp::demo_ready_snapshot)
    .timing(2000, 3000)
];

impl QuillApp {
    fn demo_ready_snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.defer(cx, show);
    }
}

/// Replaces the window's content with the demo, under a kit `Root` so the
/// capture finds one.
fn show(window: &mut Window, cx: &mut App) {
    let snapshots = TILES.map(|_| window.new_snapshot());
    let demo = cx.new(|_| SnapshotDemo {
        snapshots,
        recaptured: Recapture::FirstFrame,
    });
    window.replace_root(cx, |window, cx| Root::new(demo, window, cx));
}

const CARD_W: f32 = 200.0;
const CARD_H: f32 = 140.0;
const RADIUS: f32 = 24.0;
const GAP: f32 = 44.0;
const COLUMNS: usize = 4;

/// How a tile draws its card.
#[derive(Clone, Copy)]
enum Tile {
    Live,
    Identity,
    /// .8 about a corner: (0, 0) top left to (1, 1) bottom right.
    Corner(f32, f32),
    Moved,
    HalfOpacity,
    NoBacking,
    /// Through a snapshot dropped after the first frame and captured again.
    Recaptured,
}

const TILES: [Tile; 10] = [
    Tile::Live,
    Tile::Identity,
    Tile::Corner(0.0, 0.0),
    Tile::Corner(1.0, 0.0),
    Tile::Corner(0.0, 1.0),
    Tile::Corner(1.0, 1.0),
    Tile::Moved,
    Tile::HalfOpacity,
    Tile::NoBacking,
    Tile::Recaptured,
];

impl Tile {
    fn label(self) -> &'static str {
        match self {
            Tile::Live => "live",
            Tile::Identity => "snapshot at identity",
            Tile::Corner(0.0, 0.0) => ".8 about top left",
            Tile::Corner(_, 0.0) => ".8 about top right",
            Tile::Corner(0.0, _) => ".8 about bottom left",
            Tile::Corner(_, _) => ".8 about bottom right",
            Tile::Moved => "moved 16, 10",
            Tile::HalfOpacity => "half opacity",
            Tile::NoBacking => "identity, no backing",
            Tile::Recaptured => "captured again",
        }
    }

    /// How the snapshot is drawn for a card at `origin`.
    fn draw(self, origin: Point<Pixels>) -> Option<SnapshotDraw> {
        match self {
            Tile::Live => None,
            Tile::Identity | Tile::NoBacking | Tile::Recaptured => Some(SnapshotDraw::IDENTITY),
            Tile::Corner(x, y) => Some(SnapshotDraw::scaled_about(
                point(origin.x + px(CARD_W * x), origin.y + px(CARD_H * y)),
                0.8,
            )),
            Tile::Moved => Some(SnapshotDraw::moved(point(px(16.0), px(10.0)))),
            Tile::HalfOpacity => Some(SnapshotDraw::IDENTITY.faded(0.5)),
        }
    }
}

/// The recaptured tile's step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Recapture {
    /// Before the first frame is drawn.
    FirstFrame,
    /// Its snapshot was replaced by a new one after the first frame.
    Replaced,
}

struct SnapshotDemo {
    snapshots: [Snapshot; TILES.len()],
    recaptured: Recapture,
}

fn rgb(r: f32, g: f32, b: f32) -> Hsla {
    Rgba { r, g, b, a: 1.0 }.into()
}

struct Palette {
    page: Hsla,
    card: Hsla,
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                page: rgb(0.07, 0.075, 0.09),
                card: rgb(0.14, 0.15, 0.17),
                text: rgb(0.95, 0.95, 0.96),
                muted: rgb(0.6, 0.62, 0.66),
                accent: rgb(0.2, 0.55, 0.98),
            }
        } else {
            Self {
                page: rgb(0.92, 0.93, 0.95),
                card: rgb(1.0, 1.0, 1.0),
                text: rgb(0.08, 0.09, 0.1),
                muted: rgb(0.42, 0.44, 0.48),
                accent: rgb(0.04, 0.52, 0.96),
            }
        }
    }
}

/// The card every tile draws.
fn card(pal: &Palette) -> Div {
    let photo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-gif-still-photo.jpg");
    div()
        .relative()
        .w(px(CARD_W))
        .h(px(CARD_H))
        .rounded(px(RADIUS))
        .bg(pal.card)
        .shadow(vec![BoxShadow {
            color: pal.accent.opacity(0.45),
            offset: point(px(0.0), px(6.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .clip_rounded()
        .child(
            img(photo)
                .absolute()
                .left_0()
                .top_0()
                .w(px(CARD_W))
                .h(px(64.0)),
        )
        .child(
            div()
                .absolute()
                .left(px(14.0))
                .top(px(68.0))
                .text_size(px(30.0))
                .text_color(pal.text)
                .child("Quill \u{1F600}"),
        )
        .child(
            div()
                .absolute()
                .left(px(14.0))
                .top(px(108.0))
                .text_xs()
                .text_color(pal.muted)
                .child("Small text stays sharp."),
        )
}

impl SnapshotDemo {
    fn tile(&self, index: usize, tile: Tile, pal: &Palette) -> impl IntoElement {
        let column = index % COLUMNS;
        let row = index / COLUMNS;
        let left = GAP + column as f32 * (CARD_W + GAP);
        let top = GAP + row as f32 * (CARD_H + GAP + 18.0);
        let origin = point(px(left), px(top));
        let content = match (tile.draw(origin), self.snapshots.get(index)) {
            (Some(draw), Some(held)) => {
                let element = snapshot(card(pal), held, Some(draw)).margin(px(32.0));
                match tile {
                    Tile::NoBacking => element.into_any_element(),
                    _ => element.backing(pal.card, px(RADIUS)).into_any_element(),
                }
            }
            _ => card(pal).into_any_element(),
        };
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .child(content)
            .child(
                div()
                    .pt(px(8.0))
                    .text_xs()
                    .text_color(pal.muted)
                    .child(tile.label()),
            )
    }
}

impl Render for SnapshotDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::new(cx.theme().is_dark());
        if self.recaptured == Recapture::FirstFrame {
            // After the first frame, the last tile's snapshot is replaced.
            let entity = cx.entity();
            window.on_next_frame(move |window, cx| {
                entity.update(cx, |demo, cx| {
                    if demo.recaptured == Recapture::FirstFrame {
                        if let Some(last) = demo.snapshots.last_mut() {
                            *last = window.new_snapshot();
                        }
                        demo.recaptured = Recapture::Replaced;
                        cx.notify();
                    }
                });
            });
        }
        div()
            .id("snapshot-demo")
            .size_full()
            .relative()
            .bg(pal.page)
            .children(
                TILES
                    .iter()
                    .enumerate()
                    .map(|(index, tile)| self.tile(index, *tile, &pal)),
            )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;
