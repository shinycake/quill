//! Methods moved out of `chat_row.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// `(id, name, colour)` of the folders plus whether tags are on.
    pub(in crate::ui) fn folder_tag_context(&self) -> (Vec<(i32, String, i32)>, bool) {
        self.session()
            .map(|s| {
                (
                    s.chat_list
                        .chat_folders
                        .iter()
                        .map(|f| (f.id, f.name.clone(), f.color_id))
                        .collect::<Vec<_>>(),
                    s.chat_list.are_folder_tags_enabled,
                )
            })
            .unwrap_or_default()
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
                // Parity slice: folder names + tags flag for chat-row chips.
                let (folder_names, show_folder_tags) = self.folder_tag_context();
                // Parity slice: chat photos resolve per visible row and are
                // sandboxed before display.
                let media_roots = self.media_display_roots();
                let photo = self
                    .session()
                    .and_then(|s| s.chat_photo_path(chat.id))
                    .and_then(|path| sandboxed_display_path(path, &media_roots));
                // Slice CL3: multi-select mode — rows toggle the check
                // instead of opening the chat.
                let selecting = !self.chat_list.selected.is_empty();
                let checked = selecting && self.chat_list.selected.contains(&chat.id.0);
                // Slice CL2: pin drag runs only on the unfiltered list with
                // at least two pinned chats (TGX `ChatsAdapter`); the
                // archive has its own pinned set.
                // (Pinned flag first: counting the pinned chats walks every
                // chat, so only pinned rows pay for it.)
                let draggable = if archived {
                    chat.archive_is_pinned
                        && self.chat_list.filter == ChatListFilter::Archived
                        && self
                            .session()
                            .is_some_and(|s| s.pinned_chat_ids(true).len() >= 2)
                } else {
                    chat.is_pinned
                        && self.chat_list.filter == ChatListFilter::All
                        && self.folders.tab.is_none()
                        && self
                            .session()
                            .is_some_and(|s| s.pinned_chat_ids(false).len() >= 2)
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
                let fx = self.frame.row_fx.borrow_mut().observe(
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
                    &folder_names,
                    self.folders.tab,
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
                    story_ring,
                    muted_fg,
                    cx,
                )
                .into_any_element();
                let row = self.chat_swipe_wrap(row, id, swipe_label, height, cx);
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
