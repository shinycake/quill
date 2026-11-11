//! poll message rendering.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::bubble_accent;
use super::polls::option_thumb::{PollMediaView, option_thumb};
use super::pressable::PressableDiv;
use super::synthetic::BubbleLook;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{Key, keys, tr, tr_with, trn_decimal};
use quill::local_time::now_unix;
use quill::poll::{
    PollMark, PollRow, can_offer_add_option, can_view_poll_voters, poll_ends_in_label, poll_is_rtl,
    poll_results_hidden, poll_row, poll_vote_restriction_label, quiz_explanation,
};
use quill::telegram::envelope::{PollContent, PollOption, PollType};

/// The room before an option's text: wide enough for a semibold "100%",
/// so the text starts at one place before and after voting.
const LEAD_WIDTH: Pixels = px(34.);
const LEAD_GAP: Pixels = px(6.);
const MARK_SIZE: Pixels = px(16.);
const RADIO_SIZE: Pixels = px(18.);
const BAR_HEIGHT: Pixels = px(4.);

/// A poll's colors on its bubble: the accent on an incoming one, the
/// bubble's own text color on a filled outgoing one, where the accent would
/// vanish. Text the poll does not recolor inherits from the bubble.
#[derive(Clone, Copy)]
pub(super) struct PollInk {
    accent: Hsla,
    muted: Hsla,
    on_accent: Hsla,
    correct: Hsla,
    wrong: Hsla,
}

impl PollInk {
    pub(super) fn new(on_fill: bool, look: &BubbleLook, cx: &App) -> Self {
        if on_fill {
            let accent = look.outgoing_text();
            return Self {
                accent,
                muted: accent.opacity(0.7),
                on_accent: look.outgoing_fill().into(),
                correct: accent,
                wrong: accent,
            };
        }
        Self {
            accent: bubble_accent(false, cx),
            muted: text_muted().into(),
            on_accent: gpui_kit::white(),
            correct: success().into(),
            wrong: danger().into(),
        }
    }

    fn mark(self, mark: PollMark) -> Hsla {
        match mark {
            PollMark::Correct => self.correct,
            PollMark::Wrong => self.wrong,
            PollMark::None | PollMark::Chosen => self.accent,
        }
    }
}

/// The caption under the question, as tdesktop words it.
fn kind_key(poll: &quill::telegram::Poll) -> Key {
    let quiz = matches!(poll.poll_type, PollType::Quiz { .. });
    match (poll.is_closed, quiz, poll.is_anonymous) {
        (true, ..) => keys::POLLS_CLOSED,
        (false, true, true) => keys::POLLS_ANONYMOUS_QUIZ,
        (false, true, false) => keys::POLLS_PUBLIC_QUIZ,
        (false, false, true) => keys::POLLS_ANONYMOUS,
        (false, false, false) => keys::POLLS_PUBLIC,
    }
}

/// The total under the options.
fn total_label(poll: &quill::telegram::Poll) -> String {
    let quiz = matches!(poll.poll_type, PollType::Quiz { .. });
    let total = i64::from(poll.total_voter_count);
    match (total, quiz) {
        (0, true) => tr(keys::POLLS_ANSWERS_NONE).to_string(),
        (0, false) => tr(keys::POLLS_VOTES_NONE).to_string(),
        (_, true) => trn_decimal(keys::POLLS_ANSWERS_COUNT, total),
        (_, false) => trn_decimal(keys::POLLS_VOTES_COUNT, total),
    }
}

