//! chat-list row rendering: ChatListItem, badges, avatars, skeletons.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::message_text::kit_avatar_element;
use super::pressable::PressableDiv;
use super::userpic_videos::UserpicFrame;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::badge::Badge;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_style::ChatListRowStyle;
use quill::folder_limits::FolderTag;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::layout_style::SelectedRow;
use quill::local_path::sandboxed_display_path;
use quill::peer_badge::TitleBadge;
use quill::state::{ChatSummary, RowStatusIcon, Session, SidebarLine};
use quill::telegram::envelope::{
    ChatKind, EphemeralMessageContent, MessageContent, effective_content,
};
use quill::text::TextEntity;
mod forum_titles_demo;
pub(in crate::ui) mod row_badges;
mod topic_fit;
mod topic_titles;
#[cfg(test)]
mod topic_titles_tests;
/// Slice CL: floating chat-list peek preview — the chat being previewed
/// plus where it floated from (`MouseDownEvent.position` is in window
/// coordinates, same as the overlay's `.left()`/`.top()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatPreviewState {
    pub chat_id: ChatId,
    pub anchor: Point<Pixels>,
}

/// Slice CL: one read-only peek-preview line — sender name + body text.
/// The body is the same one-line `MessageContent::preview()` the chat
/// list shows (media falls back to its type label, e.g. "Photo").
/// Returns `(sender, body, message id)`. No interactive elements: the
/// preview never opens chats, sends, or marks anything read.
pub(super) fn chat_preview_line(
    peer_title: &str,
    is_outgoing: bool,
    author_signature: Option<&str>,
    content: &MessageContent,
    ephemeral: Option<&EphemeralMessageContent>,
    id: u64,
) -> (String, String, u64) {
    let name = if is_outgoing {
        tr(keys::FROM_YOU).to_string()
    } else {
        author_signature.unwrap_or(peer_title).to_string()
    };
    let body = effective_content(content, ephemeral).preview();
    let shown = |text: String| match quill::display_text::filtered(&text) {
        std::borrow::Cow::Borrowed(_) => text,
        std::borrow::Cow::Owned(shown) => shown,
    };
    (name, shown(body), id)
}

/// kit Phase 3: one flat item for the virtualized chat list — a chat row
/// (main list or archive) or the pinned-top "Archived chats" row / bar.
#[derive(Clone)]
pub(super) enum ChatListItem {
    // Ids only: rows look their chat up when rendered, so building the
    // list never clones thousands of summaries.
    Chat {
        id: ChatId,
        archived: bool,
        /// Declared virtual-list height (tags/preview-line aware).
        height: Pixels,
    },
    /// tdesktop's pinned-top "Archived chats" row (full height).
    ArchiveRow { height: Pixels },
    /// The same entry collapsed to a slim bar.
    ArchiveBar,
    /// A community shown as one chat ("Show as One Chat"), standing in
    /// for its chats in the main list.
    Community { community_id: i64, height: Pixels },
    /// The stories strip, first row of the list: it scrolls away with the
    /// rows, which is what collapses it (`quill::stories_strip`).
    StoryStrip,
}

impl ChatListItem {
    pub(super) fn height(&self) -> Pixels {
        match self {
            Self::Chat { height, .. }
            | Self::ArchiveRow { height }
            | Self::Community { height, .. } => *height,
            Self::ArchiveBar => px(quill::chatlist_archive::COLLAPSED_BAR_HEIGHT),
            Self::StoryStrip => px(quill::stories_strip::FULL_HEIGHT),
        }
    }
}

/// kit Phase 9: a loading placeholder row for the chat list — a kit
/// `Skeleton` avatar circle plus two skeleton text lines, in the shape and
/// sizes of the two-line chat row it stands for.
pub(super) fn chat_list_skeleton_row(
    index: usize,
    m: &quill::density::DensityMetrics,
) -> impl IntoElement {
    div()
        .id(("chat-list-skeleton", index))
        .flex()
        .items_center()
        .gap(px(m.row_gap))
        .px(px(m.row_inset))
        .h(px(m.row_base))
        .child(Skeleton::new().size(px(m.avatar)).rounded_full())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .flex_1()
                .child(Skeleton::new().w(px(110.)).h(px(12.)).rounded_sm())
                .child(
                    Skeleton::new()
                        .w(px(190.))
                        .h(px(10.))
                        .rounded_sm()
                        .secondary(),
                ),
        )
}

