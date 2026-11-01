//! A tick of an animation layer draws the layer alone: nothing around it
//! renders. Live views are GPUI's, so the first tests hold them to what a
//! whole frame would show, on small views; the last ones run the demo.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::{frames_over, take_slice_renders};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, Hsla, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Quad, Render, Styled, TestAppContext,
    VisualTestContext, Window, canvas, deferred, div, hsla, point, px, size,
};
use quill::sticker_playback::player::DecodedFrames;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

const STICKER_FILES: [i32; 2] = [44, 45];

fn color(hue: f32) -> Hsla {
    hsla(hue, 1., 0.5, 1.)
}

/// A fill by the frame the view rendered, and the state its element keeps
/// between frames: the renders so far, element state included.
struct Live {
    hue: f32,
    renders: Rc<Cell<u32>>,
    kept: Rc<Cell<u32>>,
}

impl Render for Live {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let kept = self.kept.clone();
        let hue = self.hue;
        window.with_global_id("kept".into(), |id, window| {
            window.with_element_state::<u32, _>(id, |state, _| {
                let next = state.unwrap_or(0) + 1;
                kept.set(next);
                ((), next)
            });
        });
        div().absolute().left_0().top_0().size(px(10.)).child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    window.paint_quad(gpui_kit::fill(bounds, color(hue)));
                },
            )
            .size_full(),
        )
    }
}

struct Page {
    live: Entity<Live>,
    under: f32,
    over: f32,
    renders: Rc<Cell<u32>>,
    clicks: Rc<Cell<u32>>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .relative()
            .child(div().absolute().size(px(100.)).bg(color(self.under)))
            .child(self.live.clone().live())
            .child(
                div()
                    .id("over")
                    .absolute()
                    .top(px(50.))
                    .left_0()
                    .size(px(20.))
                    .bg(color(self.over))
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        clicks.set(clicks.get() + 1);
                    }),
            )
    }
}

const UNDER: f32 = 0.1;
const OVER: f32 = 0.2;
const FIRST: f32 = 0.3;
const SECOND: f32 = 0.4;
const THIRD: f32 = 0.5;

struct Scene {
    page: Entity<Page>,
    live: Entity<Live>,
    page_renders: Rc<Cell<u32>>,
    live_renders: Rc<Cell<u32>>,
    kept: Rc<Cell<u32>>,
    clicks: Rc<Cell<u32>>,
    vcx: VisualTestContext,
}

impl Scene {
    fn open(cx: &mut TestAppContext) -> Self {
        cx.update(gpui_kit::init);
        let (page_renders, live_renders) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (kept, clicks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (page_counter, live_counter) = (page_renders.clone(), live_renders.clone());
        let (kept_in, clicks_in) = (kept.clone(), clicks.clone());
        let slot = Rc::new(RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
            let live = cx.new(|_| Live {
                hue: FIRST,
                renders: live_counter,
                kept: kept_in,
            });
            *slot_in.borrow_mut() = Some(live.clone());
            Page {
                live,
                under: UNDER,
                over: OVER,
                renders: page_counter,
                clicks: clicks_in,
            }
        });
        let page: Entity<Page> = handle.root(cx).expect("the page");
        let live = slot.borrow().clone().expect("the live view");
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
        Self {
            page,
            live,
            page_renders,
            live_renders,
            kept,
            clicks,
            vcx,
        }
    }

    fn quads(&mut self) -> Vec<Quad> {
        self.vcx.update(|window, _| window.painted_quads())
    }

    /// The hues in the shown scene, from the bottom up.
    fn hues(&mut self) -> Vec<f32> {
        let mut quads = self.quads();
        quads.sort_by_key(|quad| quad.order);
        quads
            .iter()
            .filter_map(|quad| quad.background.as_solid())
            .map(|color| color.h)
            .collect()
    }