/// Phase 4.2: `messagePoll` row: the question, a caption, one row per option
/// and the total. Before a vote a row is a circle (a square with multiple
/// answers) and the text; after it, the percentage, the text and a bar, with
/// a mark on the chosen or correct option (`Poll::Options::paintAnswer`).
/// Every row keeps to the side the first option reads from. Vote taps go through
/// `setPollAnswer` (`QuillApp::vote_on_poll`); counts refresh live via
/// `updatePoll`.
pub(super) fn poll_body(
    chat_id: ChatId,
    message_id: MessageId,
    content: &PollContent,
    // The options' pictures and custom emoji, as far as downloaded.
    media: &PollMediaView,
    // The bubble's room: a plain bubble has no padding to take off the
    // width, a narrow history lowers the floor.
    room: crate::ui::history::bubble_width::BubbleRoom,
    ink: PollInk,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let poll = &content.poll;
    let rtl = super::bidi_line::aligns_end(&poll.question);
    let caption = |text: String, color: Hsla| {
        div()
            .text_xs()
            .text_color(color)
            .when(rtl, |this| this.w_full().text_right())
            .child(text)
    };
    let mut body = div()
        .id(("poll", message_id.0 as u64))
        .flex()
        .flex_col()
        .mt_1()
        // Telegram Desktop `Poll::countOptimalSize`: never narrower than
        // `msgFileMinWidth`; the question and answers widen it.
        .min_w(super::history::bubble_width::poll_min_width(room))
        .child(
            super::bidi_line::aligned_block(poll.question.clone())
                .text_sm()
                .font_semibold(),
        );
    let mut meta = tr(kind_key(poll)).to_string();
    // B15: "Restrict to Subscribers" and the absolute deadline
    // (`lng_polls_results_in_*`-style "ends in …").
    if poll.members_only {
        meta.push_str(tr(keys::Q_MEDIA_SUBSCRIBERS_ONLY_2));
    }
    if let Some(ends) = poll_ends_in_label(poll, now_unix()) {
        meta.push_str(" · ");
        meta.push_str(&ends);
    }
    body = body.child(caption(meta, ink.muted).mt_0p5());
    if !content.description.is_empty() {
        body = body.child(
            div()
                .mt_1()
                .text_xs()
                .text_color(ink.muted)
                .child(super::bidi_line::aligned_block(content.description.clone())),
        );
    }
    // B15: "Hide results" — `lng_polls_results_after_close`.
    if poll_results_hidden(poll) {
        body = body.child(caption(tr(keys::POLLS_ENDS_TOAST).to_string(), ink.muted).mt_1());
    }
    // B4: `pollVoteRestrictionReason*` (schema `td_api.tl:494`-`:510`) —
    // a human reason instead of a dead tap.
    if let Some(reason) = &poll.vote_restriction_reason {
        body = body
            .child(caption(poll_vote_restriction_label(reason), warning_orange().into()).mt_1());
    }
    let mut options = div().flex().flex_col().mt_1();
    for (index, option) in poll.options.iter().enumerate() {
        options = options.child(poll_option_row(
            chat_id, message_id, index, option, poll, media, ink, cx,
        ));
    }
    body = body.child(options);
    // B15: "Add an Option" (`addPollOption`) when the server allows it
    // (`messagePoll.can_add_option`).
    if can_offer_add_option(content.can_add_option, poll) {
        body = body.child(
            Button::new(format!("poll-add-option-{}", message_id.0))
                .label(tr(keys::Q_MEDIA_ADD_AN_OPTION))
                .ghost()
                .text_color(ink.accent)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_poll_add_option(chat_id, message_id, window, cx);
                })),
        );
    }
    // B4: quiz explanation (`pollTypeQuiz.explanation`, schema line
    // 475-476) — auto-shown on an incorrect answer, per TGX; no lamp
    // affordance for correct answers yet (documented deviation).
    if let Some(explanation) = quiz_explanation(poll) {
        body = body.child(
            div()
                .mt_1()
                .text_xs()
                .child(super::bidi_line::aligned_block(format!("💡 {explanation}"))),
        );
    }
    // B4: voter list (`getPollVoters`, schema line 12941): where the server
    // has voters to show, the total opens them, as tdesktop's bottom button.
    body = if can_view_poll_voters(poll) {
        body.child(
            Button::new(format!("poll-voters-{}", message_id.0))
                .label(tr_with(
                    keys::Q_MEDIA_VIEW_VOTES,
                    &[("total_voter_count", &poll.total_voter_count.to_string())],
                ))
                .ghost()
                .text_color(ink.accent)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_poll_voters_dialog(chat_id, message_id, cx);
                })),
        )
    } else {
        body.child(
            div()
                .mt_1()
                .w_full()
                .text_center()
                .text_xs()
                .text_color(ink.muted)
                .child(total_label(poll)),
        )
    };
    body.into_any_element()
}

/// The poll row of a history message: the options' pictures and custom emoji
/// come from the files the session has.
pub(super) fn session_poll_body(
    message: &quill::state::HistoryMessage,
    content: &PollContent,
    (files, media_roots, session): (
        &quill::state::FileMap,
        &[std::path::PathBuf],
        Option<&quill::state::Session>,
    ),
    room: crate::ui::history::bubble_width::BubbleRoom,
    ink: PollInk,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let media =
        super::polls::option_thumb::poll_media_view(&content.poll, files, media_roots, session);
    poll_body(message.chat_id, message.id, content, &media, room, ink, cx)
}

/// A filled disc (a rounded square with multiple answers) holding `icon`.
fn filled_mark(size: Pixels, multiple: bool, fill: Hsla, icon: IconName, ink: PollInk) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(size)
        .map(|this| {
            if multiple {
                this.rounded(px(4.))
            } else {
                this.rounded_full()
            }
        })
        .bg(fill)
        .text_color(ink.on_accent)
        .child(Icon::new(icon).with_size(px(12.)))
}

/// What sits before the text while there are no numbers to show.
fn choice_mark(selected: bool, multiple: bool, ink: PollInk) -> Div {
    if selected {
        return filled_mark(RADIO_SIZE, multiple, ink.accent, IconName::Check, ink);
    }
    div()
        .flex_none()
        .size(RADIO_SIZE)
        .map(|this| {
            if multiple {
                this.rounded(px(4.))
            } else {
                this.rounded_full()
            }
        })
        .border(px(1.5))
        .border_color(ink.muted)
}

