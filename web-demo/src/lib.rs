//! Feasibility spike: a Quill-like chat UI (gpui-kit, no TDLib) running in the
//! browser through gpui's web platform, with an autoplay tour that drives the
//! UI by injecting synthetic input events and draws a fake cursor overlay.
//! The tour stops as soon as the visitor touches the page and resumes after
//! `RESUME_IDLE_MS` of inactivity.
#![cfg(target_family = "wasm")]

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::gpui::{
    AnyWindowHandle, App, AppContext, Context, Hsla, InteractiveElement, IntoElement, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    PlatformInput, Point, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    WindowOptions, div, point, px, rgb,
};
use wasm_bindgen::prelude::*;

static FONT: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

const RESUME_IDLE_MS: f64 = 9000.0;
const SIDEBAR_W: f32 = 320.0;
const HEADER_H: f32 = 56.0;
const ROW_H: f32 = 64.0;

struct Chat {
    name: &'static str,
    preview: &'static str,
    time: &'static str,
    unread: u32,
    hue: u32,
    messages: &'static [(&'static str, bool)],
}

const CHATS: &[Chat] = &[
    Chat {
        name: "Maya Chen",
        preview: "Shipping the beta tonight",
        time: "9:41",
        unread: 2,
        hue: 0x4f8cff,
        messages: &[
            ("Did the new build go out?", false),
            ("Yes, native on macOS, Windows and Linux.", true),
            ("Shipping the beta tonight", false),
        ],
    },
    Chat {
        name: "Design Crit",
        preview: "Sam: bubbles feel great now",
        time: "9:12",
        unread: 0,
        hue: 0xb36bff,
        messages: &[
            ("Sam: bubbles feel great now", false),
            ("Thanks, the tails match Telegram's.", true),
        ],
    },
    Chat {
        name: "Rust Folks",
        preview: "wgpu 29 released",
        time: "Tue",
        unread: 14,
        hue: 0xff8a4f,
        messages: &[
            ("wgpu 29 released", false),
            ("Nice, trying it today.", true),
        ],
    },
    Chat {
        name: "Saved Messages",
        preview: "Remember to renew the cert",
        time: "Mon",
        unread: 0,
        hue: 0x32c8a0,
        messages: &[("Remember to renew the cert", true)],
    },
    Chat {
        name: "Hermes Bot",
        preview: "Your build finished",
        time: "Sun",
        unread: 0,
        hue: 0xe05a7a,
        messages: &[("Your build finished", false)],
    },
];

struct Demo {
    selected: usize,
    cursor: Option<Point<Pixels>>,
    pressed: bool,
}

fn chat_row_y(i: usize) -> f32 {
    HEADER_H + ROW_H * i as f32 + ROW_H / 2.0
}

impl Render for Demo {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = &CHATS[self.selected];
        let bg = rgb(0x17212b);
        let panel = rgb(0x0e1621);
        let text = rgb(0xf5f5f5);
        let muted = rgb(0x7f91a4);

        let rows = CHATS.iter().enumerate().map(|(i, c)| {
            let sel = i == self.selected;
            div()
                .id(("chat", i))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .h(px(ROW_H))
                .when_selected(sel)
                .hover(|s| s.bg(rgb(0x202b36)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = i;
                    cx.notify();
                }))
                .child(
                    div()
                        .size(px(46.0))
                        .rounded_full()
                        .bg(rgb(c.hue))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xffffff))
                        .child(SharedString::from(c.name[..1].to_string())),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(div().text_color(text).child(c.name))
                        .child(div().text_sm().text_color(muted).child(c.preview)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap_1()
                        .child(div().text_xs().text_color(muted).child(c.time))
                        .children((c.unread > 0).then(|| {
                            div()
                                .px_2()
                                .rounded_full()
                                .bg(rgb(0x3d9be9))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child(SharedString::from(c.unread.to_string()))
                        })),
                )
        });

        let bubbles = chat.messages.iter().map(|(t, out)| {
            let b = div()
                .max_w(px(420.0))
                .px_3()
                .py_2()
                .rounded_xl()
                .bg(if *out { rgb(0x2b5278) } else { rgb(0x182533) })
                .text_color(text)
                .child(*t);
            let row = div().flex().w_full();
            if *out {
                row.justify_end().child(b)
            } else {
                row.child(b)
            }
        });

        let mut root = div()
            .size_full()
            .flex()
            .bg(bg)
            .text_color(text)
            .font_family("Noto Sans")
            .child(
                div()
                    .w(px(SIDEBAR_W))
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(0x0b1118))
                    .child(
                        div()
                            .h(px(HEADER_H))
                            .px_4()
                            .flex()
                            .items_center()
                            .text_color(text)
                            .child("Quill"),
                    )
                    .children(rows),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(panel)
                    .child(
                        div()
                            .h(px(HEADER_H))
                            .px_4()
                            .flex()
                            .items_center()
                            .bg(bg)
                            .child(chat.name),
                    )
                    .child(
                        div()
                            .flex_1()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(bubbles),
                    )
                    .child(
                        div()
                            .m_3()
                            .px_4()
                            .h(px(44.0))
                            .flex()
                            .items_center()
                            .rounded_xl()
                            .bg(bg)
                            .text_color(muted)
                            .child("Write a message..."),
                    ),
            );

        // Fake cursor overlay: a small arrow-ish pointer drawn above everything.
        if let Some(p) = self.cursor {
            let c: Hsla = if self.pressed {
                rgb(0xffffff).into()
            } else {
                rgb(0xe8e8e8).into()
            };
            root = root.child(
                div()
                    .absolute()
                    .left(p.x)
                    .top(p.y)
                    .size(px(if self.pressed { 14.0 } else { 18.0 }))
                    .rounded_full()
                    .bg(c)
                    .border_2()
                    .border_color(rgb(0x3d9be9))
                    .shadow_md(),
            );
        }
        root
    }
}