    fn set_live(&mut self, hue: f32) {
        self.live.update(&mut self.vcx, |live, cx| {
            live.hue = hue;
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn set_under(&mut self, hue: f32) {
        self.page.update(&mut self.vcx, |page, cx| {
            page.under = hue;
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn click(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_down(at, MouseButton::Left, Default::default());
        self.vcx.run_until_parked();
        self.vcx
            .simulate_mouse_up(at, MouseButton::Left, Default::default());
        self.vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_live_view_redraws_in_place_alone(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    assert_eq!(scene.hues(), vec![UNDER, FIRST, OVER]);
    let (page, live) = (scene.page_renders.get(), scene.live_renders.get());

    scene.set_live(SECOND);
    assert_eq!(scene.hues(), vec![UNDER, SECOND, OVER]);
    assert_eq!(scene.page_renders.get(), page, "its parent stayed");
    assert_eq!(scene.live_renders.get(), live + 1);

    scene.set_live(THIRD);
    assert_eq!(scene.hues(), vec![UNDER, THIRD, OVER]);
    assert_eq!(scene.page_renders.get(), page);
}

struct Shell {
    inner: Entity<Live>,
    renders: Rc<Cell<u32>>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size(px(10.)).child(self.inner.clone().live())
    }
}

struct Nest {
    shell: Entity<Shell>,
    renders: Rc<Cell<u32>>,
}

impl Render for Nest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().child(self.shell.clone().live())
    }
}

#[gpui_kit::test]
fn a_live_view_inside_another_redraws_with_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (nest_renders, shell_renders) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (nest_counter, shell_counter) = (nest_renders.clone(), shell_renders.clone());
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Live {
            hue: FIRST,
            renders: Rc::default(),
            kept: Rc::default(),
        });
        *slot_in.borrow_mut() = Some(inner.clone());
        Nest {
            shell: cx.new(|_| Shell {
                inner,
                renders: shell_counter,
            }),
            renders: nest_counter,
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let (nest, shell) = (nest_renders.get(), shell_renders.get());

    inner.update(&mut vcx, |live, cx| {
        live.hue = SECOND;
        cx.notify();
    });
    vcx.run_until_parked();

    let quads = vcx.update(|window, _| window.painted_quads());
    let hues: Vec<_> = quads
        .iter()
        .filter_map(|quad| quad.background.as_solid())
        .map(|color| color.h)
        .collect();
    assert_eq!(hues, vec![SECOND]);
    assert_eq!(nest_renders.get(), nest, "what holds them stayed");
    assert_eq!(shell_renders.get(), shell + 1);
}

/// What a live view keeps between frames survives a redraw on its own,
/// and the next whole frame finds it.
#[gpui_kit::test]
fn a_live_view_keeps_its_element_state(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    let first = scene.kept.get();
    scene.set_live(SECOND);
    assert_eq!(scene.kept.get(), first + 1);
    scene.set_live(THIRD);
    assert_eq!(scene.kept.get(), first + 2);
    scene.set_under(OVER);
    assert_eq!(scene.kept.get(), first + 3);
    scene.set_live(SECOND);
    assert_eq!(scene.kept.get(), first + 4);
}

/// Whole frames and live ones alternate without losing either's paint.
#[gpui_kit::test]
fn a_whole_frame_follows_a_live_one(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    scene.set_live(SECOND);
    let page = scene.page_renders.get();
    scene.set_under(THIRD);
    assert_eq!(scene.page_renders.get(), page + 1);
    assert_eq!(scene.hues(), vec![THIRD, SECOND, OVER]);
    scene.set_live(FIRST);
    assert_eq!(scene.hues(), vec![THIRD, FIRST, OVER]);
    assert_eq!(scene.page_renders.get(), page + 1);
}

/// A live view notified with its parent draws with the whole frame.
#[gpui_kit::test]
fn a_notified_parent_makes_a_whole_frame(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    let page = scene.page_renders.get();
    let (page_entity, live) = (scene.page.clone(), scene.live.clone());
    scene.vcx.update(|_, cx| {
        live.update(cx, |live, cx| {
            live.hue = SECOND;
            cx.notify();
        });
        page_entity.update(cx, |page, cx| {
            page.under = THIRD;
            cx.notify();
        });
    });
    scene.vcx.run_until_parked();
    assert_eq!(scene.page_renders.get(), page + 1);
    assert_eq!(scene.hues(), vec![THIRD, SECOND, OVER]);
}

/// Hit tests read the last whole frame: a press after a live redraw lands
/// as before, on what is above the live view too.
#[gpui_kit::test]
fn clicks_land_after_a_live_redraw(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    scene.set_live(SECOND);
    scene.click(point(px(5.), px(55.)));
    assert_eq!(scene.clicks.get(), 1);
    scene.set_live(THIRD);
    scene.click(point(px(5.), px(55.)));
    assert_eq!(scene.clicks.get(), 2);
    scene.click(point(px(90.), px(90.)));
    assert_eq!(scene.clicks.get(), 2, "not on the button");
}

#[gpui_kit::test]
fn a_live_redraw_is_a_frame(cx: &mut TestAppContext) {
    let mut scene = Scene::open(cx);
    let frame = |scene: &mut Scene| scene.vcx.update(|window, _| window.frame_number());
    let before = frame(&mut scene);
    scene.set_live(SECOND);
    assert_eq!(frame(&mut scene), before + 1);
}

/// The demo with two looping 30 fps stickers in the open chat, in an
/// active window, their first frames drawn.
fn open_stickers(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-sticker-playback");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        for file in STICKER_FILES {
            view.read(cx).playback.stickers.set_fixture(
                file,
                Arc::new(|edge| DecodedFrames {
                    frames: (0..30u8)
                        .map(|i| vec![i; (edge as usize).pow(2) * 4])
                        .collect(),
                    fps: 30.0,
                }),
            );
        }
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..4 {
        vcx.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
        });
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
        app.update(&mut vcx, |_, cx| cx.notify());
    }
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn a_sticker_tick_renders_the_layers_alone(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_stickers(cx);
    let vcx = &mut vcx;
    let _ = frames_over(vcx, 1);
    let drawn = frames_over(vcx, 2);
    let count = |kind: &str| drawn.get(kind).copied().unwrap_or(0);
    assert!(count("animation layer") > 10, "it plays: {drawn:?}");
    assert!(count("(window frame)") > 5, "the frames count: {drawn:?}");
    for kind in ["app", "sidebar", "conversation", "composer"] {
        assert_eq!(count(kind), 0, "{kind} stayed: {drawn:?}");
    }
    let _ = take_slice_renders();
}

/// Paints a layered item inside a half transparent box, and the layer after
/// it, as the painter does.
struct Dimmed {
    layer: super::Layer,
    painted_at: Rc<Cell<f32>>,
    /// The opacity of what holds both.
    around: f32,
}

impl Render for Dimmed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.layer.begin_frame();
        let painted_at = self.painted_at.clone();
        let item = super::with_layer(Some(&self.layer), || {
            super::painter(10, move |_, window| {
                painted_at.set(window.element_opacity());
            })
            .size(px(10.))
        });
        let (prepaint, paint) = (self.layer.clone(), self.layer.clone());
        div()
            .size_full()
            .opacity(self.around)
            .child(div().opacity(0.5).child(item))
            .child(
                canvas(
                    move |_, window, cx| prepaint.prepaint(window, cx),
                    move |_, mut built, window, cx| paint.paint(&mut built, window, cx),
                )
                .size_full(),
            )
    }
}

