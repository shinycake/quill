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
use quill::state::{ChatSummary, Session};
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
    // Boxed: `ChatSummary` is large; the other variants are tiny
    // (clippy `large_enum_variant`).
    Chat {
        chat: Box<ChatSummary>,
        archived: bool,
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

pub(super) fn chat_list_caption(
    mode: PaneMode,
    session: Option<&Session>,
    folder: Option<i32>,
) -> SharedString {
    match mode {
        PaneMode::Synthetic => "Synthetic".into(),
        PaneMode::Connecting => "Waiting for Ready".into(),
        PaneMode::Ready => {
            if session.is_some_and(|s| s.search.open) {
                if session.is_some_and(|s| s.search.recents) {
                    "Recent".into()
                } else {
                    "Search".into()
                }
            } else if let (Some(session), Some(folder_id)) = (session, folder) {
                // Phase 7.1: a selected folder tab names the caption.
                let name = session.folder_name(folder_id).unwrap_or("Folder");
                let n = session.ordered_folder_chats(folder_id).len();
                format!("{name} · {n}").into()
            } else {
                let n = session.map(|s| s.ordered_chats().len()).unwrap_or(0);
                format!("Main list · {n}").into()
            }
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
        .px_2()
        .py_2()
        .rounded_md()
        .role(Role::Button)
        .aria_label(title)
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
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

/// Parity slice: one-line description snippet for the channel/supergroup
/// header.
pub(super) fn description_snippet(description: &str, max_chars: usize) -> String {
    let one_line: String = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        return one_line;
    }
    let truncated: String = one_line.chars().take(max_chars).collect();
    format!("{truncated}…")
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
            Some(ChatListItem::Chat { chat, archived }) => {
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
                session_chat_row(
                    &chat,
                    selected,
                    &folder_names,
                    show_folder_tags,
                    photo.as_deref(),
                    draggable,
                    archived,
                    selecting,
                    checked,
                    // Slice chatlist-list-style: Settings → Appearance.
                    ChatListRowStyle::new(
                        self.appearance.preview_lines,
                        self.appearance.chat_list_media_icons,
                        self.appearance.chat_list_rich_preview,
                    ),
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
    // Slice chatlist-list-style: preview line count, media icons, and
    // formatted preview (Settings → Appearance → Chat list rows).
    row_style: ChatListRowStyle,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let id = chat.id;
    let title = chat.title.clone();
    // Slice CL2: cloned for the pin-drag ghost (the row itself moves
    // `title` below).
    let drag_title = title.clone();
    let preview = chat.sidebar_preview();
    // Slice chatlist-list-style: the icon/entities describe
    // `last_preview` only — draft/typing/activity lines render unstyled.
    // (When the shown text equals `last_preview` the entities describe
    // it even if the text arrived via the draft path.)
    let from_last = preview == chat.last_preview;
    let icon: Option<&str> = if row_style.media_icons && from_last {
        chat.last_preview_style.icon
    } else {
        None
    };
    let entities: &[TextEntity] = if row_style.rich_preview && from_last {
        &chat.last_preview_style.entities
    } else {
        &[]
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
    let unread_indicator = if chat.unread_count == 0 && chat.is_marked_as_unread {
        Some((0, true))
    } else if has_mentions && chat.unread_count == 1 {
        None
    } else if chat.unread_count > 0 {
        Some((chat.unread_count, false))
    } else {
        None
    };
    let has_reactions = chat.unread_reaction_count > 0;
    // kit Phase 7: screen-reader label for the row.
    let row_label = format!("{title} — {preview}");
    // kit Phase 3: tag chips via the shared helper — the row height
    // (declared to the `VirtualList`) is derived from the same list.
    let tags = chat_row_tags(chat, folders, show_tags);
    div()
        .id(("chat-row", id.0 as u64))
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
        .pressable(cx.theme())
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
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
                .gap_2()
                // Parity slice: circular chat photo or colored initials for
                // every chat-list row / chat type.
                // Slice CL3: the select-mode check circle precedes the
                // avatar while multi-select is active.
                // kit Phase 4: the unread badge overlays the avatar.
                .when(selecting, |this| this.child(select_check(id, checked)))
                .child({
                    let avatar = chat_avatar(&title, photo_path, 40.).into_any_element();
                    match unread_indicator {
                        Some((count, dot)) => unread_badge(avatar, count, dot),
                        None => avatar,
                    }
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .min_w_0()
                                        .child(
                                            div().font_medium().min_w_0().truncate().child(title),
                                        )
                                        .when(chat.is_muted(), |this| this.child(muted_badge(id)))
                                        .when(chat.is_forum_chat(), |this| {
                                            this.child(forum_badge(id))
                                        })
                                        // Phase B1: lock indicator for
                                        // secret chats (E2E).
                                        .when(
                                            matches!(chat.kind, ChatKind::Secret { .. }),
                                            |this| this.child(secret_badge(id)),
                                        ),
                                )
                                // Slice CL3: TGX draws the @ mention badge and
                                // ♥ reaction badge right-to-left; the flex
                                // row renders them left-to-right in the
                                // same order. kit Phase 4: the unread
                                // counter now overlays the avatar instead.
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .when(has_reactions, |this| {
                                            this.child(reaction_badge(chat.is_muted()))
                                        })
                                        .when(has_mentions, |this| this.child(mention_badge())),
                                ),
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
                                    .child(chat.last_preview_sender.clone()),
                            )
                        })
                        .child(super::chatlist_style::chat_list_preview_line(
                            icon, &preview, entities, cx,
                        )),
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
            .bg(cx.theme().accent.opacity(0.15))
            .border_1()
            .border_color(cx.theme().border)
            .text_sm()
            .font_medium()
            .child(self.title.clone())
    }
}

fn muted_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("muted-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg_badge_muted())
        .text_color(text_on_fill())
        .text_xs()
        .font_semibold()
        .child("Muted")
}

/// Phase B1: lock indicator for secret chats in the chat list (E2E).
fn secret_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("secret-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(accent_strong())
        .text_color(text_on_fill())
        .text_xs()
        .font_semibold()
        .child("🔒")
}

/// Phase 5.1: forum indicator for forum supergroups in the chat list.
fn forum_badge(chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("forum-badge", chat_id.0 as u64))
        .h(px(18.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg_premium())
        .text_color(text_on_fill())
        .text_xs()
        .font_semibold()
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
    Badge::new()
        .icon(Icon::new(IconName::AtSign))
        .color(accent_strong())
        .child(div().size(px(16.)).into_any_element())
        .into_any_element()
}

/// Slice CL3 / kit Phase 4: the ♥ reaction badge (TGX `TGChat.reactionsCounter`
/// — heart badge, dimmed when the chat is muted, shown when
/// `unread_reaction_count > 0`) as a kit `Badge` (Icon variant) on a
/// 16px anchor.
fn reaction_badge(muted: bool) -> impl IntoElement {
    let color = if muted {
        bg_badge_muted()
    } else {
        accent_strong()
    };
    Badge::new()
        .icon(Icon::new(IconName::Heart))
        .color(color)
        .child(div().size(px(16.)).into_any_element())
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