trait WhenSel: Styled + Sized {
    fn when_selected(self, sel: bool) -> Self {
        if sel { self.bg(rgb(0x2b5278)) } else { self }
    }
}
impl WhenSel for gpui_kit::gpui::Stateful<gpui_kit::gpui::Div> {}

#[derive(Clone, Copy)]
enum Step {
    Pause(u64),
    Click(f32, f32),
}

fn tour() -> Vec<Step> {
    let x = 140.0;
    vec![
        Step::Pause(800),
        Step::Click(x, chat_row_y(1)),
        Step::Pause(1200),
        Step::Click(x, chat_row_y(2)),
        Step::Pause(1200),
        Step::Click(x, chat_row_y(3)),
        Step::Pause(1200),
        Step::Click(x, chat_row_y(0)),
        Step::Pause(1500),
    ]
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

fn ease(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn inject(cx: &mut gpui_kit::gpui::AsyncApp, win: AnyWindowHandle, ev: PlatformInput) {
    let _ = win.update(cx, |_, window, cx| {
        window.dispatch_event(ev, cx);
    });
}

fn move_ev(p: Point<Pixels>) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position: p,
        pressed_button: None,
        modifiers: Modifiers::default(),
    })
}

#[wasm_bindgen(start)]
pub fn start() {
    gpui_kit::platform::web_init();
    let last_user = Rc::new(Cell::new(f64::NEG_INFINITY));
    // Real input detection: DOM events are only produced by the visitor;
    // events injected through `Window::dispatch_event` never reach the DOM.
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        for name in [
            "pointermove",
            "pointerdown",
            "wheel",
            "keydown",
            "touchstart",
        ] {
            let flag = last_user.clone();
            let cb = Closure::<dyn FnMut()>::new(move || flag.set(now_ms()));
            let _ = doc.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref());
            cb.forget();
        }
    }

    gpui_kit::application().run(move |cx: &mut App| {
        let _ = cx.text_system().add_fonts(vec![Cow::Borrowed(FONT)]);
        gpui_kit::init(cx);
        let (handle, view) = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Demo {
                selected: 0,
                cursor: None,
                pressed: false,
            })
        })
        .expect("window");
        let win: AnyWindowHandle = handle;
        let started = now_ms();
        cx.spawn(async move |cx| {
            let mut pos = point(px(700.0), px(500.0));
            loop {
                // Wait for the visitor to be idle (also true at page load).
                while now_ms() - last_user.get() < RESUME_IDLE_MS {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                }
                let _ = started;
                let _ = view.update(cx, |d, cx| {
                    d.selected = 0;
                    d.cursor = Some(pos);
                    cx.notify();
                });
                'tour: for step in tour() {
                    let interrupted = |t0: f64| last_user.get() > t0;
                    let t0 = now_ms();
                    match step {
                        Step::Pause(ms) => {
                            let mut left = ms as i64;
                            while left > 0 {
                                if interrupted(t0) {
                                    break 'tour;
                                }
                                cx.background_executor()
                                    .timer(Duration::from_millis(50))
                                    .await;
                                left -= 50;
                            }
                        }
                        Step::Click(x, y) => {
                            let from = pos;
                            let to = point(px(x), px(y));
                            let dx = f32::from(to.x - from.x);
                            let dy = f32::from(to.y - from.y);
                            let dist = (dx * dx + dy * dy).sqrt();
                            let steps = ((dist / 900.0 * 900.0) / 16.0).clamp(20.0, 70.0) as u32;
                            for i in 1..=steps {
                                if interrupted(t0) {
                                    break 'tour;
                                }
                                let t = ease(i as f32 / steps as f32);
                                pos = point(from.x + px(dx * t), from.y + px(dy * t));
                                inject(cx, win, move_ev(pos));
                                let _ = view.update(cx, |d, cx| {
                                    d.cursor = Some(pos);
                                    cx.notify();
                                });
                                cx.background_executor()
                                    .timer(Duration::from_millis(16))
                                    .await;
                            }
                            let _ = view.update(cx, |d, cx| {
                                d.pressed = true;
                                cx.notify();
                            });
                            inject(
                                cx,
                                win,
                                PlatformInput::MouseDown(MouseDownEvent {
                                    button: MouseButton::Left,
                                    position: pos,
                                    modifiers: Modifiers::default(),
                                    click_count: 1,
                                    first_mouse: false,
                                }),
                            );
                            cx.background_executor()
                                .timer(Duration::from_millis(90))
                                .await;
                            inject(
                                cx,
                                win,
                                PlatformInput::MouseUp(MouseUpEvent {
                                    button: MouseButton::Left,
                                    position: pos,
                                    modifiers: Modifiers::default(),
                                    click_count: 1,
                                }),
                            );
                            let _ = view.update(cx, |d, cx| {
                                d.pressed = false;
                                cx.notify();
                            });
                        }
                    }
                }
                // Tour ended (or interrupted): hide the synthetic cursor.
                let _ = view.update(cx, |d, cx| {
                    d.cursor = None;
                    cx.notify();
                });
                if last_user.get() == f64::NEG_INFINITY {
                    // Finished uninterrupted: loop again after a beat.
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                } else if now_ms() - last_user.get() >= RESUME_IDLE_MS {
                    continue;
                }
            }
        })
        .detach();
        cx.activate(true);
    });
}