#[gpui_kit::test]
fn a_layer_keeps_the_opacity_of_what_it_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let painted_at = Rc::new(Cell::new(1.));
    let view_painted_at = painted_at.clone();
    let handle = cx.open_window(size(px(100.), px(100.)), move |window, cx| {
        let view = cx.new(|_| Dimmed {
            layer: super::Layer::default(),
            painted_at: view_painted_at,
            around: 1.,
        });
        Root::new(view, window, cx)
    });
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    assert_eq!(painted_at.get(), 0.5);
}

/// Paints only once shown, so a whole frame can leave it an empty range.
struct Toggle {
    shown: bool,
}

impl Render for Toggle {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown;
        div().size(px(10.)).child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    if shown {
                        window.paint_quad(gpui_kit::fill(bounds, color(SECOND)));
                    }
                },
            )
            .size_full(),
        )
    }
}

/// A live fill whose last child is a live toggle.
struct ToggleShell {
    inner: Entity<Toggle>,
}

impl Render for ToggleShell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(20.))
            .bg(color(FIRST))
            .child(self.inner.clone().live())
    }
}

struct ToggleNest {
    shell: Entity<ToggleShell>,
}

impl Render for ToggleNest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.shell.clone().live())
    }
}

#[gpui_kit::test]
fn a_live_view_that_painted_nothing_inside_another_shows_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Toggle { shown: false });
        *slot_in.borrow_mut() = Some(inner.clone());
        ToggleNest {
            shell: cx.new(|_| ToggleShell { inner }),
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();

    inner.update(&mut vcx, |toggle, cx| {
        toggle.shown = true;
        cx.notify();
    });
    vcx.run_until_parked();

    let hues: Vec<_> = vcx
        .update(|window, _| window.painted_quads())
        .iter()
        .filter_map(|quad| quad.background.as_solid())
        .map(|color| color.h)
        .collect();
    assert_eq!(hues, vec![FIRST, SECOND], "the inner view paints once");
}

struct CachedHost {
    inner: Entity<Live>,
}

impl Render for CachedHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.inner.clone().live())
    }
}