/// Section caption above the chat list, shown only where it adds
/// information the list itself doesn't (search mode, connecting).
pub(super) fn chat_list_caption(mode: PaneMode, session: Option<&Session>) -> Option<SharedString> {
    match mode {
        PaneMode::Synthetic => None,
        PaneMode::Connecting => Some(tr(keys::Q_CHATLIST_CONNECTING).into()),
        // Search shows its own section headings.
        PaneMode::Ready => {
            let _ = session;
            None
        }
    }
}

pub(super) fn static_chat_row(
    title: &'static str,
    preview: &'static str,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .w_full()
        .px_2()
        .py_2()
        .rounded_md()
        .role(Role::Button)
        .aria_label(title)
        .aria_selected(selected)
        .bg(if selected {
            cx.theme().primary.opacity(0.25)
        } else {
            cx.theme().sidebar
        })
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(if selected {
                    selected_secondary_fg(cx.theme().foreground)
                } else {
                    cx.theme().muted_foreground
                })
                .child(preview),
        )
}

/// Phase 6: circular initials avatar (contact rows, info panels) shown
/// when no downloaded profile photo is available.
/// kit Phase 4: kit `Avatar` — initials + theme fallback colors.
pub(super) fn initials_avatar(name: &str, size: f32) -> impl IntoElement {
    kit_avatar_element(name, None, px(size))
}

/// Parity slice: circular chat avatar — the downloaded `chat.photo.small`
/// thumbnail when available, otherwise the kit's initials + theme
/// fallback colors. Used by chat-list rows, the conversation header, and
/// info panels.
/// kit Phase 4: kit `Avatar`; the old `chat_avatar_color` palette is gone.
pub(super) fn chat_avatar(
    name: &str,
    photo_path: Option<&std::path::Path>,
    size: f32,
) -> impl IntoElement {
    kit_avatar_element(name, photo_path, px(size))
}

/// Saved Messages' avatar: a bookmark on the accent color (Telegram
/// Desktop draws it instead of your own photo).
pub(super) fn saved_messages_avatar(size: f32) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .bg(accent())
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(IconName::Bookmark)
                .size(px(size * 0.45))
                .text_color(gpui_kit::white()),
        )
        .into_any_element()
}

/// Parity slice: compact subscriber/member counts for the header
/// ("12.3K", "1.2M").
pub(super) fn compact_count(count: i32) -> String {
    if count >= 1_000_000 {
        let value = count as f64 / 1_000_000.0;
        return format!(
            "{}{}",
            if value >= 100.0 {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            },
            "M"
        );
    }
    if count >= 1_000 {
        let value = count as f64 / 1_000.0;
        return format!(
            "{}{}",
            if value >= 100.0 {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            },
            "K"
        );
    }
    count.to_string()
}

/// kit Phase 3: folder-tag chip names for a chat row — shared by the row
/// renderer and the virtual-list height computation so they agree.
/// The folder tag chips of a row: coloured folders the chat is in, except
/// the one being viewed (tdesktop `Entry::hasChatsFilterTags`).
pub(super) fn chat_row_tags(
    chat: &ChatSummary,
    folders: &[(i32, String, i32)],
    show_tags: bool,
    viewing: Option<i32>,
) -> Vec<FolderTag> {
    quill::folder_limits::row_tags(
        |folder_id| chat.folder_positions.contains_key(&folder_id),
        folders,
        show_tags,
        viewing,
    )
}

/// Whether [`chat_row_tags`] would give `chat` any chip: all the list needs
/// to size the row, without building the chips.
pub(super) fn chat_row_has_tags(
    chat: &ChatSummary,
    folders: &[(i32, String, i32)],
    show_tags: bool,
    viewing: Option<i32>,
) -> bool {
    quill::folder_limits::row_has_tags(
        |folder_id| chat.folder_positions.contains_key(&folder_id),
        folders,
        show_tags,
        viewing,
    )
}

