//! Methods moved out of `chat_row.rs` to keep files under 1000 lines.

use super::*;

/// The active-video-chat badge on a group or channel avatar, bottom-right
/// and ringed in the sidebar color like the online dot (tdesktop paints a
/// speaking indicator there, `dialogs_row.cpp` `paintCornerBadge`).
pub(in crate::ui) fn with_call_badge(avatar: AnyElement, active: bool, cx: &App) -> AnyElement {
    if !active {
        return avatar;
    }
    const BADGE: f32 = 18.;
    const RING: f32 = 2.;
    div()
        .relative()
        .flex_none()
        .child(avatar)
        .child(
            div()
                .absolute()
                .right(px(-1.))
                .bottom(px(-1.))
                .size(px(BADGE))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(cx.theme().sidebar)
                .child(
                    div()
                        .size(px(BADGE - 2. * RING))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(cx.theme().success)
                        .child(
                            Icon::new(IconName::Mic)
                                .size(px(BADGE - 2. * RING - 3.))
                                .text_color(gpui_kit::white()),
                        ),
                ),
        )
        .into_any_element()
}

/// Small inline icon in a chat row (lock, muted bell, pin, receipts).
pub(super) fn row_glyph(name: IconName, color: Hsla) -> impl IntoElement {
    Icon::new(name).size(px(14.)).flex_none().text_color(color)
}

/// Whether the chat is pinned in the list this row belongs to.
pub(super) fn pinned_here(chat: &ChatSummary, archived: bool) -> bool {
    if archived {
        chat.archive_is_pinned
    } else {
        chat.is_pinned
    }
}

/// Trailing title-line stamp: `(status mark for an outgoing last
/// message, local time/day label)`, or `None` for an empty chat.
pub(super) fn row_stamp(chat: &ChatSummary) -> Option<(RowStatus, String)> {
    let last = chat.last_message.filter(|last| last.date > 0)?;
    let now = quill::local_time::civil_local(quill::local_time::now_unix());
    let date = quill::local_time::civil_local(i64::from(last.date));
    Some((
        chat.row_status(),
        quill::local_time::chat_list_stamp(&date, &now),
    ))
}

