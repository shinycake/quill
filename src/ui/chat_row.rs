//! chat-list row rendering: ChatListItem, badges, avatars, skeletons.

use super::app::ChatListFilter;
use super::app::{PaneMode, QuillApp};
use super::message_text::kit_avatar_element;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::badge::Badge;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_style::ChatListRowStyle;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::peer_badge::TitleBadge;
use quill::row_fx::RowFx;
use quill::state::{ChatSummary, RowStatus, Session, SidebarLine};
use quill::telegram::envelope::{
    ChatKind, EphemeralMessageContent, MessageContent, effective_content,
};
use quill::text::TextEntity;
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
        "You".to_string()
    } else {
        author_signature.unwrap_or(peer_title).to_string()
    };
    let body = effective_content(content, ephemeral).preview();
    (name, body, id)
}

/// kit Phase 3: one flat item for the virtualized chat list — a chat row
/// (main list or archive) or the collapsible archive section header.
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
    ArchiveHeader {
        count: usize,
        any_unread: bool,
        collapsed: bool,
    },
    ArchiveEmpty,
}

/// kit Phase 9: a loading placeholder row for the chat list — a kit
/// `Skeleton` avatar circle plus two skeleton text lines, matching the
/// shape of a chat row.
pub(super) fn chat_list_skeleton_row(index: usize) -> impl IntoElement {
    div()
        .id(("chat-list-skeleton", index))
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .child(Skeleton::new().w(px(40.)).h(px(40.)).rounded_full())
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

/// kit Phase 9: centered empty state for the chat list (no chats / empty
/// folder / no unread), composed from kit `Skeleton`-free primitives with
/// theme tokens — glyph, title, hint.
pub(super) fn chat_list_empty_state(
    glyph: &'static str,
    title: &'static str,
    hint: &'static str,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id("chat-list-empty")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_6()
        .py_10()
        .child(div().text_3xl().child(glyph))
        .child(
            div()
                .font_semibold()
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        )
}

/// Section caption above the chat list, shown only where it adds
/// information the list itself doesn't (search mode, connecting).
pub(super) fn chat_list_caption(mode: PaneMode, session: Option<&Session>) -> Option<SharedString> {
    match mode {
        PaneMode::Synthetic => None,
        PaneMode::Connecting => Some("Connecting…".into()),
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
        .border_l_2()
        .border_color(if selected {
            cx.theme().primary
        } else {
            transparent_black()
        })
        .bg(if selected {
            cx.theme().primary.opacity(0.25)
        } else {
            cx.theme().sidebar
        })
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
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
pub(super) fn chat_row_tags(
    chat: &ChatSummary,
    folders: &[(i32, String)],
    show_tags: bool,
) -> Vec<String> {
    if show_tags {
        folders
            .iter()
            .filter(|(folder_id, _)| chat.folder_positions.contains_key(folder_id))
            .map(|(_, name)| name.clone())
            .collect()
    } else {
        Vec::new()
    }
}

/// kit Phase 3: chat rows render at a fixed height so the kit
/// `VirtualList` can position them from declared sizes — 56px base
/// (avatar 40 + the old py_2), 80px when the folder-tag strip is present.
/// `session_chat_row` enforces the same height on the element.
pub(super) fn chat_row_height(tags: &[String], preview_lines: u8) -> Pixels {
    px(quill::chatlist_style::chat_row_height_px(
        !tags.is_empty(),
        preview_lines,
    ))
}

impl QuillApp {
    /// kit Phase 3: resolve one virtualized chat-list item to its element.
    /// Only visible indices are built, once per frame — selection, badges,
    /// pin-drag, multi-select and context-menu behavior are unchanged.
    pub(super) fn chat_list_item_element(
        &mut self,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = self.chat_list_items.get(ix).cloned();
        match item {
            Some(ChatListItem::ArchiveHeader {
                count,
                any_unread,
                collapsed,
            }) => self.archive_header_element(count, any_unread, collapsed, cx),
            Some(ChatListItem::ArchiveEmpty) => div()
                .h(px(24.))
                .flex()
                .items_center()
                .px_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("No archived chats.")
                .into_any_element(),
            Some(ChatListItem::Chat { id, archived, .. }) => {
                let Some(chat) = self.session().and_then(|s| s.chats.get(&id.0)) else {
                    return div().into_any_element();
                };
                let open = self.session().and_then(|s| s.open_chat);
                let selected = open == Some(chat.id);
                // Parity slice: folder names + tags flag for chat-row chips.
                let (folder_names, show_folder_tags) = self
                    .session()
                    .map(|s| {
                        (
                            s.chat_folders
                                .iter()
                                .map(|f| (f.id, f.name.clone()))
                                .collect::<Vec<_>>(),
                            s.are_folder_tags_enabled,
                        )
                    })
                    .unwrap_or_default();
                // Parity slice: chat photos resolve per visible row and are
                // sandboxed before display.
                let media_roots = self.media_display_roots();
                let photo = self
                    .session()
                    .and_then(|s| s.chat_photo_path(chat.id))
                    .and_then(|path| sandboxed_display_path(path, &media_roots));
                // Slice CL3: multi-select mode — rows toggle the check
                // instead of opening the chat.
                let selecting = !self.selected_chats.is_empty();
                let checked = selecting && self.selected_chats.contains(&chat.id.0);
                // Slice CL2: pin drag runs only on the unfiltered list with
                // at least two pinned chats (TGX `ChatsAdapter`); the
                // archive has its own pinned set.
                let draggable = if archived {
                    (self.chat_filter == ChatListFilter::All
                        || self.chat_filter == ChatListFilter::Archived)
                        && self
                            .session()
                            .is_some_and(|s| s.pinned_chat_ids(true).len() >= 2)
                        && chat.archive_is_pinned
                } else {
                    self.chat_filter == ChatListFilter::All
                        && self.folder_tab.is_none()
                        && self
                            .session()
                            .is_some_and(|s| s.pinned_chat_ids(false).len() >= 2)
                        && chat.is_pinned
                };
                // The activity indicator is clock-driven: keep ticking while
                // a visible row shows one (the chat list's animation layer
                // ticks its own).
                if chat.peer_activity().is_some() && super::anim_layer::current().is_none() {
                    self.request_animation_tick(super::activity_indicator::FPS, cx);
                }
                let saved = self.session().is_some_and(|s| s.is_saved_messages(chat.id));
                let online = !saved && self.session().is_some_and(|s| s.chat_peer_online(chat));
                // Online dot and new-badge animations: 150 ms, only while
                // the window is active (inactive windows snap).
                let active = self.window_active.get();
                let fx = self.row_fx.borrow_mut().observe(
                    chat.id.0,
                    online,
                    chat_unread_indicator(chat).is_some(),
                    active,
                    std::time::Instant::now(),
                );
                if fx.animating {
                    self.request_animation_tick(60, cx);
                }
                let title_badge = if saved {
                    None
                } else {
                    self.session().and_then(|s| s.chat_title_badge(chat))
                };
                // Status emoji: the still image (never animated in the
                // list, so many Premium rows keep the window idle).
                let badge_emoji = match title_badge {
                    Some(TitleBadge::EmojiStatus(id)) => self.custom_emoji_still(id),
                    _ => None,
                };
                session_chat_row(
                    chat,
                    selected,
                    &folder_names,
                    show_folder_tags,
                    photo.as_deref(),
                    draggable,
                    archived,
                    selecting,
                    checked,
                    fx,
                    title_badge,
                    badge_emoji,
                    // Slice chatlist-list-style: Settings → Appearance.
                    ChatListRowStyle::new(
                        self.appearance.preview_lines,
                        self.appearance.chat_list_media_icons,
                        self.appearance.chat_list_rich_preview,
                    ),
                    saved,
                    self.session().and_then(|s| s.chat_preview_sender(chat)),
                    self.preview_emoji_images(chat, cx),
                    match chat.kind {
                        ChatKind::BasicGroup { .. }
                        | ChatKind::Supergroup {
                            is_channel: false, ..
                        } => Some(IconName::Users),
                        ChatKind::Supergroup {
                            is_channel: true, ..
                        } => Some(IconName::Megaphone),
                        ChatKind::Private { user_id }
                            if self
                                .session()
                                .and_then(|s| s.user(user_id.0))
                                .is_some_and(|u| u.is_bot) =>
                        {
                            Some(IconName::Bot)
                        }
                        _ => None,
                    },
                    self.session().and_then(|s| s.chat_row_topic_names(chat.id)),
                    cx,
                )
                .into_any_element()
            }
            None => div().into_any_element(),
        }
    }

    /// kit Phase 3: the archive section header as a fixed-height virtual
    /// list item — collapses the section, marks the archive read, and
    /// opens the auto-archive settings (Slice CL2, unchanged behavior).
    pub(super) fn archive_header_element(
        &mut self,
        count: usize,
        any_unread: bool,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("archive-section")
            .h(px(32.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .id("archive-section-toggle")
                    .role(gpui_kit::Role::Button)
                    .aria_label("Expand or collapse archived chats")
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} Archived ({})",
                        if collapsed { "▸" } else { "▾" },
                        count
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_archive_collapsed(cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(any_unread && !collapsed, |this| {
                        this.child(
                            Button::new("archive-mark-read")
                                .label("✓")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.mark_all_chats_as_read(true, cx);
                                })),
                        )
                    })
                    .child(Button::new("archive-settings").label("⚙").ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.open_archive_settings(cx);
                        }),
                    )),
            )
            .into_any_element()
    }
}