/// Row extent shared by the renderer, virtual list and pin reordering
/// (`quill::chatlist_style::chat_row_height_px`).
pub(super) fn chat_row_height(
    has_tags: bool,
    preview_lines: u8,
    m: &quill::density::DensityMetrics,
) -> Pixels {
    px(quill::chatlist_style::chat_row_height_px(
        has_tags,
        preview_lines,
        m,
    ))
}

/// A list row's text column, title over preview, at the density's line
/// height and gap (the row heights fit them). Chat, archive and community
/// rows share it.
pub(super) fn row_text_lines(m: &quill::density::DensityMetrics) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(m.row_lines_gap))
        .when_some(m.line_height, |lines, line| lines.line_height(px(line)))
        .min_w_0()
        .flex_1()
}

pub(super) fn session_chat_row(
    input: ChatRowInput<'_>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + Styled + StatefulInteractiveElement + use<> {
    let ChatRowInput {
        chat,
        selected,
        folders,
        viewing_folder,
        show_tags,
        photo_path,
        draggable,
        archived,
        selecting,
        checked,
        fx,
        title_badge,
        badge_emoji,
        verify_icon,
        row_style,
        height,
        saved,
        preview_sender,
        preview_emoji,
        kind_icon,
        forum,
        story_ring,
        video,
        promotion,
        muted_fg,
        rest_fill,
        corners,
        glass,
    } = input;
    let id = chat.id;
    // The open row's colours: Classic's tint, the solid accent row, or on
    // glass the theme's text over a glass highlight.
    let ink = match glass {
        Some(_) => RowInk::of(selected, SelectedRow::Tint, rest_fill, muted_fg, cx.theme())
            .on_glass(rest_fill),
        None => RowInk::of(
            selected,
            row_style.selected,
            rest_fill,
            muted_fg,
            cx.theme(),
        ),
    };
    let muted_fg = ink.secondary;
    let stamp = match promotion.as_ref() {
        Some(badge) => Some((RowStatusIcon::None, badge.label.clone())),
        None => row_stamp(chat),
    };
    let title: String = if saved {
        tr(keys::SAVED_MESSAGES).to_string()
    } else {
        chat.title.clone()
    };
    // A draft leads with a red "Draft:" (and a reply mark); anything else
    // is the plain preview line.
    // The announcement of a promoted chat comes first (tdesktop
    // `topPromotionMessage`).
    let (preview, draft): (String, Option<bool>) =
        match promotion.as_ref().and_then(|badge| badge.preview.clone()) {
            Some(text) => (text, None),
            None => match chat.sidebar_line() {
                SidebarLine::Text(text) => (text, None),
                SidebarLine::Draft(draft) => (draft.text, Some(draft.reply)),
            },
        };
    // A promoted row paints only the announcement: no typing line and no
    // sender of the last message (tdesktop's promoted branch).
    let announced = promotion
        .as_ref()
        .is_some_and(|badge| badge.preview.is_some());
    let activity = chat.peer_activity().filter(|_| !announced);
    // Slice chatlist-list-style: the icon/entities describe
    // `last_preview` only — draft/typing/activity lines render unstyled.
    // (When the shown text equals `last_preview` the entities describe
    // it even if the text arrived via the draft path.)
    let from_last = draft.is_none() && preview == chat.last_preview();
    // A sender prefix and a right-to-left preview are one line of text, whose direction
    // comes from the first strong character (a Latin name: left-to-right).
    let combine_prefix = preview_sender.is_some()
        && from_last
        && activity.is_none()
        && chat.last_preview_thumb().is_none()
        && quill::text::has_rtl_text(&preview);
    let icon: Option<&str> = if row_style.media_icons && from_last {
        chat.last_preview_style().icon
    } else {
        None
    };
    // Custom emoji are content, not formatting: they show even when the
    // rich preview (bold, italic…) is off.
    let custom_only: Vec<TextEntity>;
    let entities: &[TextEntity] = if !from_last {
        &[]
    } else if row_style.rich_preview {
        &chat.last_preview_style().entities
    } else {
        custom_only = chat
            .last_preview_style()
            .entities
            .iter()
            .filter(|e| matches!(e.kind, quill::text::TextEntityKind::CustomEmoji { .. }))
            .cloned()
            .collect();
        &custom_only
    };
    // Slice CL1: a marked-as-unread chat shows the unread badge even
    // with zero unread messages (official clients show a dot); the count
    // wins when there are unread messages (TGX TGChat.java:396).
    // Slice CL3: with mentions and a single unread message the main
    // counter hides — the @ badge carries it (TGX `setCounter`:
    // `hasMentions && unreadCount == 1 ? 0 : unreadCount`).
    // kit Phase 4: the unread indicator is a kit `Badge` overlaying the
    // avatar — `(count, is_dot)`.
    let has_mentions = chat.unread_mention_count > 0;
    let jump = forum.as_ref().and_then(|line| line.jump(selected));
    let unread_indicator = match forum.as_ref().map(|line| &line.forum) {
        // tdesktop never marks a forum itself: only its topics count
        // (`history.cpp:2765-2770`, `data_thread.cpp:117-119`).
        Some(forum) => (forum.unread_topics > 0).then_some((forum.unread_topics, false)),
        None => chat_unread_indicator(chat),
    };
    let has_reactions = chat.unread_reaction_count > 0;
    // The counters at the trailing edge: on the topics line of a forum row,
    // else on the preview line.
    let badge_muted = match forum.as_ref().map(|line| &line.forum) {
        Some(forum) if forum.unread_topics > 0 => forum.badge_muted,
        _ => chat.is_muted(),
    };
    let badges = row_badges::row_badges(
        ("unread-pill", id.0 as u64).into(),
        pinned_here(chat, archived),
        ink.badges,
        muted_fg,
        badge_muted,
        unread_indicator,
        has_reactions,
        has_mentions,
        fx.badge,
        cx,
    )
    .into_any_element();
    let (forum_badges, plain_badges) = if forum.is_some() {
        (Some(badges), None)
    } else {
        (None, Some(badges))
    };
    // kit Phase 7: screen-reader label for the row.
    let row_label = if draft.is_some() {
        tr_with(
            keys::Q_CHATLIST_DRAFT,
            &[
                ("title", &title.to_string()),
                ("preview", &preview.to_string()),
            ],
        )
    } else {
        format!("{title} — {preview}")
    };
    // kit Phase 3: tag chips via the shared helper — the row height
    // (declared to the `VirtualList`) is derived from the same list.
    let tags = chat_row_tags(chat, folders, show_tags, viewing_folder);
    let m = row_style.metrics();
    let avatar_size = m.avatar;
    div()
        .id(("chat-row", id.0 as u64))
        .a11y_tag(quill::agent::ui_driver::redact::chat_tag(id.0))
        .w_full()
        .px(px(m.row_inset))
        .role(Role::Button)
        .aria_label(row_label)
        // kit Phase 3: fixed height (`chat_row_height`) — the `VirtualList`
        // positions rows from declared sizes, so the row enforces the same
        // height and centers its content. Title and preview truncate to one
        // line each, so content can never overflow it; the heights fit the
        // density's line height (`DensityMetrics::line_height`).
        .h(height)
        .flex()
        .flex_col()
        .justify_center()
        .rounded(corners)
        .when_some(glass.filter(|_| selected), |this, lens| {
            this.backdrop_glass(Some(lens))
        })
        // Plain arrow while hovering: on macOS 26 the pointing hand reads
        // as a drag affordance, which a row you only click isn't.
        .when(!selected, |this| this.pressable(cx.theme()))
        // UI tests find the row by its id (inert outside test support).
        .test_support()
        .aria_selected(selected)
        .when_some(ink.open_hover, |this, (hover, pressed)| {
            this.hover(move |s| s.bg(hover))
                .active(move |s| s.bg(pressed))
        })
        .bg(ink.fill)
        .when_some(ink.title, |this, title| this.text_color(title))
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            // Slice CL3: in multi-select mode a row click toggles the
            // check instead of opening the chat.
            if selecting {
                this.toggle_chat_selected(id, cx);
            } else if event.modifiers().secondary(cx) {
                // Ctrl+click (Cmd on macOS) opens the chat in a new window
                // (tdesktop `dialogs_inner_widget.cpp`: `row.newWindow =
                // modifiers & Qt::ControlModifier`).
                this.open_chat_in_new_window(id, window, cx);
            } else {
                this.select_listed_chat(id, window, cx);
                this.focus_clicked_chat(id, window, cx);
            }
        }))
        // Slice CL1: right-click opens the chat-row context menu at the
        // click position (window coordinates).
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                this.message_ui.menu.open_row(ChatMenuState::at(id, event.position));
                cx.notify();
            }),
        )
        // tdesktop's quick action also runs on a middle click
        // (`dialogs_inner_widget.cpp`, `Qt::MiddleButton`); a no-op while
        // the setting is Disabled.
        .on_mouse_up(
            MouseButton::Middle,
            cx.listener(move |this, _, _, cx| {
                if !selecting {
                    this.perform_swipe_action(id, cx);
                }
            }),
        )
        // Slice CL: long-press (press-and-hold) peeks at the chat's
        // recent messages without opening it (tdesktop shows the same
        // preview on hover). A quick release is still a plain click.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                // Multi-select taps toggle the check — no preview there.
                if !selecting {
                    this.begin_chat_preview_press(id, event.position, cx);
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, _, _, _| {
                this.message_ui.menu.cancel_press();
            }),
        )
        // Release outside the row (e.g. press, drag off, let go) also
        // cancels the pending long press.
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, _, _, _| {
                this.message_ui.menu.cancel_press();
            }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(m.row_gap))
                // Parity slice: circular chat photo or colored initials for
                // every chat-list row / chat type.
                // Slice CL3: the select-mode check circle precedes the
                // avatar while multi-select is active.
                .when(selecting, |this| this.child(select_check(id, checked)))
                .child(with_presence_dot_scaled(
                    with_call_badge(
                    match story_ring {
                        Some(ring) => super::story_ring::with_story_ring(
                            |size| {
                                if saved {
                                    saved_messages_avatar(size)
                                } else {
                                    UserpicFrame::over(
                                        video.as_ref(),
                                        chat_avatar(&title, photo_path, size).into_any_element(),
                                        size,
                                    )
                                }
                            },
                            ring,
                            avatar_size,
                            muted_fg,
                        ),
                        None if saved => saved_messages_avatar(avatar_size),
                        None => UserpicFrame::over(
                            video.as_ref(),
                            chat_avatar(&title, photo_path, avatar_size).into_any_element(),
                            avatar_size,
                        ),
                    },
                    quill::peer_badge::shows_call_badge(chat.video_chat.as_ref()),
                    cx,
                    ),
                    fx.online,
                    13.,
                    ink.ring,
                    cx,
                ))
                .child(
                    row_text_lines(m)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .min_w_0()
                                        .flex_1()
                                        // Phase B1: lock for secret chats (E2E).
                                        .when(
                                            matches!(chat.kind, ChatKind::Secret { .. }),
                                            |this| {
                                                this.child(row_glyph(
                                                    IconName::Lock,
                                                    success().into(),
                                                ))
                                            },
                                        )
                                        .when_some(kind_icon, |this, icon| {
                                            this.child(row_glyph(icon, muted_fg))
                                        })
                                        .when_some(verify_icon, |this, path| {
                                            this.child(super::bot_verify_icon::bot_verify_icon(path))
                                        })
                                        .child(
                                            div()
                                                .font_semibold()
                                                .min_w_0()
                                                .truncate()
                                                .child(
                                                    super::emoji_sprites::one_line(&title, px(14.))
                                                        .unwrap_or_else(|| {
                                                            super::bidi_line::one_line_plain(title)
                                                                .into_any_element()
                                                        }),
                                                ),
                                        )
                                        .when_some(title_badge, |this, badge| {
                                            this.child(title_badge_element(
                                                badge,
                                                badge_emoji,
                                                cx,
                                            ))
                                        })
                                        .when(chat.is_muted(), |this| {
                                            this.child(row_glyph(
                                                IconName::BellOff,
                                                muted_fg,
                                            ))
                                        })
                                        .when(chat.is_forum_chat(), |this| {
                                            this.child(forum_badge(id, cx))
                                        }),
                                )
                                .when_some(stamp, |this, (status, stamp)| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .items_center()
                                            .gap_0p5()
                                            .text_xs()
                                            .text_color(muted_fg)
                                            .when(status != RowStatusIcon::None, |this| {
                                                this.child(row_status_mark(status, cx))
                                            })
                                            .child(stamp),
                                    )
                                }),
                        )
                        .when_some(forum.as_ref(), |this, forum| {
                            this.child(topic_titles::forum_topics_line(
                                id,
                                forum,
                                selected,
                                selecting,
                                muted_fg,
                                forum_badges,
                                cx,
                            ))
                        })
                        .when(forum.is_none() && row_style.preview_lines >= 3, |this| {
                            // Slice chatlist-list-style: the third line
                            // names the sender ("You" / author signature /
                            // chat title).
                            this.child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .truncate()
                                    .text_color(muted_fg)
                                    .child(if announced {
                                        String::new()
                                    } else {
                                        chat.last_preview_sender().to_string()
                                    }),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child({
                                    let message = div()
                                        .when(jump.is_none(), |this| this.flex_1())
                                        .min_w_0()
                                        .flex()
                                        .min_w_0()
                                        .items_center()
                                        .when_some(
                                            draft.filter(|_| activity.is_none()),
                                            |this, reply| {
                                                this.child(draft_prefix(reply, preview.is_empty()))
                                            },
                                        )
                                        .when_some(
                                            preview_sender
                                                .clone()
                                                .filter(|_| from_last)
                                                .filter(|_| !combine_prefix),
                                            |this, sender| {
                                                this.child(
                                                    div()
                                                        .flex_none()
                                                        .text_xs()
                                                        .text_color(ink.sender)
                                                        // Drawn on its own, so a Hebrew name keeps its colon
                                                        // on the reading side (isolated, as tdesktop's WrapRtl).
                                                        .child(format!("{}: ", quill::text::isolate(&sender))),
                                                )
                                            },
                                        )
                                        .when_some(
                                            chat.last_preview_thumb()
                                                .cloned()
                                                .filter(|_| from_last),
                                            |this, mini| {
                                                this.child(
                                                    img(super::image_budget::sized_image(
                                                        super::image_budget::SizedSource::Mini(
                                                            mini,
                                                        ),
                                                        px(18.),
                                                    ))
                                                    .id(("preview-thumb", id.0 as u64))
                                                    .size(px(18.))
                                                    .aspect_square()
                                                    .flex_none()
                                                    .mr_1()
                                                    .rounded_sm()
                                                    .object_fit(ObjectFit::Cover),
                                                )
                                            },
                                        )
                                        .child(match activity {
                                            // Typing / recording / uploading replaces
                                            // the preview, in the accent color.
                                            Some(line) => div()
                                                .min_w_0()
                                                .flex_1()
                                                .flex()
                                                .items_center()
                                                .text_xs()
                                                .text_color(ink.activity)
                                                .child(
                                                    super::activity_indicator::activity_indicator(
                                                        line.indicator,
                                                        ink.activity,
                                                        format!("row-activity-{}", id.0).into(),
                                                    ),
                                                )
                                                .child(div().min_w_0().truncate().child(
                                                    quill::display_text::filtered(&line.text)
                                                        .into_owned(),
                                                ))
                                                .into_any_element(),
                                            None => div()
                                                .min_w_0()
                                                .flex_1()
                                                .child(
                                                    super::chatlist_style::chat_list_preview_line_layered(
                                                        icon,
                                                        &preview,
                                                        entities,
                                                        &preview_emoji.still,
                                                        &preview_emoji.layered,
                                                        combine_prefix
                                                            .then(|| {
                                                                preview_sender
                                                                    .as_ref()
                                                                    .map(|sender| format!("{sender}: "))
                                                            })
                                                            .flatten(),
                                                        muted_fg,
                                                    ),
                                                )
                                                .into_any_element(),
                                        });
                                    match jump {
                                        Some(topic) => topic_titles::jump_message(
                                            id,
                                            topic,
                                            selecting,
                                            muted_fg,
                                            message.into_any_element(),
                                            cx,
                                        )
                                        .into_any_element(),
                                        None => message.into_any_element(),
                                    }
                                })
                                .children(plain_badges),
                        ),
                ),
        )
        .when(!tags.is_empty(), |this| {
            // kit Phase 3: the tag strip has a fixed height so tagged rows
            // stay exactly `chat_row_height` tall; extra chips clip.
            this.child(
                div().pt_1().child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_1()
                        .h(px(20.))
                        .overflow_hidden()
                        .children(
                            tags.into_iter()
                                .map(|tag| super::folder_extras::tag_chip(tag.text, tag.color_id)),
                        ),
                ),
            )
        })
        // Slice CL2: pin-drag reorder — dropping a pinned chat onto
        // another pinned row moves it to that row's slot (TGX
        // `ChatsAdapter.movePinnedChat` sends the full reordered
        // pinned-id list via `setPinnedChats`).
        .when(draggable, |this| {
            let drag = PinnedChatDrag {
                chat_id: id,
                archived,
            };
            this.on_drag(drag, |drag: &PinnedChatDrag, _, _, cx| cx.new(|_| drag.clone()))
                .drag_start_distance(px(quill::selection_drag::START_DRAG_DISTANCE))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, _| {
                        this.chat_list.pin.press(id.0, f32::from(event.position.y));
                    }),
                )
                .on_drop(cx.listener(move |this, _: &PinnedChatDrag, _, cx| {
                    this.finish_pin_drag(cx);
                }))
        })
}

