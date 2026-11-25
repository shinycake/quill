//! Snapshot transforms (the vendored GPUI's `scene/snapshot.rs` and
//! `window/snapshot.rs`, docs/decisions/snapshot-transitions.md): what a
//! capture takes from the frame, when its texture is drawn again, how the
//! snapshot is drawn, and every way a snapshot ends.

use std::rc::Rc;

use gpui_kit::{
    AppContext as _, Bounds, Context, DevicePixels, Entity, IntoElement, ParentElement as _,
    Render, ScaledPixels, Snapshot, SnapshotDraw, SnapshotState, Styled as _, TestAppContext,
    VisualTestContext, Window, canvas, div, hsla, point, px, size, snapshot,
};

struct Child;

impl Render for Child {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(hsla(0.0, 1.0, 0.5, 1.0))
    }
}

struct Card {
    child: Entity<Child>,
    snapshot: Option<Snapshot>,
    draw: Option<SnapshotDraw>,
    side: f32,
    /// Paints a lens inside the capture too.
    lens: bool,
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let side = px(self.side);
        let child = self
            .child
            .clone()
            .cached(gpui_kit::StyleRefinement::default().size(side));
        let content = div().size(side).child(child);
        let content = if self.lens {
            content
                .child(
                    canvas(
                        |_, _, _| {},
                        |bounds, _, window, _| {
                            window.paint_backdrop_lens(bounds, px(8.0), &lens_style());
                        },
                    )
                    .absolute()
                    .size(px(20.0)),
                )
                .into_any_element()
        } else {
            content.into_any_element()
        };
        let card = div().absolute().left(px(10.0)).top(px(20.0)).size(side);
        div().size_full().child(match &self.snapshot {
            Some(held) => card.child(snapshot(content, held, self.draw)),
            None => card.child(content),
        })
    }
}

/// Holds the card as a cached view, so a frame that changes nothing
/// replays the card's capture whole. (A cached view that renders again
/// paints every view inside it afresh, so a notified card recaptures.)
struct Outer {
    card: Entity<Card>,
}

impl Render for Outer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.card
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}

fn lens_style() -> gpui_kit::LensStyle {
    gpui_kit::LensStyle {
        unit: 0.0,
        bevel: 50.0,
        band: None,
        shadow: 0.0,
        shadow_softness: 34.0,
        shadow_offset: 22.0,
        strength: 1.0,
        color: hsla(0.0, 0.0, 1.0, 1.0),
        surface: hsla(0.6, 0.5, 0.5, 1.0),
        material: gpui_kit::LensMaterial::Clear {
            bend: gpui_kit::LensBend::Edge {
                refract: 30.0,
                magnify: 0.1,
                dispersion: 0.1,
                blur_center: 0.35,
                blur_edge: 2.2,
            },
            frost: 0.0,
            blur: px(0.0),
            blur_mix: 1.0,
        },
        border: None,
        tune: gpui_kit::LensTune::default(),
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: gpui_kit::LensDetail::Copied,
    }
}

struct Harness {
    vcx: VisualTestContext,
    card: Entity<Card>,
    side: f32,
}

