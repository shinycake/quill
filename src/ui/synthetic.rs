#![deny(clippy::indexing_slicing)]
use super::chat_theme::*;
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
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
            row(
                1,
                tr(keys::Q_MISC_ADA),
                tr(keys::Q_MISC_SHORT_LINE),
                SyntheticKind::Text,
                false,
            ),
            row(
                2,
                tr(keys::Q_MISC_ADA),
                tr(keys::Q_MISC_A_LONGER_PARAGRAPH_USED_TO),
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
                tr(keys::FROM_YOU),
                tr(keys::Q_MISC_HEBREW_WITH_LATIN_OK_AND),
                SyntheticKind::Emoji,
                true,
            ),
            row(
                5,
                tr(keys::Q_MISC_ADA),
                tr(keys::Q_MISC_PHOTO_PENDING),
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
            .push(row(id, tr(keys::FROM_YOU), text, SyntheticKind::Text, true));
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
                tr(keys::Q_MISC_ARCHIVE),
                tr(keys::Q_MISC_OLDER_MESSAGE_PREPENDED_THE_VISIBLE),
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
            row.body = tr(keys::Q_MISC_PHOTO_LOADED_HEIGHT_CHANGED).into();
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
    /// The bubble continues the previous bubble of the same sender
    /// (Telegram Desktop's grouping): its top corner on the avatar side is
    /// small.
    pub joined_above: bool,
    /// The chat's emoji theme: fill of outgoing bubbles.
    pub out_fill: Option<super::bubble_theme::OutgoingFill>,
    /// "Adaptive layout for wide screens" is in effect (`chat_wide`):
    /// outgoing bubbles sit on the left like incoming ones.
    pub wide: bool,
    /// The history's width at the last paint: a narrow history narrows
    /// the bubbles (`bubble_width::BubbleRoom`).
    pub history: quill::bubble_layout::HistoryWidth,
}

impl BubbleLook {
    /// Fill of an outgoing bubble: the chat theme's (its mean when it is
    /// a gradient), else the accent.
    pub(crate) fn outgoing_fill(&self) -> Rgba {
        super::bubble_theme::outgoing_bubble_color(self.out_fill)
    }

    /// Text on an outgoing bubble: white, unless a light theme fill needs
    /// dark text.
    pub(crate) fn outgoing_text(&self) -> Hsla {
        self.out_fill
            .map_or_else(|| Hsla::from(text_on_fill()), |fill| fill.text())
    }
}

/// Radius of a corner that joins a neighbouring bubble or carries the
/// bubble's tail (tdesktop `bubbleRadiusSmall`).
const JOINED_RADIUS: f32 = 5.0;

impl BubbleLook {
    /// Pre-login synthetic placeholder: today's fixed look.
    pub(crate) fn demo() -> Self {
        Self {
            font: px(14.),
            plain: false,
            text: Hsla::from(rgb(0xffffff)),
            joined_above: false,
            out_fill: None,
            wide: false,
            history: quill::bubble_layout::HistoryWidth::WIDE,
        }
    }
}