/// A fill that notifies the page, beside a cached view holding a live one.
struct CachedPage {
    host: Entity<CachedHost>,
    under: f32,
}

impl Render for CachedPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(div().absolute().size(px(100.)).bg(color(self.under)))
            .child(
                div().absolute().size(px(50.)).child(
                    self.host
                        .clone()
                        .cached(gpui_kit::StyleRefinement::default().size_full()),
                ),
            )
    }
}

#[gpui_kit::test]
fn a_live_view_in_a_cached_view_keeps_its_latest_paint_through_a_whole_frame(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Live {
            hue: FIRST,
            renders: Rc::default(),
            kept: Rc::default(),
        });
        *slot_in.borrow_mut() = Some(inner.clone());
        CachedPage {
            host: cx.new(|_| CachedHost { inner }),
            under: UNDER,
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();

    inner.update(&mut vcx, |live, cx| {
        live.hue = SECOND;
        cx.notify();
    });
    vcx.run_until_parked();
    let page = handle.root(&mut vcx).expect("the page");
    page.update(&mut vcx, |page, cx| {
        page.under = THIRD;
        cx.notify();
    });
    vcx.run_until_parked();

    let hues: Vec<_> = vcx
        .update(|window, _| window.painted_quads())
        .iter()
        .filter_map(|quad| quad.background.as_solid())
        .map(|color| color.h)
        .collect();
    assert_eq!(
        hues,
        vec![THIRD, SECOND],
        "the live view keeps what it last drew"
    );
}

/// How [`Retried`] throws its first prepaint away. `List` rolls back through
/// `Window::transact` when it autoscrolls and prepaints again.
#[derive(Clone, Copy)]
enum Discard {
    RolledBack,
    Dropped,
}

/// What [`Retried`] prepainted first.
#[derive(Clone, Copy)]
enum First {
    /// The live view it goes on to keep.
    SameView,
    /// A live view around that one, which it keeps as a plain view.
    OuterLiveView,
}

fn prepaint(element: &mut AnyElement, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    element.layout_as_root(bounds.size.into(), window, cx);
    element.prepaint_at(bounds.origin, window, cx);
}

/// Prepaints `discarded` and throws it away, then keeps `kept`.
struct Retried {
    discarded: Rc<dyn Fn() -> AnyElement>,
    kept: Rc<dyn Fn() -> AnyElement>,
    discard: Discard,
    renders: Rc<Cell<u32>>,
}

impl Render for Retried {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let (discarded, kept) = (self.discarded.clone(), self.kept.clone());
        let discard = self.discard;
        div().size(px(10.)).child(
            canvas(
                move |bounds, window, cx| {
                    let mut first = discarded();
                    match discard {
                        Discard::RolledBack => {
                            let _ = window.transact(|window| {
                                prepaint(&mut first, bounds, window, cx);
                                Err::<(), ()>(())
                            });
                        }
                        Discard::Dropped => prepaint(&mut first, bounds, window, cx),
                    }
                    let mut kept = kept();
                    prepaint(&mut kept, bounds, window, cx);
                    kept
                },
                |_, mut kept, window, cx| kept.paint(window, cx),
            )
            .size_full(),
        )
    }
}

/// What a tick of the live view inside [`Retried`] did: its renders, those
/// of what holds it, and the hues shown.
#[derive(Debug, PartialEq)]
struct RetriedTick {
    inner_renders: u32,
    holder_renders: u32,
    hues: Vec<f32>,
}

fn tick_retried(cx: &mut TestAppContext, discard: Discard, first: First) -> RetriedTick {
    cx.update(gpui_kit::init);
    let (inner_renders, holder_renders) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (inner_counter, holder_counter) = (inner_renders.clone(), holder_renders.clone());
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Live {
            hue: FIRST,
            renders: inner_counter,
            kept: Rc::default(),
        });
        *slot_in.borrow_mut() = Some(inner.clone());
        let (discarded, kept): (Rc<dyn Fn() -> AnyElement>, Rc<dyn Fn() -> AnyElement>) =
            match first {
                First::SameView => {
                    let live: Rc<dyn Fn() -> AnyElement> =
                        Rc::new(move || inner.clone().live().into_any_element());
                    (live.clone(), live)
                }
                First::OuterLiveView => {
                    let shell = cx.new(|_| Shell {
                        inner,
                        renders: Rc::default(),
                    });
                    let outer = shell.clone();
                    (
                        Rc::new(move || outer.clone().live().into_any_element()),
                        Rc::new(move || shell.clone().into_any_element()),
                    )
                }
            };
        Retried {
            discarded,
            kept,
            discard,
            renders: holder_counter,
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let (inner_before, holder_before) = (inner_renders.get(), holder_renders.get());

    inner.update(&mut vcx, |live, cx| {
        live.hue = SECOND;
        cx.notify();
    });
    vcx.run_until_parked();

    RetriedTick {
        inner_renders: inner_renders.get() - inner_before,
        holder_renders: holder_renders.get() - holder_before,
        hues: vcx
            .update(|window, _| window.painted_quads())
            .iter()
            .filter_map(|quad| quad.background.as_solid())
            .map(|color| color.h)
            .collect(),
    }
}

