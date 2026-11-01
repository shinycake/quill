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
    VisualTestContext, Window, canvas, div, hsla, point, px, size,
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

/// What [`Dimmed`] shows at full opacity, and inside a box at half: the
/// layer sits under that box too, as a slice and its painter sit under
/// whatever dims the window.
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