/// Slice CL2: drag payload for pinned-chat reorder. The real row follows
/// the pointer (tdesktop drags the row itself), so the cursor-attached
/// ghost draws nothing.
#[derive(Clone)]
pub(super) struct PinnedChatDrag {
    pub(super) chat_id: ChatId,
    pub(super) archived: bool,
}

impl Render for PinnedChatDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// An avatar with a green "online" dot at its bottom-right, ringed in the
/// sidebar color (chat list and contacts).
pub(super) fn with_presence_dot(
    avatar: impl IntoElement,
    online: bool,
    dot: f32,
    cx: &App,
) -> impl IntoElement {
    with_presence_dot_scaled(
        avatar,
        if online { 1. } else { 0. },
        dot,
        cx.theme().sidebar,
        cx,
    )
}

/// [`with_presence_dot`] with the dot at `scale` (0 hidden, 1 full): the
/// 150 ms grow / shrink Telegram Desktop plays when a user goes online or
/// offline. The dot scales about its centre inside a fixed box, so the
/// avatar's layout never moves.
pub(super) fn with_presence_dot_scaled(
    avatar: impl IntoElement,
    scale: f32,
    dot: f32,
    // The ring between dot and photo: the row's fill.
    ring: Hsla,
    cx: &App,
) -> impl IntoElement {
    const RING: f32 = 2.;
    div()
        .relative()
        .flex_none()
        .child(avatar)
        .when(scale > 0., |this| {
            this.child(
                div()
                    .absolute()
                    .right(px(0.))
                    .bottom(px(1.))
                    .size(px(dot))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(dot * scale))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(ring)
                            .child(
                                div()
                                    .size(px((dot - 2. * RING) * scale))
                                    .rounded_full()
                                    .bg(cx.theme().success),
                            ),
                    ),
            )
        })
}

/// Chat-row avatar edge (px). Row heights in `chatlist_style` leave room
/// for it plus vertical padding.
pub(in crate::ui) const CHAT_ROW_AVATAR: f32 = 46.;

mod folder_tag_context;
mod row_input;
pub(in crate::ui) use row_input::ChatRowInput;
mod pin_lift;
pub(in crate::ui) use folder_tag_context::FolderTagContext;
mod promoted_demo;
mod row_ink;
pub(in crate::ui) use row_ink::{
    BadgeInk, RowInk, open_row_hover_fill, tinted_open_row_hover_fill,
};
mod selected_fg;
pub(crate) use selected_fg::selected_secondary_fg;
mod with_call_badge;
#[allow(unused_imports)]
pub use with_call_badge::*;

#[cfg(test)]
#[path = "preview_line_tests.rs"]
mod preview_line_tests;