fn redrew_alone() -> RetriedTick {
    RetriedTick {
        inner_renders: 1,
        holder_renders: 0,
        hues: vec![SECOND],
    }
}

#[gpui_kit::test]
fn a_live_view_prepainted_again_after_a_rollback_keeps_redrawing(cx: &mut TestAppContext) {
    let tick = tick_retried(cx, Discard::RolledBack, First::SameView);
    assert_eq!(tick, redrew_alone());
}

#[gpui_kit::test]
fn a_live_view_prepainted_twice_is_not_nested_in_itself(cx: &mut TestAppContext) {
    let tick = tick_retried(cx, Discard::Dropped, First::SameView);
    assert_eq!(tick, redrew_alone());
}

#[gpui_kit::test]
fn a_rolled_back_outer_live_view_leaves_no_trace_in_the_frame(cx: &mut TestAppContext) {
    let tick = tick_retried(cx, Discard::RolledBack, First::OuterLiveView);
    assert_eq!(tick, redrew_alone());
}

/// Nothing a patch draws holds this view, so its tick must not pass as one:
/// a tick that redraws nothing never asks for the next.
#[gpui_kit::test]
fn a_tick_no_patch_can_reach_takes_a_whole_frame(cx: &mut TestAppContext) {
    let tick = tick_retried(cx, Discard::Dropped, First::OuterLiveView);
    assert!(tick.inner_renders > 0, "the notified view rendered");
    assert!(tick.holder_renders > 0, "in a whole frame");
    assert_eq!(tick.hues, vec![SECOND], "and shows its new paint");
}

#[gpui_kit::test]
fn a_layer_under_a_dimmed_ancestor_dims_its_items_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let painted_at = Rc::new(Cell::new(1.));
    let view_painted_at = painted_at.clone();
    let handle = cx.open_window(size(px(100.), px(100.)), move |window, cx| {
        let view = cx.new(|_| Dimmed {
            layer: super::Layer::default(),
            painted_at: view_painted_at,
            around: 0.5,
        });
        Root::new(view, window, cx)
    });
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    assert_eq!(painted_at.get(), 0.25, "half of a half, and no more");
}

/// The hues in the shown scene, from the bottom up.
fn probe_hues(vcx: &mut VisualTestContext) -> Vec<f32> {
    let mut quads = vcx.update(|window, _| window.painted_quads());
    quads.sort_by_key(|quad| quad.order);
    quads
        .iter()
        .filter_map(|quad| quad.background.as_solid())
        .map(|color| color.h)
        .collect()
}

struct Hoverable {
    hue: f32,
    hover: bool,
    renders: Rc<Cell<u32>>,
}

impl Render for Hoverable {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let fill = div()
            .id("hoverable")
            .absolute()
            .left_0()
            .top_0()
            .size(px(10.))
            .bg(color(self.hue));
        if self.hover {
            fill.hover(|style| style.bg(color(OVER)))
        } else {
            fill
        }
    }
}

struct HoverPage {
    live: Entity<Hoverable>,
    renders: Rc<Cell<u32>>,
}

impl Render for HoverPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().relative().child(self.live.clone().live())
    }
}

/// A live view over a page, and what counts their renders.
struct HoverScene {
    live: Entity<Hoverable>,
    page_renders: Rc<Cell<u32>>,
    vcx: VisualTestContext,
}