pub(super) fn session_chat_row(
    chat: &ChatSummary,
    selected: bool,
    // Parity slice: `(folder id, name)` for folder-tag chips.
    folders: &[(i32, String)],
    // Parity slice: show folder-tag chips (`are_folder_tags_enabled`).
    show_tags: bool,
    // Parity slice: sandboxed display path for the downloaded chat photo
    // (`chat.photo.small`), if any; the avatar falls back to colored
    // initials otherwise.
    photo_path: Option<&std::path::Path>,
    // Slice CL2: pinned rows are drag-reorderable, but only on the
    // unfiltered list with at least two pinned chats (TGX
    // `ChatsAdapter`); the caller computes this.
    draggable: bool,
    // Slice CL2: which pinned list the row is in (`setPinnedChats`
    // takes main or archive).
    archived: bool,
    // Slice CL3: multi-select mode — the row toggles selection instead
    // of opening the chat, and shows the check circle.
    selecting: bool,
    checked: bool,
    // Animated progress for this frame: the online dot on the avatar of
    // a private chat whose user is online, and a newly shown unread badge.
    fx: RowFx,
    // Verified / Premium / SCAM mark after the title.
    title_badge: Option<TitleBadge>,
    // Still image for a Premium emoji status, once downloaded.
    badge_emoji: Option<std::path::PathBuf>,
    // Slice chatlist-list-style: preview line count, media icons, and
    // formatted preview (Settings → Appearance → Chat list rows).
    row_style: ChatListRowStyle,
    // The chat with yourself: "Saved Messages" with a bookmark avatar.
    saved: bool,
    // Group previews lead with the sender ("Dad: …", "You: …").
    preview_sender: Option<String>,
    // Images for custom emoji in the preview.
    preview_emoji: super::sticker_playback::PreviewEmoji,
    // Telegram Desktop marks groups, channels and bots before the title.
    kind_icon: Option<IconName>,
    // Subsection tabs: topic names for bots with topics and forums (the
    // sender line in 3-line rows, the preview line in 2-line rows).
    topic_names: Option<String>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let id = chat.id;
    let topic_line = topic_names.clone().filter(|_| row_style.preview_lines < 3);
    let title: String = if saved {
        "Saved Messages".to_string()
    } else {
        chat.title.clone()
    };
    // Slice CL2: cloned for the pin-drag ghost (the row itself moves
    // `title` below).
    let drag_title = title.clone();
    // A draft leads with a red "Draft:" (and a reply mark); anything else
    // is the plain preview line.
    let (preview, draft): (String, Option<bool>) = match chat.sidebar_line() {
        SidebarLine::Text(text) => (text, None),
        SidebarLine::Draft(draft) => (draft.text, Some(draft.reply)),
    };
    let activity = chat.peer_activity();
    // Slice chatlist-list-style: the icon/entities describe
    // `last_preview` only — draft/typing/activity lines render unstyled.
    // (When the shown text equals `last_preview` the entities describe
    // it even if the text arrived via the draft path.)
    let from_last = draft.is_none() && preview == chat.last_preview;
    let icon: Option<&str> = if row_style.media_icons && from_last {
        chat.last_preview_style.icon
    } else {
        None
    };
    // Custom emoji are content, not formatting: they show even when the
    // rich preview (bold, italic…) is off.
    let custom_only: Vec<TextEntity>;
    let entities: &[TextEntity] = if !from_last {
        &[]
    } else if row_style.rich_preview {
        &chat.last_preview_style.entities
    } else {
        custom_only = chat
            .last_preview_style
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
    let unread_indicator = chat_unread_indicator(chat);
    let has_reactions = chat.unread_reaction_count > 0;
    // kit Phase 7: screen-reader label for the row.
    let row_label = if draft.is_some() {
        format!("{title} — Draft: {preview}")
    } else {
        format!("{title} — {preview}")
    };
    // kit Phase 3: tag chips via the shared helper — the row height
    // (declared to the `VirtualList`) is derived from the same list.
    let tags = chat_row_tags(chat, folders, show_tags);
    div()
        .id(("chat-row", id.0 as u64))
        .w_full()
        .px_2()
        .role(Role::Button)
        .aria_label(row_label)
        // kit Phase 3: fixed height (see `chat_row_height`) — the
        // `VirtualList` positions rows from declared sizes, so the row
        // enforces the same height and centers its content. Title and
        // preview truncate to one line so content can never overflow it.
        .h(chat_row_height(&tags, row_style.preview_lines))
        .flex()
        .flex_col()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .when(!selected, |this| this.pressable(cx.theme()))
        .aria_selected(selected)
        .when(selected, |this| {
            this.hover(|s| s.bg(cx.theme().primary.opacity(0.3)))
                .active(|s| s.bg(cx.theme().primary.opacity(0.4)))
        })
        .border_l_2()
        .border_color(if selected {
            cx.theme().primary
        } else {
            transparent_black()
        })
        .bg(if selected {
            cx.theme().primary.opacity(0.25)
        } else {
            cx.theme().sidebar
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            // Slice CL3: in multi-select mode a row click toggles the
            // check instead of opening the chat.
            if selecting {
                this.toggle_chat_selected(id, cx);
            } else {
                this.select_listed_chat(id, window, cx);
            }
        }))
        // Slice CL1: right-click opens the chat-row context menu at the
        // click position (window coordinates).
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                this.chat_menu = Some(ChatMenuState {
                    chat_id: id,
                    position: event.position,
                });
                cx.notify();
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
                this.preview_press = None;
            }),
        )
        // Release outside the row (e.g. press, drag off, let go) also
        // cancels the pending long press.
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, _, _, _| {
                this.preview_press = None;
            }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                // Parity slice: circular chat photo or colored initials for
                // every chat-list row / chat type.
                // Slice CL3: the select-mode check circle precedes the
                // avatar while multi-select is active.
                .when(selecting, |this| this.child(select_check(id, checked)))
                .child(with_presence_dot_scaled(
                    if saved {
                        saved_messages_avatar(CHAT_ROW_AVATAR)
                    } else {
                        chat_avatar(&title, photo_path, CHAT_ROW_AVATAR).into_any_element()
                    },
                    fx.online,
                    13.,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .min_w_0()
                        .flex_1()
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
                                            this.child(row_glyph(icon, cx.theme().muted_foreground))
                                        })
                                        .child(
                                            div().font_semibold().min_w_0().truncate().child(title),
                                        )
                                        .when_some(title_badge, |this, badge| {
                                            this.child(title_badge_element(
                                                badge,
                                                badge_emoji.clone(),
                                                cx,
                                            ))
                                        })
                                        .when(chat.is_muted(), |this| {
                                            this.child(row_glyph(
                                                IconName::BellOff,
                                                cx.theme().muted_foreground,
                                            ))
                                        })
                                        .when(chat.is_forum_chat(), |this| {
                                            this.child(forum_badge(id, cx))
                                        }),
                                )
                                .when_some(row_stamp(chat), |this, (status, stamp)| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .items_center()
                                            .gap_0p5()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .when(status != RowStatus::None, |this| {
                                                this.child(row_status_mark(status, cx))
                                            })
                                            .child(stamp),
                                    )
                                }),
                        )
                        .when(row_style.preview_lines >= 3, |this| {
                            // Slice chatlist-list-style: the third line
                            // names the sender ("You" / author signature /
                            // chat title).
                            this.child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        topic_names
                                            .unwrap_or_else(|| chat.last_preview_sender.clone()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
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
                                                .filter(|_| from_last && topic_line.is_none()),
                                            |this, sender| {
                                                this.child(
                                                    div()
                                                        .flex_none()
                                                        .text_xs()
                                                        .text_color(accent())
                                                        .child(format!("{sender}: ")),
                                                )
                                            },
                                        )
                                        .when_some(
                                            chat.last_preview_thumb
                                                .clone()
                                                .filter(|_| from_last && topic_line.is_none()),
                                            |this, mini| {
                                                this.child(
                                                    img(ImageSource::Image(std::sync::Arc::new(
                                                        gpui_kit::Image::from_bytes(
                                                            gpui_kit::ImageFormat::Jpeg,
                                                            mini.data.clone(),
                                                        ),
                                                    )))
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
                                                .text_color(cx.theme().primary)
                                                .child(
                                                    super::activity_indicator::activity_indicator(
                                                        line.indicator,
                                                        cx.theme().primary,
                                                        format!("row-activity-{}", id.0).into(),
                                                    ),
                                                )
                                                .child(div().min_w_0().truncate().child(line.text))
                                                .into_any_element(),
                                            None if topic_line.is_some() => div()
                                                .min_w_0()
                                                .flex_1()
                                                .text_xs()
                                                .truncate()
                                                .child(topic_line.unwrap_or_default())
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
                                                        cx,
                                                    ),
                                                )
                                                .into_any_element(),
                                        }),
                                )
                                // Slice CL3: TGX order — ♥ reactions, @ mentions,
                                // then the unread counter at the trailing edge.
                                .child(
                                    div()
                                        .flex()
                                        .flex_none()
                                        .items_center()
                                        .gap_1()
                                        .when(has_reactions, |this| {
                                            this.child(reaction_badge(chat.is_muted()))
                                        })
                                        .when(has_mentions, |this| this.child(mention_badge()))
                                        .map(|this| match unread_indicator {
                                            Some((count, dot)) => this.child(unread_pill(
                                                id,
                                                count,
                                                dot,
                                                chat.is_muted(),
                                                fx.badge,
                                                cx,
                                            )),
                                            None if pinned_here(chat, archived)
                                                && !has_mentions
                                                && !has_reactions =>
                                            {
                                                this.child(row_glyph(
                                                    IconName::Pin,
                                                    cx.theme().muted_foreground,
                                                ))
                                            }
                                            None => this,
                                        }),
                                ),
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
                        .children(tags.into_iter().map(|name| {
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(cx.theme().accent.opacity(0.12))
                                .text_xs()
                                .child(name)
                        })),
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
                title: drag_title.clone(),
            };
            let target = id;
            this.cursor_move()
                .on_drag(drag, |drag: &PinnedChatDrag, _, _, cx| {
                    cx.new(|_| drag.clone())
                })
                .on_drop(cx.listener(move |this, drag: &PinnedChatDrag, _, cx| {
                    this.drop_pinned_chat(drag.chat_id, drag.archived, target, cx);
                }))
        })
}

