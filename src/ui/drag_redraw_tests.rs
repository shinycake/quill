use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, ExternalPaths, FileDropEvent, InteractiveElement,
    IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels,
    PlatformInput, Point, Render, StatefulInteractiveElement, StyleRefinement, Styled,
    TestAppContext, Window, div, point, px,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Default)]
struct Counts {
    shell: Cell<usize>,
    slice: Cell<usize>,
    drag_over: Cell<usize>,
    covered: Cell<bool>,
    ghost_at: RefCell<Vec<Point<Pixels>>>,
}

#[derive(Clone)]
struct Payload;

struct Ghost(Rc<Counts>);

impl Render for Ghost {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.ghost_at.borrow_mut().push(window.mouse_position());
        div()
    }
}

struct Slice(Rc<Counts>);

impl Render for Slice {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.slice.set(self.0.slice.get() + 1);
        let ghost = self.0.clone();
        let drag_over = self.0.clone();
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .id("source")
                    .size(px(100.))
                    .on_drag(Payload, move |_, _, _, cx| cx.new(|_| Ghost(ghost.clone()))),
            )
            .child(
                div()
                    .id("target")
                    .size(px(100.))
                    .drag_over::<Payload>(move |style, _, _, _| {
                        drag_over.drag_over.set(drag_over.drag_over.get() + 1);
                        style
                    }),
            )
            .child(
                div()
                    .id("files")
                    .size(px(100.))
                    .drag_over::<ExternalPaths>({
                        let drag_over = self.0.clone();
                        move |style, _, _, _| {
                            drag_over.drag_over.set(drag_over.drag_over.get() + 1);
                            style
                        }
                    }),
            )
    }
}

struct Shell {
    counts: Rc<Counts>,
    slice: Entity<Slice>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.counts.shell.set(self.counts.shell.get() + 1);
        let cover = self.counts.covered.get().then(|| {
            div()
                .id("cover")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
        });
        div()
            .size_full()
            .relative()
            .child(
                self.slice
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
            .children(cover)
    }
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Rc<Counts>) {
    let counts = Rc::new(Counts::default());
    let shared = counts.clone();
    let window = cx.add_window(move |_, cx| Shell {
        slice: cx.new(|_| Slice(shared.clone())),
        counts: shared,
    });
    (window.into(), counts)
}

fn send(window: AnyWindowHandle, event: PlatformInput, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(event, cx);
    })
    .unwrap();
}

fn mouse_move(window: AnyWindowHandle, x: f32, cx: &mut TestAppContext) {
    send(
        window,
        PlatformInput::MouseMove(MouseMoveEvent {
            position: point(px(x), px(50.)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        }),
        cx,
    );
}

fn start_drag(window: AnyWindowHandle, cx: &mut TestAppContext) {
    send(
        window,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: point(px(10.), px(50.)),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    mouse_move(window, 40., cx);
}

#[gpui_kit::test]
fn a_drag_inside_one_hitbox_moves_the_ghost_without_rendering_cached_views(
    cx: &mut TestAppContext,
) {
    let (window, counts) = open(cx);
    start_drag(window, cx);
    let (shell, slice) = (counts.shell.get(), counts.slice.get());

    mouse_move(window, 60., cx);

    assert_eq!(counts.slice.get(), slice);
    assert_eq!(counts.shell.get(), shell + 1);
    assert_eq!(
        counts.ghost_at.borrow().last(),
        Some(&point(px(60.), px(50.)))
    );
}

#[gpui_kit::test]
fn a_repeated_position_draws_no_frame(cx: &mut TestAppContext) {
    let (window, counts) = open(cx);
    start_drag(window, cx);
    mouse_move(window, 60., cx);
    let (shell, slice) = (counts.shell.get(), counts.slice.get());

    mouse_move(window, 60., cx);

    assert_eq!(counts.slice.get(), slice);
    assert_eq!(counts.shell.get(), shell);
}

#[gpui_kit::test]
fn crossing_into_a_drop_target_renders_its_style(cx: &mut TestAppContext) {
    let (window, counts) = open(cx);
    start_drag(window, cx);
    let slice = counts.slice.get();

    mouse_move(window, 150., cx);

    assert_eq!(counts.slice.get(), slice + 1);
    assert!(counts.drag_over.get() > 0);
}

#[gpui_kit::test]
fn a_file_entering_over_a_drop_target_renders_its_style(cx: &mut TestAppContext) {
    let (window, counts) = open(cx);
    mouse_move(window, 250., cx);
    let slice = counts.slice.get();

    send(
        window,
        PlatformInput::FileDrop(FileDropEvent::Entered {
            position: point(px(250.), px(50.)),
            paths: ExternalPaths(vec![PathBuf::from("/tmp/file")].into()),
        }),
        cx,
    );

    assert_eq!(counts.slice.get(), slice + 1);
    assert!(counts.drag_over.get() > 0);
}

fn set_covered(window: AnyWindowHandle, counts: &Counts, cx: &mut TestAppContext) {
    counts.covered.set(!counts.covered.get());
    notify_root(window, cx);
    draw_what_is_dirty(window, cx);
}

fn notify_root(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |root, _, cx| cx.notify(root.entity_id()))
        .unwrap();
}

fn draw_what_is_dirty(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, _, _| ()).unwrap();
}

#[gpui_kit::test]
fn a_drop_target_covered_under_a_still_pointer_renders_again_once(cx: &mut TestAppContext) {
    let (window, counts) = open(cx);
    start_drag(window, cx);
    mouse_move(window, 150., cx);
    let slice = counts.slice.get();

    set_covered(window, &counts, cx);

    assert_eq!(counts.slice.get(), slice + 1);
    notify_root(window, cx);
    draw_what_is_dirty(window, cx);
    assert_eq!(counts.slice.get(), slice + 1, "the next frame replays");
}