/// Grouped corners: on the side facing the sender's avatar the bottom
/// corner is small (the tail of the last bubble, the join of the others)
/// and so is the top corner when the bubble continues the one above.
fn bubble_corners(
    content: component::bubble::BubbleContent,
    right: bool,
    look: BubbleLook,
) -> component::bubble::BubbleContent {
    if look.plain {
        return content;
    }
    let small = px(JOINED_RADIUS);
    if right {
        content
            .when(look.joined_above, |this| this.rounded_tr(small))
            .rounded_br(small)
    } else {
        content
            .when(look.joined_above, |this| this.rounded_tl(small))
            .rounded_bl(small)
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
    /// The sender's name color (groups); `None` keeps the default text.
    pub sender_color: Option<Hsla>,
    /// The Premium status or star after the sender's name
    /// (`status_emoji::sender_status_element`).
    pub sender_status: Option<AnyElement>,
    /// Incoming sender avatar → kit `Message` avatar slot. `None` where the
    /// sender can't be identified (groups) — never invented.
    pub avatar: Option<AnyElement>,
    /// `HH:MM` + delivery checkmarks, painted inside the bubble at its
    /// bottom-right corner. `None` hides the footer.
    pub footer: Option<AnyElement>,
    /// The body already ends with trailing space reserved for the footer
    /// (`rich_text_reserving`), so the footer overlays the last text line
    /// instead of taking a line of its own.
    pub footer_inline: bool,
    /// The bubble ends with a picture: paint the footer on it as a pill.
    pub footer_overlay: bool,
    /// Builds `footer` again: an animation layer drawing a video under the
    /// pill redraws the pill above it (`anim_layer::mirror`).
    pub footer_rebuild: Option<std::rc::Rc<dyn Fn() -> Option<AnyElement>>>,
    /// Media-led bubble: thin inset instead of text padding.
    pub media_led: bool,
    /// The media stands on its own (`quill::media_bubble`): no fill,
    /// border or gradient, and the media carries its own time pill
    /// (`media_footer_pill`).
    pub bubbleless: bool,
    /// Hover-revealed control in the bubble's top-right corner (message
    /// actions). Right-click opens the same menu.
    pub actions: Option<AnyElement>,
    /// Room the actions take beside the bubble.
    pub actions_span: Pixels,
    /// Full-width bar under the footer (the comments / replies bar).
    pub bottom_bar: Option<AnyElement>,
    /// Content width of the picture / video / GIF / album that leads the
    /// bubble. The media decides the bubble's width (Telegram Desktop's
    /// `Photo::countCurrentSize`): captions, reactions and the footer wrap
    /// to it instead of widening the bubble.
    pub media_width: Option<Pixels>,
    /// The bubble's outer width limit when a monospace block needs more
    /// than `msgMaxWidth` (`bubble_width::bubble_width_limit`).
    pub max_width: Option<Pixels>,
}

/// The time on a picture: a dark pill in the bottom-right corner of the
/// nearest positioned parent (the bubble, or a bubble-less media block).
pub(crate) fn media_footer_pill(
    footer: AnyElement,
    rebuild: Option<std::rc::Rc<dyn Fn() -> Option<AnyElement>>>,
) -> AnyElement {
    let pill = |footer: AnyElement| {
        div()
            .px_1p5()
            .rounded_full()
            .bg(gpui_kit::black().opacity(0.45))
            .text_color(gpui_kit::white())
            .child(footer)
    };
    let pill_at = |footer| pill(footer).absolute().right(px(10.)).bottom(px(10.));
    match rebuild {
        // Over an inline video the history's animation layer draws: it
        // draws the pill again, above the video.
        Some(rebuild) => super::anim_layer::mirror(pill_at(footer), move || {
            pill(rebuild().unwrap_or_else(|| Empty.into_any_element())).into_any_element()
        })
        .into_any_element(),
        None => pill_at(footer).into_any_element(),
    }
}

/// Width the time/receipt footer needs inside a bubble (`21:44 ✓✓` at
/// `text_xs` plus breathing room). Outgoing rows carry delivery marks.
pub(crate) fn footer_reserve(outgoing: bool) -> Pixels {
    if outgoing { px(62.) } else { px(46.) }
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
        sender_color,
        sender_status,
        avatar,
        footer,
        footer_inline,
        footer_overlay,
        footer_rebuild,
        media_led,
        bubbleless,
        actions,
        actions_span,
        bottom_bar,
        media_width,
        max_width,
    } = chrome;
    let has_sender_status = sender_status.is_some();
    let room =
        super::history::bubble_width::BubbleRoom::new(look.history, avatar.is_some(), look.plain);
    let group: SharedString = format!("message-row-{}", row.id).into();
    // tdesktop `Element::hasRightLayout`: the wide layout keeps outgoing
    // messages on the left, tail and all; only their colors differ.
    let right = quill::chat_wide::has_right_layout(row.outgoing, look.wide);
    let alignment = if right {
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
    // A theme with several outgoing colors: the bubble shows its slice of
    // the gradient laid over the message area.
    let gradient = look
        .out_fill
        .filter(|_| row.outgoing && !look.plain && !bubbleless)
        .and_then(|fill| super::bubble_theme::gradient_layer(fill, look.joined_above));
    let bubble_content = component::bubble::BubbleContent::new()
        .relative()
        .children(gradient)
        // Where a delete's dust starts.
        .child(super::vanish::bubble_tracker(row.id as i64))
        .when(!look.plain && !bubbleless, |this| {
            this.bg(if row.outgoing {
                look.outgoing_fill()
            } else {
                bg_bubble_incoming()
            })
        })
        .when(bubbleless, |this| {
            this.bg(gpui_kit::transparent_black()).border_0()
        })
        .text_color(if look.plain {
            look.text
        } else if row.outgoing {
            look.outgoing_text()
        } else {
            Hsla::from(text_bright())
        })
        .when(rtl, |this| this.text_right())
        .map(|this| bubble_corners(this, right, look))
        // Telegram Desktop names a group sender inside the bubble, on its
        // first line, in the sender's color. Bubble-less rows (stickers,
        // round videos, big emoji) keep the name above instead.
        .when_some(sender.clone().filter(|_| !look.plain), |this, sender| {
            this.child(
                div()
                    .id("sender")
                    .role(Role::Label)
                    .aria_label(sender.clone())
                    .when(media_led, |this| this.px_2().pt_1())
                    .pb_0p5()
                    .text_size(look.font * 0.93)
                    .font_weight(FontWeight::SEMIBOLD)
                    .when_some(sender_color, |this, color| this.text_color(color))
                    .when_some(sender_status, |this, status| {
                        // The status sits right after the name, which
                        // shortens first (`Message::paintFromName`).
                        this.flex()
                            .items_center()
                            .gap_1()
                            .child(div().min_w_0().truncate().child(sender.clone()))
                            .child(status)
                    })
                    .when(!has_sender_status, |this| this.truncate().child(sender)),
            )
        })
        .when_some(quote, |this, quote| this.child(quote))
        .when_some(body_el, |this, body_el| this.child(body_el))
        .when(!rich_body && has_body, |this| {
            this.child(
                div()
                    .id("body")
                    .role(Role::Label)
                    .aria_label(body.clone())
                    .text_size(look.font)
                    .child(body),
            )
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
                            tr(keys::Q_MISC_IMAGE_4_3)
                        } else {
                            tr(keys::Q_MISC_LOADING_IMAGE)
                        },
                    ),
            )
        })
        .when_some(extra, |this, el| this.child(el))
        .relative()
        // Telegram Desktop `msgMaxWidth`: the widest a bubble gets, less in
        // a narrow history (`BubbleRoom`). A wide monospace block lifts it
        // (`monospaceMaxWidth`); the kit bubble's 80% share caps that one.
        .max_w(max_width.unwrap_or(px(room.outer() as f32)))
        .line_height(relative(1.4))
        .when(media_led && !look.plain, |this| this.p_0())
        .when_some(media_width, |this, width| {
            this.w(super::message_media::bubble_outer_width(
                width,
                media_led,
                look.plain || bubbleless,
            ))
        })
        .map(|this| match footer {
            Some(footer) if footer_overlay => this.child(media_footer_pill(footer, footer_rebuild)),
            Some(footer) if footer_inline => {
                this.child(div().absolute().right(px(12.)).bottom(px(7.)).child(footer))
            }
            Some(footer) => this.child(div().flex().justify_end().mt_0p5().child(footer)),
            None => this,
        })
        .when_some(bottom_bar, |this, bar| {
            this.child(div().when(media_led, |bar| bar.px_2()).child(bar))
        });
    let mut bubble = component::bubble::Bubble::new()
        .alignment(alignment)
        .content(bubble_content);
    // Hover-revealed actions anchored just outside the bubble's side edge
    // (right of incoming, left of outgoing), so they never cover its text
    // or time. The kit's reaction region is the bubble-anchored slot that
    // isn't clipped by the surface; restyle it to a bare container.
    if let Some(actions) = actions {
        let (side_alignment, outside) = if right {
            (component::message::MessageAlignment::Start, true)
        } else {
            (component::message::MessageAlignment::End, false)
        };
        let region = component::bubble::BubbleReactions::new()
            .side(component::bubble::BubbleReactionSide::Top)
            .alignment(side_alignment)
            .top(px(2.))
            .p_0()
            .border_0()
            .bg(gpui_kit::transparent_black())
            .map(|this| {
                if outside {
                    this.left(-actions_span)
                } else {
                    this.right(-actions_span)
                }
            })
            .child(
                div()
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(actions),
            );
        bubble = bubble.reactions(region);
    }
    if look.plain {
        bubble = bubble.with_variant(component::bubble::BubbleVariant::Ghost);
    }
    let mut message = component::message::Message::new()
        .alignment(alignment)
        .content(component::message::MessageContent::new().bubble(bubble));
    if let Some(sender) = sender.filter(|_| look.plain) {
        message = message.header(
            component::message::MessageHeader::new().child(
                div()
                    .id("sender")
                    .role(Role::Label)
                    .aria_label(sender.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .when_some(sender_color, |this, color| this.text_color(color))
                    .child(sender)
                    .into_any_element(),
            ),
        );
    }
    if let Some(avatar) = avatar {
        // Transparent slot: real avatars paint their own background, and
        // the in-run spacer must stay invisible.
        message = message.avatar_slot(
            component::message::MessageAvatar::new()
                .bg(gpui_kit::transparent_black())
                .child(avatar),
        );
    }
    div()
        .id(("row", row.id))
        .group(group)
        .w_full()
        .py_0p5()
        .child(message)
        .into_any_element()
}