/// The percentage over its mark, in the column before the text.
fn result_lead(percent: i32, mark: PollMark, multiple: bool, ink: PollInk) -> Vec<AnyElement> {
    let percent = div()
        .text_xs()
        .font_semibold()
        .mt(px(2.))
        .whitespace_nowrap()
        .child(format!("{percent}%"));
    let icon = match mark {
        PollMark::None => None,
        PollMark::Wrong => Some(IconName::Close),
        PollMark::Chosen | PollMark::Correct => Some(IconName::Check),
    };
    let mark = icon.map(|icon| filled_mark(MARK_SIZE, multiple, ink.mark(mark), icon, ink));
    std::iter::once(percent.into_any_element())
        .chain(mark.map(IntoElement::into_any_element))
        .collect()
}

/// The result bar: a tinted track with the filled part from the option's
/// starting edge. The line is as tall as the mark, so the two line up.
fn result_bar(fill: f32, color: Hsla, rtl: bool) -> Div {
    div().flex().flex_none().items_center().h(MARK_SIZE).child(
        div()
            .flex()
            .when(rtl, |this| this.flex_row_reverse())
            .w_full()
            .h(BAR_HEIGHT)
            .rounded(BAR_HEIGHT / 2.)
            .bg(color.opacity(0.15))
            .child(
                div()
                    .h_full()
                    .w(relative(fill))
                    .min_w(BAR_HEIGHT)
                    .rounded(BAR_HEIGHT / 2.)
                    .bg(color),
            ),
    )
}

/// One poll option, laid out from the poll's side. Tappable
/// while the poll is open.
pub(super) fn poll_option_row(
    chat_id: ChatId,
    message_id: MessageId,
    index: usize,
    option: &PollOption,
    poll: &quill::telegram::Poll,
    media: &PollMediaView,
    ink: PollInk,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let state = poll_row(poll, index, option);
    let rtl = poll_is_rtl(poll);
    let multiple = poll.allows_multiple_answers;
    let correct = matches!(
        state,
        PollRow::Result {
            mark: PollMark::Correct,
            ..
        }
    );
    let label: AnyElement = if option.entities.is_empty() {
        super::bidi_line::aligned_block_to(option.text.clone(), rtl).into_any_element()
    } else {
        // Custom emoji: drawn through the message text, whose entity
        // offsets need the text as it came.
        let key = (message_id.0, index as u64);
        div()
            .flex()
            .flex_wrap()
            .when(rtl, |this| this.justify_end())
            .child(super::message_text::rich_text_line(
                &option.text,
                &option.entities,
                (
                    chat_id.0,
                    (key.0 as u64).wrapping_mul(64).wrapping_add(key.1),
                ),
                false,
                &Default::default(),
                px(14.),
                &media.emoji,
                cx,
            ))
            .into_any_element()
    };
    let thumb = media
        .thumb(index)
        .map(|thumb| option_thumb(chat_id, message_id, index, thumb, cx));
    let text = div()
        .flex()
        .when(rtl, |this| this.flex_row_reverse())
        .items_center()
        .gap_2()
        .text_sm()
        .child(div().flex_1().min_w_0().child(label))
        .children(thumb);
    let lead = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(LEAD_WIDTH)
        .map(|this| match state {
            PollRow::Choice { selected } => this
                .when(rtl, |this| this.items_end())
                .pt(px(1.))
                .child(choice_mark(selected, multiple, ink)),
            PollRow::Result { percent, mark, .. } => this
                .justify_between()
                .when(!rtl, |this| this.items_end())
                .children(result_lead(percent, mark, multiple, ink)),
        });
    let content = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(text)
        .when_some(
            match state {
                PollRow::Result { fill, mark, .. } => Some((fill, mark)),
                PollRow::Choice { .. } => None,
            },
            |this, (fill, mark)| this.child(result_bar(fill, ink.mark(mark), rtl)),
        );
    let mut row = div()
        .id((
            "poll-option",
            (message_id.0 as u64)
                .wrapping_mul(64)
                .wrapping_add(index as u64),
        ))
        .flex()
        .when(rtl, |this| this.flex_row_reverse())
        .gap(LEAD_GAP)
        .mx(px(-6.))
        .px(px(6.))
        .py(px(5.))
        .rounded_md()
        .child(lead)
        .child(content);
    if correct {
        row = row.role(gpui_kit::Role::Label).aria_label(tr_with(
            keys::Q_MEDIA_CORRECT_ANSWER,
            &[("option_label", &option.text)],
        ));
    }
    // A right-press notes which option it is over; the message menu opened
    // by the bubble behind it then offers the option's own rows.
    let option_id = option.id.clone();
    row = row.on_mouse_down(
        MouseButton::Right,
        cx.listener(move |this, event: &MouseDownEvent, _, _| {
            this.note_right_clicked_item(
                event.position,
                quill::composer::ReplyItem::PollOption(option_id.clone()),
            );
        }),
    );
    if poll.can_vote() {
        let vote_option_id = option.id.clone();
        row = row
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_MEDIA_VOTE_FOR_OPTION,
                &[("index_1", &(index + 1).to_string())],
            ))
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.vote_on_poll(chat_id, message_id, &vote_option_id, cx);
            }));
    }
    row.test_support().into_any_element()
}
