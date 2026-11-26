//! The counters at the trailing edge of a chat row: ♥ reactions, @ mentions,
//! then the unread counter (or the pin mark), in TGX's order.
#![deny(clippy::indexing_slicing)]

use super::*;

/// `id` keys the unread pill; `pinned` shows the pin mark when nothing is
/// counted. Chat rows and forum topic rows share it.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn row_badges(
    id: ElementId,
    pinned: bool,
    ink: BadgeInk,
    muted_fg: Hsla,
    muted: bool,
    unread_indicator: Option<(i32, bool)>,
    has_reactions: bool,
    has_mentions: bool,
    badge_scale: f32,
    cx: &App,
) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .when(has_reactions, |this| this.child(reaction_badge(muted)))
        .when(has_mentions, |this| this.child(mention_badge(ink)))
        .map(|this| match unread_indicator {
            Some((count, dot)) => {
                this.child(unread_pill(id, count, dot, muted, ink, badge_scale, cx))
            }
            None if pinned && !has_mentions && !has_reactions => {
                this.child(row_glyph(IconName::Pin, muted_fg))
            }
            None => this,
        })
}
