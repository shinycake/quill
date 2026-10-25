//! Methods moved out of `chat_row.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::chat_list_keys::ListRow;
use std::rc::Rc;

/// What the chat rows need to draw folder tag chips: built once per list
/// render (with the item list it belongs to) and shared by its rows.
#[derive(Default)]
pub(in crate::ui) struct FolderTagContext {
    /// `(id, chip text, colour)` of the folders (`quill::folder_limits::tag_names`).
    pub(in crate::ui) names: Vec<(i32, String, i32)>,
    /// Tags are on (`are_folder_tags_enabled`).
    pub(in crate::ui) show: bool,
}

impl QuillApp {
    /// The folder chip names plus whether tags are on.
    pub(in crate::ui) fn folder_tag_context(&self) -> Rc<FolderTagContext> {
        Rc::new(
            self.session()
                .map_or_else(Default::default, |s| FolderTagContext {
                    names: quill::folder_limits::tag_names(&s.chat_list.chat_folders),
                    show: s.chat_list.are_folder_tags_enabled,
                }),
        )
    }

    /// kit Phase 3: resolve one virtualized chat-list item to its element.
    /// Only visible indices are built, once per frame — selection, badges,
    /// pin-drag, multi-select and context-menu behavior are unchanged.
    pub(in crate::ui) fn chat_list_item_element(
        &mut self,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = self.chat_list.items.get(ix).cloned();
        match item {
            Some(ChatListItem::ArchiveRow { .. }) => self.archive_row_element(cx),
            Some(ChatListItem::ArchiveBar) => self.archive_bar_element(cx),
            Some(ChatListItem::StoryStrip) => self.story_strip_element(cx),
            Some(ChatListItem::Chat {
                id,
                archived,
                height,
            }) => {
                let Some(chat) = self.session().and_then(|s| s.chats.get(&id.0)) else {
                    return div().into_any_element();
                };
                let open = self.session().and_then(|s| s.open_chat);
                let selected = open == Some(chat.id);
                // Parity slice: folder names + tags flag for chat-row chips,
                // built with the item list this row belongs to.
                let folder_tags = Rc::clone(&self.chat_list.folder_tags);
                // Parity slice: chat photos resolve per visible row and are
                // sandboxed before display.
                let media_roots = self.media_display_roots();
                let photo = self
                    .session()
                    .and_then(|s| s.chat_photo_path(chat.id))
                    .and_then(|path| sandboxed_display_path(path, &media_roots));
                // Slice CL3: multi-select mode — rows toggle the check
                // instead of opening the chat.
                let selecting = self.rows_selecting();
                let checked = selecting && self.chat_list.selected.contains(&chat.id.0);
                // Slice CL2: pin drag runs only on the unfiltered list with
                // at least two pinned chats (TGX `ChatsAdapter`); the
                // archive has its own pinned set.
                // (Pinned flag first: looking for a second pinned chat walks
                // the chats, so only pinned rows pay for it.)
                let draggable = if archived {
                    chat.archive_is_pinned
                        && self.chat_list.filter == ChatListFilter::Archived
                        && self.session().is_some_and(|s| s.has_two_pinned_chats(true))
                } else {
                    chat.is_pinned
                        && self.chat_list.filter == ChatListFilter::All
                        && self.folders.tab.is_none()
                        && self
                            .session()
                            .is_some_and(|s| s.has_two_pinned_chats(false))
                };
                // The activity indicator is clock-driven: keep ticking while
                // a visible row shows one (the chat list's animation layer
                // ticks its own).
                if chat.peer_activity().is_some() && super::super::anim_layer::current().is_none() {
                    self.request_animation_tick(super::super::activity_indicator::FPS, cx);
                }
                let saved = self.session().is_some_and(|s| s.is_saved_messages(chat.id));
                let online = !saved && self.session().is_some_and(|s| s.chat_peer_online(chat));
                // Online dot and new-badge animations: 150 ms, only while
                // the window is active (inactive windows snap).
                let active = self.frame.window_active.get();
                let forum = self.session().and_then(|s| s.chat_row_forum(chat.id));
                let has_badge = match &forum {
                    Some(forum) => forum.unread_topics > 0,
                    None => chat_unread_indicator(chat).is_some(),
                };
                let fx = self.frame.row_fx.borrow_mut().observe(
                    chat.id.0,
                    online,
                    has_badge,
                    active,
                    std::time::Instant::now(),
                );
                let forum = forum.map(|forum| {
                    let icons: Vec<i64> = forum
                        .topics
                        .iter()
                        .map(|t| t.icon)
                        .filter(|icon| *icon > 0)
                        .collect();
                    let emoji = if icons.is_empty() {
                        Default::default()
                    } else {
                        self.topic_icon_images(&icons, cx)
                    };
                    super::topic_titles::ForumLine { forum, emoji }
                });
                if fx.animating {
                    self.request_animation_tick(60, cx);
                }
                let title_badge = if saved {
                    None
                } else {
                    self.session().and_then(|s| s.chat_title_badge(chat))
                };
                // Status emoji: two loops in the animation layer, then
                // the first frame (`status_emoji`).
                let badge_emoji = match title_badge {
                    Some(TitleBadge::EmojiStatus(id)) => self.status_glyph(
                        crate::ui::status_emoji::StatusSurface::ListRow,
                        chat.id.0,
                        id,
                        cx,
                    ),
                    _ => None,
                };
                // Peers with active stories get the ring (tdesktop
                // `storiesPeer`: users and channels). Clicking the row
                // opens the chat, as in tdesktop's chat list.
                let story_ring = matches!(
                    chat.kind,
                    ChatKind::Private { .. } | ChatKind::Supergroup { .. }
                )
                .then(|| self.session().and_then(|s| s.chat_story_ring(chat.id.0)))
                .flatten();
                let muted_fg = cx.theme().muted_foreground;
                let video = self.chat_row_userpic(
                    chat.id,
                    &chat.kind,
                    selected,
                    saved,
                    CHAT_ROW_AVATAR,
                    cx,
                );
                // Swipe action for this row (none while selecting).
                let swipe_label = if selecting {
                    quill::chat_swipe::SwipeLabel::Disabled
                } else {
                    self.chat_swipe_label(chat)
                };
                let pin_slide = self.chat_list.pin_reorder.as_ref().map(|r| {
                    (
                        r.offset(chat.id.0, std::time::Instant::now()),
                        r.dragging() == Some(chat.id.0),
                    )
                });
                let row = session_chat_row(
                    chat,
                    selected,
                    &folder_tags.names,
                    self.folders.tab,
                    folder_tags.show,
                    photo.as_deref(),
                    draggable,
                    archived,
                    selecting,
                    checked,
                    fx,
                    title_badge,
                    badge_emoji,
                    self.bot_verify_icon_path(chat),
                    // Slice chatlist-list-style: Settings → Appearance.
                    ChatListRowStyle::new(
                        self.session().map_or(self.appearance.preview_lines, |s| {
                            s.chat_row_lines(chat.id, self.appearance.preview_lines)
                        }),
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
                    forum,
                    story_ring,
                    video,
                    self.session().and_then(|s| {
                        let promotion = quill::promotion::top_promotion(s, chat)?;
                        Some(s.chat_list.psa_labels.row(promotion))
                    }),
                    muted_fg,
                    cx,
                );
                let row = self
                    .chat_list_nav_row(row, ListRow::Chat(id), selected, cx)
                    .into_any_element();
                let row = self.chat_swipe_wrap(row, id, swipe_label, height, cx);
                let row = self.drop_row_wrap(row, id, cx);
                match pin_slide {
                    // A row shifted by the pinned drag: the dragged row
                    // follows the pointer above its neighbours, the
                    // displaced ones slide home.
                    Some((dy, dragging)) if dragging || dy != 0. => {
                        let shifted = div().relative().top(px(dy)).w_full().child(row);
                        if dragging {
                            deferred(shifted).priority(10).into_any_element()
                        } else {
                            shifted.into_any_element()
                        }
                    }
                    _ => row,
                }
            }
            None => div().into_any_element(),
        }
    }
}
