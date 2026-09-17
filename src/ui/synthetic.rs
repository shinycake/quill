use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Duration;

#[derive(Clone)]
pub enum SyntheticKind {
    Text,
    Rtl,
    Emoji,
    Image { loaded: bool },
}

#[derive(Clone)]
pub struct SyntheticRow {
    pub id: u64,
    pub sender: SharedString,
    pub body: SharedString,
    pub kind: SyntheticKind,
    pub outgoing: bool,
}

pub struct SyntheticChat {
    rows: Vec<SyntheticRow>,
    scroller: Entity<MessageScrollerState>,
    next_id: u64,
}

impl SyntheticChat {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let rows = vec![
            row(1, "Ada", "Short line.", SyntheticKind::Text, false),
            row(
                2,
                "Ada",
                "A longer paragraph used to force a mixed-height row. Quill is an independent GPUI + TDLib client, not a ZapFast fork. This text should wrap across several lines in the history pane.",
                SyntheticKind::Text,
                false,
            ),
            row(
                3,
                "نورة",
                "مرحبا — العربية مع English digits 123 ورابط https://example.com",
                SyntheticKind::Rtl,
                false,
            ),
            row(
                4,
                "You",
                "שלום עולם — Hebrew with Latin OK and emoji 👨‍👩‍👧‍👦 👍🏽",
                SyntheticKind::Emoji,
                true,
            ),
            row(
                5,
                "Ada",
                "Photo pending…",
                SyntheticKind::Image { loaded: false },
                false,
            ),
        ];
        let scroller = cx.new(|cx| {
            let mut state = MessageScrollerState::new(rows.len(), cx);
            state.scroll_to_item(0, cx);
            state
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            this.update(cx, |this, cx| this.finish_image(cx)).ok();
        })
        .detach();
        Self {
            next_id: 6,
            rows,
            scroller,
        }
    }

    pub fn send_text(&mut self, text: String, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        self.rows
            .push(row(id, "You", text, SyntheticKind::Text, true));
        self.scroller.update(cx, |state, cx| {
            state.append(1, cx);
        });
        cx.notify();
    }

    pub fn prepend_older(&mut self, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        self.rows.insert(
            0,
            row(
                id,
                "Archive",
                "Older message prepended — the visible anchor must stay put.",
                SyntheticKind::Text,
                false,
            ),
        );
        self.scroller.update(cx, |state, cx| {
            state.prepend(1, cx);
        });
        cx.notify();
    }

    fn finish_image(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| matches!(row.kind, SyntheticKind::Image { loaded: false }))
        {
            row.kind = SyntheticKind::Image { loaded: true };
            row.body = "Photo loaded (height changed).".into();
        }
        let len = self.rows.len();
        self.scroller.update(cx, |state, cx| {
            let _ = state.remeasure_items(0..len, cx);
        });
        cx.notify();
    }
}

fn row(
    id: u64,
    sender: impl Into<SharedString>,
    body: impl Into<SharedString>,
    kind: SyntheticKind,
    outgoing: bool,
) -> SyntheticRow {
    SyntheticRow {
        id,
        sender: sender.into(),
        body: body.into(),
        kind,
        outgoing,
    }
}

impl Render for SyntheticChat {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        div()
            .id("history")
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0()
            .px_3()
            .pt_2()
            .child(
                MessageScroller::new(
                    "synthetic-history",
                    self.scroller.clone(),
                    move |ix, _, _| {
                        rows.get(ix)
                            .cloned()
                            .map(message_bubble)
                            .unwrap_or_else(|| div().into_any_element())
                    },
                )
                .jump_button(false)
                .scrollbar(false)
                .size_full()
                .min_h_0(),
            )
    }
}

fn message_bubble(row: SyntheticRow) -> AnyElement {
    let image_h = match row.kind {
        SyntheticKind::Image { loaded: false } => px(40.),
        SyntheticKind::Image { loaded: true } => px(96.),
        _ => px(0.),
    };
    let rtl = matches!(row.kind, SyntheticKind::Rtl | SyntheticKind::Emoji);
    let bubble = div()
        .id(("bubble", row.id))
        .max_w(px(520.))
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(if row.outgoing {
            rgb(0x1f6feb)
        } else {
            rgb(0x2d333b)
        })
        .text_color(rgb(0xffffff))
        .when(rtl, |this| this.text_right())
        .child(div().text_xs().opacity(0.8).child(row.sender.clone()))
        .child(div().text_sm().child(row.body.clone()))
        .when(image_h > px(0.), |this| {
            this.child(
                div()
                    .mt_2()
                    .w(px(240.))
                    .h(image_h)
                    .rounded_md()
                    .bg(rgb(0x444c56))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        if matches!(row.kind, SyntheticKind::Image { loaded: true }) {
                            "image 4:3"
                        } else {
                            "loading image"
                        },
                    ),
            )
        });
    let row_el = div().id(("row", row.id)).w_full().flex().py_1();
    if row.outgoing {
        row_el.justify_end().child(bubble).into_any_element()
    } else {
        row_el.justify_start().child(bubble).into_any_element()
    }
}