impl HoverScene {
    fn open(cx: &mut TestAppContext, hover: bool) -> Self {
        cx.update(gpui_kit::init);
        let page_renders = Rc::new(Cell::new(0));
        let page_in = page_renders.clone();
        let slot = Rc::new(RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
            let live = cx.new(|_| Hoverable {
                hue: FIRST,
                hover,
                renders: Rc::default(),
            });
            *slot_in.borrow_mut() = Some(live.clone());
            HoverPage {
                live,
                renders: page_in,
            }
        });
        let live = slot.borrow().clone().expect("the live view");
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
        Self {
            live,
            page_renders,
            vcx,
        }
    }

    fn tick(&mut self, hue: f32) {
        self.live.update(&mut self.vcx, |live, cx| {
            live.hue = hue;
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn move_to(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_mouse_move(at, None, Default::default());
        self.vcx.run_until_parked();
    }

    fn hues(&mut self) -> Vec<f32> {
        probe_hues(&mut self.vcx)
    }
}

/// Hover comes from the last whole frame's hit test, which a patch can't
/// answer for what it built: such a view takes whole frames, so its hover
/// holds through every tick and ends when the pointer leaves.
#[gpui_kit::test]
fn hover_in_a_live_view_holds_through_its_ticks(cx: &mut TestAppContext) {
    let mut scene = HoverScene::open(cx, true);
    scene.move_to(point(px(5.), px(5.)));
    assert_eq!(scene.hues(), vec![OVER], "the pointer is on it");
    for hue in [SECOND, THIRD] {
        scene.tick(hue);
        assert_eq!(scene.hues(), vec![OVER], "a tick keeps the hover");
    }
    scene.move_to(point(px(100.), px(100.)));
    assert_eq!(scene.hues(), vec![THIRD], "and leaving ends it");
    scene.tick(FIRST);
    assert_eq!(scene.hues(), vec![FIRST]);
}

#[gpui_kit::test]
fn a_live_view_with_nothing_to_hover_ticks_alone(cx: &mut TestAppContext) {
    let mut scene = HoverScene::open(cx, false);
    let page = scene.page_renders.get();
    scene.tick(SECOND);
    assert_eq!(scene.hues(), vec![SECOND]);
    assert_eq!(scene.page_renders.get(), page, "its page stayed");
}

#[gpui_kit::test]
fn a_hover_a_patch_built_gets_a_whole_frame(cx: &mut TestAppContext) {
    let mut scene = HoverScene::open(cx, false);
    scene.move_to(point(px(5.), px(5.)));
    let page = scene.page_renders.get();
    scene.live.update(&mut scene.vcx, |live, cx| {
        live.hover = true;
        cx.notify();
    });
    scene.vcx.run_until_parked();
    assert!(scene.page_renders.get() > page, "a whole frame came");
    assert_eq!(scene.hues(), vec![OVER]);
}

/// A layer's painter, as a slice's sibling.
struct TilePainter {
    layer: super::Layer,
}

impl Render for TilePainter {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (prepaint, paint) = (self.layer.clone(), self.layer.clone());
        canvas(
            move |_, window, cx| prepaint.prepaint(window, cx),
            move |_, mut built, window, cx| paint.paint(&mut built, window, cx),
        )
        .size_full()
    }
}

/// A slice holding a tile, its layer over it.
struct TilePage {
    layer: super::Layer,
    painter: Entity<TilePainter>,
    renders: Rc<Cell<u32>>,
}

impl Render for TilePage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        self.layer.begin_frame();
        let tile = super::with_layer(Some(&self.layer), || {
            super::hover_tile(div().size(px(20.)), 0, |hovered| {
                let hue = if hovered { OVER } else { FIRST };
                Some(div().size(px(20.)).bg(color(hue)).into_any_element())
            })
        });
        div()
            .size_full()
            .relative()
            .child(tile)
            .child(self.painter.clone().live())
    }
}

#[gpui_kit::test]
fn a_hover_tile_shows_the_pointer_in_patches(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let renders = Rc::new(Cell::new(0));
    let renders_in = renders.clone();
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let layer = super::Layer::default();
        let painter = cx.new(|_| TilePainter {
            layer: layer.clone(),
        });
        *slot_in.borrow_mut() = Some(painter.clone());
        TilePage {
            layer,
            painter,
            renders: renders_in,
        }
    });
    let painter = slot.borrow().clone().expect("the painter");
    let vcx = &mut VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let page = renders.get();
    for (at, hue) in [
        (point(px(5.), px(5.)), OVER),
        (point(px(100.), px(100.)), FIRST),
    ] {
        vcx.simulate_mouse_move(at, None, Default::default());
        painter.update(vcx, |_, cx| cx.notify());
        vcx.run_until_parked();
        assert_eq!(probe_hues(vcx), vec![hue]);
    }
    assert_eq!(renders.get(), page, "its page stayed");
}