/// Slice CL2: drag payload for pinned-chat reorder. It renders itself
/// as the drag ghost (the chat title on a highlighted chip).
#[derive(Clone)]
pub(super) struct PinnedChatDrag {
    pub(super) chat_id: ChatId,
    pub(super) archived: bool,
    pub(super) title: String,
}

impl Render for PinnedChatDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().primary.opacity(0.25))
            .border_1()
            .border_color(cx.theme().border)
            .text_sm()
            .font_medium()
            .child(self.title.clone())
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
    with_presence_dot_scaled(avatar, if online { 1. } else { 0. }, dot, cx)
}

/// [`with_presence_dot`] with the dot at `scale` (0 hidden, 1 full): the
/// 150 ms grow / shrink Telegram Desktop plays when a user goes online or
/// offline. The dot scales about its centre inside a fixed box, so the
/// avatar's layout never moves.
pub(super) fn with_presence_dot_scaled(
    avatar: impl IntoElement,
    scale: f32,
    dot: f32,
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
                            .bg(cx.theme().sidebar)
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
const CHAT_ROW_AVATAR: f32 = 46.;

/// Small inline icon in a chat row (lock, muted bell, pin, receipts).
fn row_glyph(name: IconName, color: Hsla) -> impl IntoElement {
    Icon::new(name).size(px(14.)).flex_none().text_color(color)
}

/// Whether the chat is pinned in the list this row belongs to.
fn pinned_here(chat: &ChatSummary, archived: bool) -> bool {
    if archived {
        chat.archive_is_pinned
    } else {
        chat.is_pinned
    }
}

/// Trailing title-line stamp: `(status mark for an outgoing last
/// message, local time/day label)`, or `None` for an empty chat.
fn row_stamp(chat: &ChatSummary) -> Option<(RowStatus, String)> {
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
fn row_status_mark(status: RowStatus, cx: &App) -> AnyElement {
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
pub(super) fn chat_unread_indicator(chat: &ChatSummary) -> Option<(i32, bool)> {
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
fn draft_prefix(reply: bool, bare: bool) -> impl IntoElement {
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
fn title_badge_element(
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
fn unread_pill(
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
fn forum_badge(chat_id: ChatId, cx: &App) -> impl IntoElement {
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
pub(super) fn unread_badge(anchor: AnyElement, count: i32, dot: bool) -> AnyElement {
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
fn mention_badge() -> impl IntoElement {
    // Telegram Desktop: a bare "@" in the accent color, no badge.
    Icon::new(IconName::AtSign)
        .size(px(16.))
        .text_color(accent_strong())
        .into_any_element()
}

/// The unread-reaction mark: Telegram Desktop draws a bare filled heart,
/// red, or grey when the chat is muted (`dialogsUnreadReaction`).
fn reaction_badge(muted: bool) -> impl IntoElement {
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
fn select_check(chat_id: ChatId, checked: bool) -> impl IntoElement {
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
