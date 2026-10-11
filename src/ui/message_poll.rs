//! poll message rendering.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::local_time::now_unix;
use quill::poll::{
    can_offer_add_option, can_view_poll_voters, poll_bar_fraction, poll_ends_in_label,
    poll_results_hidden, poll_vote_restriction_label, quiz_explanation, voter_count_label,
};
use quill::telegram::envelope::{PollContent, PollOption, PollType};
/// Phase 4.2: `messagePoll` row — the question, one tappable option row per
/// option with a percentage bar and voter count, chosen option(s) marked;
/// closed polls render results without voting affordance. Vote taps go
/// through `setPollAnswer` (`QuillApp::vote_on_poll`); counts refresh live
/// via `updatePoll`.
pub(super) fn poll_body(
    chat_id: ChatId,
    message_id: MessageId,
    content: &PollContent,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let poll = &content.poll;
    let kind_label = match &poll.poll_type {
        PollType::Quiz { .. } => "Quiz",
        PollType::Regular => "Poll",
    };
    let mut body = div()
        .id(("poll", message_id.0 as u64))
        .flex()
        .flex_col()
        .gap_1()
        .mt_1()
        .child(
            super::bidi_line::aligned_block(poll.question.clone())
                .text_sm()
                .font_semibold(),
        );
    if !content.description.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(super::bidi_line::aligned_block(content.description.clone())),
        );
    }
    let mut meta = format!(
        "{} · {} · {}",
        kind_label,
        voter_count_label(poll.total_voter_count),
        if poll.is_closed {
            "closed"
        } else if poll.is_anonymous {
            "anonymous"
        } else {
            "public"
        },
    );
    // B15: "Restrict to Subscribers" and the absolute deadline
    // (`lng_polls_results_in_*`-style "ends in …").
    if poll.members_only {
        meta.push_str(" · subscribers only");
    }
    if let Some(ends) = poll_ends_in_label(poll, now_unix()) {
        meta.push_str(" · ");
        meta.push_str(&ends);
    }
    body = body.child(div().text_xs().text_color(text_muted()).child(meta));
    // B15: "Hide results" — `lng_polls_results_after_close`.
    let results_hidden = poll_results_hidden(poll);
    if results_hidden {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child("Results will appear after the poll ends."),
        );
    }
    // B4: `pollVoteRestrictionReason*` (schema `td_api.tl:494`-`:510`) —
    // a human reason instead of a dead tap.
    if let Some(reason) = &poll.vote_restriction_reason {
        body = body.child(
            div()
                .text_xs()
                .text_color(warning_orange())
                .child(poll_vote_restriction_label(reason)),
        );
    }
    for (index, option) in poll.options.iter().enumerate() {
        body = body.child(poll_option_row(
            chat_id,
            message_id,
            index,
            option,
            poll,
            results_hidden,
            cx,
        ));
    }
    // B15: "Add an Option" (`addPollOption`) when the server allows it
    // (`messagePoll.can_add_option`).
    if can_offer_add_option(content.can_add_option, poll) {
        body = body.child(
            Button::new(format!("poll-add-option-{}", message_id.0))
                .label("Add an Option")
                .ghost()
                .text_color(accent())
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
                .text_xs()
                .text_color(warning())
                .child(super::bidi_line::aligned_block(format!("💡 {explanation}"))),
        );
    }
    // B4: voter list (`getPollVoters`, schema line 12941) — a single
    // affordance opens the per-option voters dialog; only when the
    // server says voters are available (`can_get_voters`).
    if can_view_poll_voters(poll) {
        body = body.child(
            Button::new(format!("poll-voters-{}", message_id.0))
                .label(format!("View Votes ({})", poll.total_voter_count))
                .ghost()
                .text_color(accent())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_poll_voters_dialog(chat_id, message_id, cx);
                })),
        );
    }
    body.into_any_element()
}

/// One poll option: text + percentage, a bar split by `flex_grow`
/// (no percentage widths in GPUI), the per-option voter count, and the
/// chosen / quiz-correct marks. Tappable while the poll is open.
pub(super) fn poll_option_row(
    chat_id: ChatId,
    message_id: MessageId,
    index: usize,
    option: &PollOption,
    poll: &quill::telegram::Poll,
    results_hidden: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let fill = if results_hidden {
        0.0
    } else {
        poll_bar_fraction(option.vote_percentage)
    };
    let chosen = option.is_chosen;
    let votable = poll.can_vote();
    let quiz_correct = poll.is_closed
        && matches!(&poll.poll_type, PollType::Quiz { correct_option_ids, .. }
            if correct_option_ids.contains(&(index as i32)));
    let option_label = if chosen {
        format!("✓ {}", option.text)
    } else {
        option.text.clone()
    };
    let option_label = if quiz_correct {
        format!("{option_label} · correct answer")
    } else {
        option_label
    };
    let stats = if results_hidden {
        String::new()
    } else if option.voter_count > 0 {
        format!(
            "{}% · {} {}",
            option.vote_percentage,
            option.voter_count,
            if option.voter_count == 1 {
                "vote"
            } else {
                "votes"
            },
        )
    } else {
        format!("{}%", option.vote_percentage)
    };
    let mut row = div()
        .id(("poll-option", message_id.0 as u64 * 64 + index as u64))
        .flex()
        .flex_col()
        .gap_0p5()
        .px_3()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(if chosen { accent() } else { border() })
        .bg(bg_canvas())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .text_color(if quiz_correct { success() } else { text_menu() })
                        .flex_1()
                        .min_w_0()
                        .child(super::bidi_line::aligned_block(option_label)),
                )
                .child(div().text_sm().text_color(text_muted()).child(stats)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .h(px(6.))
                .rounded_md()
                .overflow_hidden()
                .bg(bg_deep())
                .child(
                    div()
                        .flex_grow(fill)
                        .bg(if chosen { accent_strong() } else { border() }),
                )
                .child(div().flex_grow(1.0 - fill)),
        );
    if votable {
        row = row
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Vote for option {}", index + 1))
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.vote_on_poll(chat_id, message_id, index, cx);
            }));
    }
    row.into_any_element()
}