struct CountedHost {
    inner: Entity<Live>,
    deferred: bool,
    renders: Rc<Cell<u32>>,
}

impl Render for CountedHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let live = self.inner.clone().live();
        let live = if self.deferred {
            deferred(live).into_any_element()
        } else {
            live.into_any_element()
        };
        div().size_full().child(live)
    }
}

struct CountedPage {
    host: Entity<CountedHost>,
    under: f32,
    renders: Rc<Cell<u32>>,
}

impl Render for CountedPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .size_full()
            .relative()
            .child(div().absolute().size(px(100.)).bg(color(self.under)))
            .child(
                div().absolute().size(px(50.)).child(
                    self.host
                        .clone()
                        .cached(gpui_kit::StyleRefinement::default().size_full()),
                ),
            )
    }
}

/// A cached view holding a live one beside a page's fill, and what counts
/// their renders.
struct CachedScene {
    page: Entity<CountedPage>,
    inner: Entity<Live>,
    page_renders: Rc<Cell<u32>>,
    host_renders: Rc<Cell<u32>>,
    vcx: VisualTestContext,
}

impl CachedScene {
    fn open(cx: &mut TestAppContext) -> Self {
        Self::open_with(cx, false)
    }

    fn open_with(cx: &mut TestAppContext, deferred: bool) -> Self {
        cx.update(gpui_kit::init);
        let (page_renders, host_renders) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (page_in, host_in) = (page_renders.clone(), host_renders.clone());
        let slot = Rc::new(RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
            let inner = cx.new(|_| Live {
                hue: FIRST,
                renders: Rc::default(),
                kept: Rc::default(),
            });
            *slot_in.borrow_mut() = Some(inner.clone());
            CountedPage {
                host: cx.new(|_| CountedHost {
                    inner,
                    deferred,
                    renders: host_in,
                }),
                under: UNDER,
                renders: page_in,
            }
        });
        let page = handle.root(cx).expect("the page");
        let inner = slot.borrow().clone().expect("the inner view");
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
        Self {
            page,
            inner,
            page_renders,
            host_renders,
            vcx,
        }
    }

    fn tick(&mut self, hue: f32) {
        self.inner.update(&mut self.vcx, |live, cx| {
            live.hue = hue;
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn set_under(&mut self, hue: f32) {
        self.page.update(&mut self.vcx, |page, cx| {
            page.under = hue;
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn hues(&mut self) -> Vec<f32> {
        probe_hues(&mut self.vcx)
    }

    /// Whole frames and renders of the cached view so far.
    fn renders(&self) -> (u32, u32) {
        (self.page_renders.get(), self.host_renders.get())
    }
}

/// With no patch before it, a whole frame replays the cached view: the live
/// view it holds stays a live view, so its ticks stay patches.
#[gpui_kit::test]
fn a_live_view_in_a_replayed_cached_view_keeps_ticking_alone(cx: &mut TestAppContext) {
    let mut scene = CachedScene::open(cx);
    let (_, hosts) = scene.renders();
    scene.set_under(THIRD);
    assert_eq!(scene.renders().1, hosts, "the whole frame replayed it");
    assert_eq!(scene.hues(), vec![THIRD, FIRST]);
    let renders = scene.renders();
    for hue in [SECOND, FIRST, SECOND] {
        scene.tick(hue);
        assert_eq!(scene.hues(), vec![THIRD, hue]);
    }
    assert_eq!(scene.renders(), renders, "ticks after it stay patches");
    scene.set_under(UNDER);
    assert_eq!(scene.hues(), vec![UNDER, SECOND]);
}

/// A deferred draw replays with the cached view that holds it, later in the
/// frame.
#[gpui_kit::test]
fn a_deferred_live_view_in_a_replayed_cached_view_keeps_ticking_alone(cx: &mut TestAppContext) {
    let mut scene = CachedScene::open_with(cx, true);
    let (_, hosts) = scene.renders();
    scene.set_under(THIRD);
    assert_eq!(scene.renders().1, hosts, "the whole frame replayed it");
    let renders = scene.renders();
    scene.tick(SECOND);
    assert_eq!(scene.hues(), vec![THIRD, SECOND]);
    assert_eq!(scene.renders(), renders, "the tick stayed a patch");
}

/// The same after patches, whose whole frame renders the cached view.
#[gpui_kit::test]
fn a_live_view_in_a_cached_view_ticks_alone_after_a_whole_frame(cx: &mut TestAppContext) {
    let mut scene = CachedScene::open(cx);
    scene.tick(SECOND);
    scene.set_under(THIRD);
    assert_eq!(scene.hues(), vec![THIRD, SECOND]);
    let renders = scene.renders();
    for hue in [FIRST, SECOND] {
        scene.tick(hue);
        assert_eq!(scene.hues(), vec![THIRD, hue]);
    }
    assert_eq!(scene.renders(), renders);
}

struct CachedShell {
    host: Entity<CachedHost>,
    renders: Rc<Cell<u32>>,
}

impl Render for CachedShell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size(px(50.)).child(
            self.host
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}

struct OuterPage {
    shell: Entity<CachedShell>,
    renders: Rc<Cell<u32>>,
}

impl Render for OuterPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().child(self.shell.clone().live())
    }
}

#[gpui_kit::test]
fn a_live_view_behind_a_cached_view_in_a_live_view_shows_every_tick(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let page_renders = Rc::new(Cell::new(0));
    let page_in = page_renders.clone();
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Live {
            hue: FIRST,
            renders: Rc::default(),
            kept: Rc::default(),
        });
        *slot_in.borrow_mut() = Some(inner.clone());
        OuterPage {
            shell: cx.new(|cx| CachedShell {
                host: cx.new(|_| CachedHost { inner }),
                renders: Rc::default(),
            }),
            renders: page_in,
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let page = page_renders.get();
    for hue in [SECOND, THIRD, FIRST] {
        inner.update(&mut vcx, |live, cx| {
            live.hue = hue;
            cx.notify();
        });
        vcx.run_until_parked();
        assert_eq!(probe_hues(&mut vcx), vec![hue]);
    }
    assert_eq!(page_renders.get(), page, "the ticks stayed patches");
}

/// Two fills, then one: what a cached view after them finds moves.
struct Shrinking {
    fixed: Entity<Fixed>,
    extra: bool,
}

impl Render for Shrinking {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(50.))
            .relative()
            .child(div().absolute().size(px(10.)).bg(color(UNDER)))
            .children(
                self.extra
                    .then(|| div().absolute().size(px(10.)).bg(color(OVER))),
            )
            .child(
                self.fixed
                    .clone()
                    .cached(gpui_kit::StyleRefinement::default().size_full()),
            )
    }
}

struct Fixed;

impl Render for Fixed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(10.)).bg(color(FIRST))
    }
}