fn open(cx: &mut TestAppContext, supported: bool, side: f32) -> Harness {
    cx.update(gpui_kit::init);
    let made = Rc::new(std::cell::OnceCell::new());
    let made_in = made.clone();
    let handle = cx.open_window(size(px(400.0), px(300.0)), move |window, cx| {
        let child = cx.new(|_| Child);
        let card = cx.new(|_| Card {
            child,
            snapshot: Some(window.new_snapshot()),
            draw: Some(SnapshotDraw::IDENTITY),
            side,
            lens: false,
        });
        let _ = made_in.set(card.clone());
        let outer = cx.new(|_| Outer { card });
        gpui_kit::component::Root::new(outer, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, _| window.set_test_snapshots_supported(supported));
    vcx.run_until_parked();
    let card = made.get().cloned().expect("the card view");
    Harness { vcx, card, side }
}

/// What one frame drew.
#[derive(Debug, PartialEq)]
struct Drawn {
    /// How many of the child's quads the frame draws itself.
    child_quads: usize,
    /// Where the frame draws snapshots, device pixels.
    snapshots: Vec<Bounds<ScaledPixels>>,
    /// How many textures the frame draws first.
    captures: usize,
}

impl Harness {
    /// Draws a frame after `update`, without the refresh that would make
    /// every cached view paint afresh.
    fn frame(&mut self, update: impl FnOnce(&mut Card, &mut Context<Card>)) -> Drawn {
        self.draw(Some(update))
    }

    /// Draws a frame in which the card is not notified, so it replays.
    fn replay_frame(&mut self) -> Drawn {
        self.draw(None::<fn(&mut Card, &mut Context<Card>)>)
    }

    fn draw(&mut self, update: Option<impl FnOnce(&mut Card, &mut Context<Card>)>) -> Drawn {
        let side = self.side;
        let card = self.card.clone();
        // One update, so that flushing the notify draws no frame of its own.
        self.vcx.update(|window, cx| {
            if let Some(update) = update {
                card.update(cx, |card, cx| {
                    update(card, cx);
                    cx.notify();
                });
            }
            window.draw(cx).clear(cx);
            let child = ScaledPixels(side * window.scale_factor());
            let scene = window.rendered_scene();
            let of_child = |quad: &&gpui_kit::Quad| quad.bounds.size == size(child, child);
            Drawn {
                child_quads: scene.quads.iter().filter(of_child).count(),
                snapshots: scene.snapshots.iter().map(|quad| quad.bounds).collect(),
                captures: scene.snapshot_scenes.len(),
            }
        })
    }

    fn state(&mut self) -> Option<SnapshotState> {
        self.card.read_with(&self.vcx, |card, _| {
            card.snapshot.as_ref().map(Snapshot::state)
        })
    }
}

fn device(x: f32, y: f32, side: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: point(ScaledPixels(x), ScaledPixels(y)),
        size: size(ScaledPixels(side), ScaledPixels(side)),
    }
}

fn ready(side: i32, scale: f32) -> Option<SnapshotState> {
    Some(SnapshotState::Ready {
        size: size(DevicePixels(side), DevicePixels(side)),
        scale,
    })
}

#[gpui_kit::test]
fn without_support_the_child_paints_live(cx: &mut TestAppContext) {
    let mut h = open(cx, false, 100.0);
    let drawn = h.frame(|_, _| {});
    assert_eq!(
        drawn,
        Drawn {
            child_quads: 1,
            snapshots: vec![],
            captures: 0
        }
    );
    assert_eq!(h.state(), Some(SnapshotState::Empty));
}

#[gpui_kit::test]
fn a_capture_takes_the_subtree_out_of_the_frame_and_draws_it_once(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    // The test window draws at 2x.
    let card = device(20.0, 40.0, 200.0);
    assert_eq!(
        h.frame(|_, _| {}),
        Drawn {
            child_quads: 0,
            snapshots: vec![card],
            captures: 1
        }
    );
    assert_eq!(h.state(), ready(200, 2.0));
    // The child's quad, in the texture's texels.
    let captured = h.vcx.update(|window, _| {
        window
            .rendered_scene()
            .snapshot_scenes
            .first()
            .map(|capture| {
                capture
                    .scene
                    .quads
                    .iter()
                    .map(|quad| quad.bounds)
                    .collect::<Vec<_>>()
            })
    });
    assert_eq!(captured, Some(vec![device(0.0, 0.0, 200.0)]));

    // The card replays, its capture with it: the texture already holds the
    // picture, for as many frames as nothing changes.
    for _ in 0..2 {
        assert_eq!(
            h.replay_frame(),
            Drawn {
                child_quads: 0,
                snapshots: vec![card],
                captures: 0
            }
        );
    }
    // The child paints afresh: drawn again.
    let painted = h.frame(|card, cx| card.child.update(cx, |_, cx| cx.notify()));
    assert_eq!(painted.captures, 1);
    // Live again: no texture, the child in the frame.
    assert_eq!(
        h.frame(|card, _| card.draw = None),
        Drawn {
            child_quads: 1,
            snapshots: vec![],
            captures: 0
        }
    );
    // Back to the snapshot after a frame live: drawn again, never stale.
    assert_eq!(
        h.frame(|card, _| card.draw = Some(SnapshotDraw::IDENTITY))
            .captures,
        1
    );
}

#[gpui_kit::test]
fn the_snapshot_is_drawn_scaled_moved_and_faded(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    h.frame(|_, _| {});
    let drawn = h.frame(|card, _| {
        card.draw = Some(SnapshotDraw::scaled_about(point(px(10.0), px(20.0)), 0.5).faded(0.25));
    });
    assert_eq!(drawn.snapshots, [device(20.0, 40.0, 100.0)]);
    let opacity = h.vcx.update(|window, _| {
        window
            .rendered_scene()
            .snapshots
            .first()
            .map(|quad| quad.opacity)
    });
    assert_eq!(opacity, Some(0.25));
    let drawn = h.frame(|card, _| card.draw = Some(SnapshotDraw::moved(point(px(5.0), px(-10.0)))));
    assert_eq!(drawn.snapshots, [device(30.0, 20.0, 200.0)]);
}

#[gpui_kit::test]
fn a_lens_inside_a_capture_stays_in_the_frame(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    h.frame(|card, _| card.lens = true);
    let (lenses, captured) = h.vcx.update(|window, _| {
        let scene = window.rendered_scene();
        (
            scene.lenses.len(),
            scene
                .snapshot_scenes
                .iter()
                .map(|capture| capture.scene.lenses.len())
                .sum::<usize>(),
        )
    });
    assert_eq!((lenses, captured), (1, 0));
}

#[gpui_kit::test]
fn dropping_the_handle_releases_its_texture_at_the_next_frame(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    h.frame(|_, _| {});
    let slot = h
        .card
        .read_with(&h.vcx, |card, _| card.snapshot.as_ref().map(Snapshot::slot));
    let drawn = h.frame(|card, _| card.snapshot = None);
    assert_eq!((drawn.child_quads, drawn.snapshots), (1, vec![]));
    let released = h.vcx.update(|window, _| {
        (
            window.rendered_scene().snapshot_releases.clone(),
            window.live_snapshot_count(),
        )
    });
    assert_eq!(released, (slot.into_iter().collect(), 0));
}

#[gpui_kit::test]
fn a_scale_factor_change_or_a_lost_device_loses_the_picture(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    h.frame(|_, _| {});
    h.frame(|card, _| card.draw = None);
    h.vcx.simulate_scale_factor_change(1.0);
    h.frame(|_, _| {});
    assert_eq!(h.state(), Some(SnapshotState::Lost));

    // Captured again, at the new scale.
    assert_eq!(
        h.frame(|card, _| card.draw = Some(SnapshotDraw::IDENTITY))
            .captures,
        1
    );
    assert_eq!(h.state(), ready(100, 1.0));

    h.frame(|card, _| card.draw = None);
    h.vcx
        .update(|window, _| window.simulate_test_snapshot_loss());
    h.frame(|_, _| {});
    assert_eq!(h.state(), Some(SnapshotState::Lost));
}

#[gpui_kit::test]
fn a_capture_beyond_the_windows_limit_is_refused_and_paints_live(cx: &mut TestAppContext) {
    // 400 x 300 at 2x is 480,000 device pixels; the limit is 1.25 times that.
    let mut h = open(cx, true, 400.0);
    let drawn = h.frame(|_, _| {});
    assert_eq!((drawn.child_quads, drawn.captures), (1, 0));
    assert_eq!(h.state(), Some(SnapshotState::Empty));
}

#[gpui_kit::test]
fn a_last_frame_snapshot_is_drawn_at_once(cx: &mut TestAppContext) {
    let mut h = open(cx, true, 100.0);
    h.frame(|card, _| card.draw = None);
    let region = Bounds {
        origin: point(px(10.0), px(20.0)),
        size: size(px(100.0), px(100.0)),
    };
    let taken = h.vcx.update(|window, _| {
        let taken = window
            .snapshot_last_frame(region, 1.0)
            .map(|snapshot| (snapshot.slot(), snapshot.state()));
        (taken, window.test_snapshot_history().0)
    });
    let (Some((slot, state)), drawn) = taken else {
        panic!("a snapshot of quads");
    };
    assert_eq!(Some(state), ready(200, 2.0));
    assert_eq!(drawn.last(), Some(&slot));

    let half = h.vcx.update(|window, _| {
        window
            .snapshot_last_frame(region, 0.5)
            .map(|snapshot| snapshot.state())
    });
    assert_eq!(half, ready(100, 2.0));

    let refused = h.vcx.update(|window, _| {
        window.set_test_snapshots_supported(false);
        window.snapshot_last_frame(region, 1.0).is_none()
    });
    assert!(refused);
}
