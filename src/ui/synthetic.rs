use super::chat_theme::*;
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

/// Settings → Appearance: bubble look for history message rows (font
/// size + bubble/plain style). Bubble fill colors stay fixed
/// (Telegram-style colored bubbles); `text` is white in bubble mode and
/// the theme foreground in plain mode (computed at the call site, which
/// owns `cx`).
#[derive(Clone, Copy)]
pub(crate) struct BubbleLook {
    pub font: Pixels,
    pub plain: bool,
    pub text: Hsla,
}

impl BubbleLook {
    /// Pre-login synthetic placeholder: today's fixed look.
    pub(crate) fn demo() -> Self {
        Self {
            font: px(14.),
            plain: false,
            text: Hsla::from(rgb(0xffffff)),
        }
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

/// kit Phase 4: optional per-message chrome for the shared kit shell.
/// Bundled so the bubble helpers keep one extra parameter instead of three.
#[derive(Default)]
pub(crate) struct MessageChrome {
    /// Incoming sender name → kit `MessageHeader` (above the bubble).
    /// `None` renders no header (outgoing messages never had one).
    pub sender: Option<SharedString>,
    /// Incoming sender avatar → kit `Message` avatar slot. `None` where the
    /// sender can't be identified (groups) — never invented.
    pub avatar: Option<AnyElement>,
    /// `HH:MM` + delivery checkmarks → kit `MessageFooter` (below the
    /// bubble, right-aligned). `None` hides the footer.
    pub footer: Option<AnyElement>,
}

pub(crate) fn session_bubble_quoted(
    id: u64,
    chrome: MessageChrome,
    body: impl Into<SharedString>,
    outgoing: bool,
    extra: Option<AnyElement>,
    quote: Option<AnyElement>,
    look: BubbleLook,
) -> AnyElement {
    message_bubble_with_quote(
        row(
            id,
            chrome.sender.clone().unwrap_or_default(),
            body,
            SyntheticKind::Text,
            outgoing,
        ),
        extra,
        quote,
        None,
        chrome,
        look,
    )
}

pub(crate) fn session_bubble_rich(
    id: u64,
    chrome: MessageChrome,
    outgoing: bool,
    body: AnyElement,
    extra: Option<AnyElement>,
    quote: Option<AnyElement>,
    look: BubbleLook,
) -> AnyElement {
    message_bubble_with_quote(
        row(
            id,
            chrome.sender.clone().unwrap_or_default(),
            "",
            SyntheticKind::Text,
            outgoing,
        ),
        extra,
        quote,
        Some(body),
        chrome,
        look,
    )
}

fn message_bubble(row: SyntheticRow) -> AnyElement {
    let chrome = MessageChrome {
        sender: Some(row.sender.clone()),
        ..Default::default()
    };
    message_bubble_with_quote(row, None, None, None, chrome, BubbleLook::demo())
}

fn message_bubble_with_quote(
    row: SyntheticRow,
    extra: Option<AnyElement>,
    quote: Option<AnyElement>,
    body_el: Option<AnyElement>,
    chrome: MessageChrome,
    look: BubbleLook,
) -> AnyElement {
    let image_h = match row.kind {
        SyntheticKind::Image { loaded: false } => px(40.),
        SyntheticKind::Image { loaded: true } => px(96.),
        _ => px(0.),
    };
    let rtl = matches!(row.kind, SyntheticKind::Rtl | SyntheticKind::Emoji);
    let body = row.body.clone();
    let has_body = !body.is_empty();
    let rich_body = body_el.is_some();
    let MessageChrome {
        sender,
        avatar,
        footer,
    } = chrome;
    let alignment = if row.outgoing {
        component::message::MessageAlignment::End
    } else {
        component::message::MessageAlignment::Start
    };
    // kit Phase 4: the bubble surface keeps the Phase 0 palette
    // (`accent_strong()` / `bg_bubble_incoming()`); the kit only supplies
    // the shape/chrome, never the colors. Sender and footer live in the kit
    // `MessageHeader` / `MessageFooter` slots — the header above the bubble,
    // the footer below it, right-aligned like the old in-bubble timestamp.
    // Settings → Appearance: font size + bubble/plain style ride on top —
    // `look.text` overrides the text color; plain mode renders the kit
    // `Ghost` variant (no surface, padding, or border).
    let bubble_content = component::bubble::BubbleContent::new()
        .when(!look.plain, |this| {
            this.bg(if row.outgoing {
                accent_strong()
            } else {
                bg_bubble_incoming()
            })
        })
        .text_color(if look.plain {
            look.text
        } else if row.outgoing {
            Hsla::from(text_on_fill())
        } else {
            Hsla::from(text_bright())
        })
        .when(rtl, |this| this.text_right())
        .when_some(quote, |this, quote| this.child(quote))
        .when_some(body_el, |this, body_el| this.child(body_el))
        .when(!rich_body && has_body, |this| {
            this.child(div().text_size(look.font).child(body))
        })
        .when(image_h > px(0.), |this| {
            this.child(
                div()
                    .mt_2()
                    .w(px(240.))
                    .h(image_h)
                    .rounded_md()
                    .bg(fill_muted())
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
        })
        .when_some(extra, |this, el| this.child(el));
    let mut bubble = component::bubble::Bubble::new()
        .alignment(alignment)
        .content(bubble_content);
    if look.plain {
        bubble = bubble.with_variant(component::bubble::BubbleVariant::Ghost);
    }
    let mut message = component::message::Message::new()
        .alignment(alignment)
        .content(component::message::MessageContent::new().bubble(bubble));
    if let Some(sender) = sender {
        message = message.header(
            component::message::MessageHeader::new().child(div().child(sender).into_any_element()),
        );
    }
    if let Some(avatar) = avatar {
        message = message.avatar(avatar);
    }
    if let Some(footer) = footer {
        message = message.footer(
            component::message::MessageFooter::new()
                .justify_end()
                .child(footer),
        );
    }
    div()
        .id(("row", row.id))
        .w_full()
        .py_1()
        .child(message)
        .into_any_element()
}