struct ShrinkingPage {
    shell: Entity<Shrinking>,
    under: f32,
}

impl Render for ShrinkingPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(div().absolute().size(px(100.)).bg(color(self.under)))
            .child(self.shell.clone().live())
    }
}

#[gpui_kit::test]
fn a_whole_frame_after_a_patch_replays_no_range_of_the_patch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let shell = cx.new(|cx| Shrinking {
            fixed: cx.new(|_| Fixed),
            extra: true,
        });
        *slot_in.borrow_mut() = Some(shell.clone());
        ShrinkingPage {
            shell,
            under: THIRD,
        }
    });
    let shell = slot.borrow().clone().expect("the live view");
    let page: Entity<ShrinkingPage> = handle.root(cx).expect("the page");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    assert_eq!(probe_hues(&mut vcx), vec![THIRD, UNDER, OVER, FIRST]);
    shell.update(&mut vcx, |shell, cx| {
        shell.extra = false;
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(probe_hues(&mut vcx), vec![THIRD, UNDER, FIRST]);
    page.update(&mut vcx, |page, cx| {
        page.under = SECOND;
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(probe_hues(&mut vcx), vec![SECOND, UNDER, FIRST]);
}

#[gpui_kit::test]
fn a_live_view_inside_another_ticks_alone_tick_after_tick(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let nest_renders = Rc::new(Cell::new(0));
    let nest_counter = nest_renders.clone();
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let inner = cx.new(|_| Live {
            hue: FIRST,
            renders: Rc::default(),
            kept: Rc::default(),
        });
        *slot_in.borrow_mut() = Some(inner.clone());
        Nest {
            shell: cx.new(|_| Shell {
                inner,
                renders: Rc::default(),
            }),
            renders: nest_counter,
        }
    });
    let inner = slot.borrow().clone().expect("the inner view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let nest = nest_renders.get();
    for hue in [SECOND, THIRD, SECOND] {
        inner.update(&mut vcx, |live, cx| {
            live.hue = hue;
            cx.notify();
        });
        vcx.run_until_parked();
        assert_eq!(probe_hues(&mut vcx), vec![hue]);
    }
    assert_eq!(nest_renders.get(), nest, "every tick stayed a patch");
}