/// The mark beside the date: a clock while sending, a red "!" when the
/// send failed, one check when delivered, two when read.
pub(super) fn row_status_mark(status: RowStatus, cx: &App) -> AnyElement {
    match status {
        RowStatus::None => div().into_any_element(),
        RowStatus::Sending => {
            row_glyph(IconName::Clock, cx.theme().muted_foreground).into_any_element()
        }
        RowStatus::Failed => div()
            .id("row-send-failed")
            .flex_none()
            .size(px(14.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().danger)
            .text_color(text_on_fill())
            .text_size(px(10.))
            .line_height(px(14.))
            .font_bold()
            .aria_label("Failed to send")
            .child("!")
            .into_any_element(),
        RowStatus::Sent => row_glyph(IconName::Check, accent().into()).into_any_element(),
        RowStatus::Read => row_glyph(IconName::CheckCheck, accent().into()).into_any_element(),
    }
}

/// `(count, is_dot)` for a row's unread badge, or `None` when it shows
/// none: a marked-as-unread chat shows a dot even with zero unread; the
/// count wins when there are unread messages; with mentions and a single
/// unread message the @ badge carries it (TGX `setCounter`).
pub(in crate::ui) fn chat_unread_indicator(chat: &ChatSummary) -> Option<(i32, bool)> {
    if chat.unread_count == 0 && chat.is_marked_as_unread {
        Some((0, true))
    } else if chat.unread_mention_count > 0 && chat.unread_count == 1 {
        None
    } else if chat.unread_count > 0 {
        Some((chat.unread_count, false))
    } else {
        None
    }
}

/// The red "Draft:" lead of a draft preview, with Telegram Desktop's
/// reply mark before it when the draft replies to a message
/// (`dialogsDraftFg`).
pub(super) fn draft_prefix(reply: bool, bare: bool) -> impl IntoElement {
    let tone = Hsla::from(danger());
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_0p5()
        .text_xs()
        .text_color(tone)
        .when(reply, |this| {
            this.child(
                Icon::new(IconName::Reply)
                    .size(px(12.))
                    .flex_none()
                    .text_color(tone),
            )
        })
        .child(if bare { "Draft:" } else { "Draft: " })
}

/// The mark after a row title: verified check, Premium star or status
/// emoji, or a bordered SCAM / FAKE label (`Ui::PeerBadge`).
pub(in crate::ui) fn title_badge_element(
    badge: TitleBadge,
    emoji: Option<std::path::PathBuf>,
    cx: &App,
) -> AnyElement {
    if let Some(label) = badge.label() {
        let tone = cx.theme().danger;
        // `dialogsScamFont` 9px semibold, 2px padding, 2px radius.
        return div()
            .flex_none()
            .px(px(2.))
            .rounded(px(2.))
            .border_1()
            .border_color(tone)
            .text_color(tone)
            .text_size(px(9.))
            .line_height(px(11.))
            .font_semibold()
            .child(label)
            .into_any_element();
    }
    match badge {
        TitleBadge::Verified => Icon::new(IconName::BadgeCheck)
            .size(px(14.))
            .flex_none()
            .text_color(Hsla::from(accent_strong()))
            .into_any_element(),
        TitleBadge::EmojiStatus(_) if emoji.is_some() => img(emoji.unwrap_or_default())
            .size(px(14.))
            .aspect_square()
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        // A status that has not downloaded yet shows the star.
        TitleBadge::EmojiStatus(_) | TitleBadge::PremiumStar => div()
            .flex_none()
            .text_size(px(13.))
            .line_height(px(14.))
            .text_color(Hsla::from(accent_strong()))
            .child("\u{2605}")
            .into_any_element(),
        TitleBadge::Scam | TitleBadge::Fake => div().into_any_element(),
    }
}

/// Unread counter at the row's trailing edge: accent for active chats,
/// neutral for muted ones; a bare dot for marked-as-unread.
pub(super) fn unread_pill(
    chat_id: ChatId,
    count: i32,
    dot: bool,
    muted: bool,
    // 0..1 scale-in progress (1 = settled); see `quill::row_fx`.
    scale: f32,
    cx: &App,
) -> AnyElement {
    let bg = if muted {
        cx.theme().muted_foreground.opacity(0.55)
    } else {
        Hsla::from(accent_strong())
    };
    let label = if count > 999 {
        format!("{}K", count / 1000)
    } else {
        count.to_string()
    };
    // Appearing badges grow from 60% and fade in; a settled one is the
    // plain pill, with no extra styling.
    let animating = scale < 1.;
    let k = 0.6 + 0.4 * scale;
    div()
        .id(("unread-pill", chat_id.0 as u64))
        .flex_none()
        .h(px(20.))
        .min_w(px(20.))
        .when(dot, |this| this.w(px(12.)).h(px(12.)).min_w(px(12.)))
        .when(animating, |this| {
            let edge = if dot { 12. } else { 20. } * k;
            this.h(px(edge))
                .min_w(px(edge))
                .when(dot, |this| this.w(px(edge)))
                .opacity(scale)
                .text_size(px(12. * k))
        })
        .px(if dot { px(0.) } else { px(6.) })
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(text_on_fill())
        .text_xs()
        .font_semibold()
        .aria_label(if dot {
            "Marked as unread".to_string()
        } else {
            format!("{count} unread")
        })
        .when(!dot, |this| this.child(label))
        .into_any_element()
}

/// Phase 5.1: forum indicator for forum supergroups in the chat list.
pub(super) fn forum_badge(chat_id: ChatId, cx: &App) -> impl IntoElement {
    div()
        .id(("forum-badge", chat_id.0 as u64))
        .flex_none()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(cx.theme().border)
        .text_color(cx.theme().muted_foreground)
        .text_xs()
        .child("Topics")
}

/// kit Phase 4: the unread indicator is a kit `Badge` overlaying the given
/// anchor (the chat avatar in chat rows, the topic name in topic rows) —
/// the kit's designed badge pattern. A count pill for `count > 0`
/// (capped at `99+` like the old pill), a dot for marked-as-unread.
pub(in crate::ui) fn unread_badge(anchor: AnyElement, count: i32, dot: bool) -> AnyElement {
    let badge = if dot {
        Badge::new().dot()
    } else {
        Badge::new().count(count.max(0) as usize).max(99)
    };
    badge
        .color(accent_strong())
        .child(anchor)
        .into_any_element()
}

/// Slice CL3 / kit Phase 4: the @ mention badge (TGX `TGChat.mentionCounter`
/// — shown when `unread_mention_count > 0`) as a kit `Badge` (Icon variant)
/// on a 16px anchor — the badge exactly fills its anchor by construction.
pub(super) fn mention_badge() -> impl IntoElement {
    // Telegram Desktop: a bare "@" in the accent color, no badge.
    Icon::new(IconName::AtSign)
        .size(px(16.))
        .text_color(accent_strong())
        .into_any_element()
}

/// The unread-reaction mark: Telegram Desktop draws a bare filled heart,
/// red, or grey when the chat is muted (`dialogsUnreadReaction`).
pub(super) fn reaction_badge(muted: bool) -> impl IntoElement {
    div()
        .text_base()
        .line_height(px(16.))
        .text_color(if muted {
            bg_badge_muted()
        } else {
            danger_bright()
        })
        .child("\u{2665}")
        .into_any_element()
}

/// Slice CL3: the select-mode check circle shown on every row while
/// multi-select is active (TGX select mode shows checkboxes over the
/// rows).
pub(super) fn select_check(chat_id: ChatId, checked: bool) -> impl IntoElement {
    div()
        .id(("select-check", chat_id.0 as u64))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(accent_strong())
        .bg(if checked {
            accent_strong()
        } else {
            bg_black().opacity(0.0)
        })
        .text_color(text_bright())
        .text_xs()
        .font_semibold()
        .w(px(20.))
        .h(px(20.))
        .child(if checked { "✓" } else { "" })
}
