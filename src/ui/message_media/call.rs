//! A `messageCall` as Telegram Desktop draws it (`HistoryView::Call`): a
//! card `historyCallWidth` wide in an ordinary bubble, with the call's
//! title, an arrow and the time (and duration) under it, and the call
//! icon in the corner that calls back.

use super::*;
use quill::telegram::envelope::CallDiscardReason;

/// `historyCallWidth`: the call card's bubble width.
pub(in crate::ui) const CALL_WIDTH: i32 = 240;

/// `Data::MediaCall::Text`: the card's title. A missed outgoing call was
/// cancelled; a declined incoming one was declined here.
pub(in crate::ui) fn call_title(
    is_video: bool,
    reason: &CallDiscardReason,
    outgoing: bool,
) -> String {
    let what = match (outgoing, reason) {
        (true, CallDiscardReason::Missed) => "Cancelled",
        (true, _) => "Outgoing",
        (false, CallDiscardReason::Missed) => "Missed",
        (false, CallDiscardReason::Declined) => "Declined",
        (false, _) => "Incoming",
    };
    if is_video {
        format!("{what} video call")
    } else {
        format!("{what} call")
    }
}

/// A missed or declined call draws its arrow red (`historyCallArrowMissed`)
/// and shows no duration (`ComputeDuration`).
pub(in crate::ui) fn call_missed(reason: &CallDiscardReason) -> bool {
    matches!(
        reason,
        CallDiscardReason::Missed | CallDiscardReason::Declined
    )
}

/// `Ui::FormatDurationWords`: "45 seconds", "6 min 12 s".
pub(in crate::ui) fn duration_words(seconds: i32) -> String {
    if seconds > 59 {
        format!("{} min {} s", seconds / 60, seconds % 60)
    } else if seconds == 1 {
        "1 second".to_string()
    } else {
        format!("{seconds} seconds")
    }
}

/// The status line: the time, then the duration when the call connected
/// (`lng_call_duration_info`).
pub(in crate::ui) fn call_status(time: &str, reason: &CallDiscardReason, duration: i32) -> String {
    if duration > 0 && !call_missed(reason) {
        format!("{time}, {}", duration_words(duration))
    } else {
        time.to_string()
    }
}

/// The call card for a bubble. `peer` is the 1:1 chat's user, whom the
/// corner icon calls back; group chats show the icon without the action.
pub(in crate::ui) fn call_card(
    message_id: MessageId,
    outgoing: bool,
    is_video: bool,
    reason: &CallDiscardReason,
    duration: i32,
    date: i32,
    peer: Option<i64>,
    plain: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    use gpui_kit::assets::IconName;
    let row = message_id.0 as u64;
    let title = call_title(is_video, reason, outgoing);
    let time = quill::state::message_time_hhmm(date).unwrap_or_default();
    let status = call_status(&time, reason, duration);
    let missed = call_missed(reason);
    let on_fill = outgoing && !plain;
    let arrow_color: Hsla = match (missed, on_fill) {
        (true, true) => danger_soft().into(),
        (true, false) => danger().into(),
        (false, true) => text_on_fill().into(),
        (false, false) => success().into(),
    };
    let accent = bubble_accent(on_fill, cx);
    let icon = if is_video {
        IconName::Video
    } else {
        IconName::Phone
    };
    let width = crate::ui::history::bubble_width::content_for_outer(CALL_WIDTH, plain);
    let label = if is_video {
        "Video call again"
    } else {
        "Call again"
    };
    div()
        .id(("call-card", row))
        .w(width)
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .id(("call-title", row))
                        .role(gpui_kit::Role::Label)
                        .aria_label(title.clone())
                        .font_semibold()
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .child(
                            // `call_arrow_out` / `call_arrow_in`: the bundled
                            // up / down arrows turned to the diagonals.
                            Icon::new(if outgoing {
                                IconName::ArrowUp
                            } else {
                                IconName::ArrowDown
                            })
                            .rotate(gpui_kit::radians(std::f32::consts::FRAC_PI_4))
                            .size(px(14.))
                            .flex_none()
                            .text_color(arrow_color),
                        )
                        .child(div().opacity(0.7).truncate().child(status)),
                ),
        )
        .child(
            div()
                .id(("call-back", row))
                .size(px(26.))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(accent)
                .child(Icon::new(icon).size(px(20.)))
                .when_some(peer, |this, user_id| {
                    this.role(gpui_kit::Role::Button)
                        .aria_label(label)
                        .tab_index(0)
                        .cursor_pointer()
                        .hover(|style| style.bg(accent.opacity(0.12)))
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.call_again(user_id, is_video, cx);
                        }))
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{call_missed, call_status, call_title, duration_words};
    use quill::telegram::envelope::CallDiscardReason::*;

    #[test]
    fn titles_follow_telegram_desktop() {
        assert_eq!(call_title(false, &Missed, false), "Missed call");
        assert_eq!(call_title(true, &Missed, false), "Missed video call");
        assert_eq!(call_title(false, &Missed, true), "Cancelled call");
        assert_eq!(call_title(true, &HungUp, true), "Outgoing video call");
        // A declined outgoing call is still "Outgoing", with a red arrow.
        assert_eq!(call_title(false, &Declined, true), "Outgoing call");
        assert_eq!(call_title(false, &Declined, false), "Declined call");
        assert_eq!(call_title(false, &HungUp, false), "Incoming call");
        assert_eq!(call_title(false, &Disconnected, false), "Incoming call");
        assert!(call_missed(&Declined) && call_missed(&Missed) && !call_missed(&HungUp));
    }

    #[test]
    fn status_shows_the_time_and_a_connected_calls_duration() {
        assert_eq!(duration_words(1), "1 second");
        assert_eq!(duration_words(45), "45 seconds");
        assert_eq!(duration_words(372), "6 min 12 s");
        assert_eq!(call_status("16:15", &HungUp, 372), "16:15, 6 min 12 s");
        assert_eq!(call_status("16:15", &HungUp, 0), "16:15");
        assert_eq!(call_status("16:15", &Missed, 30), "16:15");
    }
}
