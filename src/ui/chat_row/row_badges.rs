//! The counters at the trailing edge of a chat row: ♥ reactions, @ mentions,
//! then the unread counter (or the pin mark), in TGX's order.
#![deny(clippy::indexing_slicing)]

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn row_badges(
    chat: &ChatSummary,
    archived: bool,
    muted_fg: Hsla,
    muted: bool,
    unread_indicator: Option<(i32, bool)>,
    has_reactions: bool,
    has_mentions: bool,
    badge_scale: f32,
    cx: &App,
) -> impl IntoElement + use<> {
    let id = chat.id;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .when(has_reactions, |this| this.child(reaction_badge(muted)))
        .when(has_mentions, |this| this.child(mention_badge()))
        .map(|this| match unread_indicator {
            Some((count, dot)) => this.child(unread_pill(id, count, dot, muted, badge_scale, cx)),
            None if pinned_here(chat, archived) && !has_mentions && !has_reactions => {
                this.child(row_glyph(IconName::Pin, muted_fg))
            }
            None => this,
        })
}
