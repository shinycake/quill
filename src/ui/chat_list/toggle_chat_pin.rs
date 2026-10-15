//! Methods moved out of `chat_list.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Slice CL1: Pin / Unpin from the chat-row context menu
    /// (`toggleChatIsPinned`, schema 1.8.67, line 13678). The pin-limit
    /// pre-check mirrors TGX `ChatsController`: count pinned chats of
    /// the same secrecy class in the target list against
    /// `pinned_chat_count_max` / `pinned_archived_chat_count_max`
    /// (`updateOption`); at the limit the request is never sent and the
    /// TGX message shows instead.
    pub(in crate::ui) fn toggle_chat_pin(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let session = match self.session() {
            Some(session) => session,
            None => return,
        };
        let chat = match session.chats.get(&chat_id.0) {
            Some(chat) => chat,
            None => return,
        };
        let archived = chat.in_archive;
        let pinned = if archived {
            chat.archive_is_pinned
        } else {
            chat.is_pinned
        };
        let secret = matches!(chat.kind, ChatKind::Secret { .. });
        if !pinned {
            let limit = if archived {
                session.chat_list.pinned_archived_chat_count_max
            } else {
                session.chat_list.pinned_chat_count_max
            };
            let pinned_count = session
                .chats
                .values()
                .filter(|c| {
                    let is_pinned = if archived {
                        c.in_archive && c.archive_is_pinned
                    } else {
                        c.in_main_list && c.is_pinned
                    };
                    is_pinned && matches!(c.kind, ChatKind::Secret { .. }) == secret
                })
                .count() as i32;
            if pinned_count >= limit.max(0) {
                // tdesktop `PinsLimitBox`: the limit box, with Premium's
                // higher limit as the way out.
                self.show_premium_limit(
                    if archived {
                        quill::premium_limits::PremiumLimitKind::PinsArchive
                    } else {
                        quill::premium_limits::PremiumLimitKind::Pins
                    },
                    0,
                    cx,
                );
                return;
            }
        }
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.toggle_chat_pin(chat_id, !pinned);
            self.connection.status_note = match result {
                Ok(Some(_)) if pinned => tr(keys::Q_CHATLIST_UNPINNING).into(),
                Ok(Some(_)) => tr(keys::Q_CHATLIST_PINNING).into(),
                Ok(None) => tr(keys::Q_CHATLIST_PIN_REQUEST_ALREADY_IN_FLIGHT).into(),
                Err(_) => tr(keys::Q_CHATLIST_COULD_NOT_CHANGE_PIN).into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_pin(chat_id, !pinned, archived);
            self.connection.status_note = if pinned {
                "unpinned".into()
            } else {
                "pinned".into()
            };
            cx.notify();
        }
    }

    /// Slice CL1: Mark as read / Mark as unread from the chat-row
    /// context menu. Mark-as-read follows Telegram X
    /// (`Tdlib.markChatAsRead` with `MessageSourceChatList`):
    /// `viewMessages` over the newest known message reads real unread
    /// history, plus `toggleChatIsMarkedAsUnread(false)` clears the
    /// manual marker. Mark-as-unread is the plain toggle (TGX only
    /// offers it when `unreadCount == 0`, which the menu label gates).
    pub(in crate::ui) fn toggle_chat_marked_as_unread(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        let marked = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.is_marked_as_unread || chat.unread_count > 0);
        // Slice CL1 review nit: server-side unread with no cached history
        // can't be marked read (the driver has nothing to view and sends
        // nothing); say so honestly instead of "request already in flight".
        let nothing_to_view = self.session().is_some_and(|session| {
            session
                .chats
                .get(&chat_id.0)
                .is_some_and(|c| c.unread_count > 0 && !c.is_marked_as_unread)
                && !session
                    .histories
                    .get(&chat_id.0)
                    .is_some_and(|h| !h.messages.is_empty())
        });
        if marked && nothing_to_view && self.live.is_some() {
            self.connection.status_note = tr(keys::Q_CHATLIST_OPEN_THE_CHAT_TO_MARK).into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let result = if marked {
                live.driver.mark_chat_as_read(chat_id)
            } else {
                live.driver.toggle_chat_marked_as_unread(chat_id, true)
            };
            self.connection.status_note = match result {
                Ok(Some(_)) if marked => tr(keys::Q_CHATLIST_MARKING_AS_READ).into(),
                Ok(Some(_)) => tr(keys::Q_CHATLIST_MARKING_AS_UNREAD).into(),
                Ok(None) => tr(keys::Q_CHATLIST_REQUEST_ALREADY_IN_FLIGHT).into(),
                Err(_) => tr(keys::Q_CHATLIST_COULD_NOT_CHANGE_READ_STATE).into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_marked_as_unread(chat_id, !marked);
            self.connection.status_note = if marked {
                tr(keys::Q_CHATLIST_MARKED_AS_READ).into()
            } else {
                tr(keys::Q_CHATLIST_MARKED_AS_UNREAD).into()
            };
            cx.notify();
        }
    }

    pub(in crate::ui) fn sidebar(
        &mut self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.pane_mode();
        let mut list = div()
            .id("sidebar")
            .when(mode != PaneMode::Ready, |this| this.overflow_y_scroll())
            .track_focus(&self.focus_sidebar)
            .w(self.frame.sidebar_width)
            .flex_none()
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(mode == PaneMode::Ready, |this| {
                        this.child(self.main_navigation_menu(cx))
                    })
                    .when(
                        mode == PaneMode::Ready && self.account.passcode.enabled,
                        |this| this.child(self.lock_button(cx)),
                    )
                    .child(self.list_tabs(cx))
                    .children(self.proxy_shield_button(cx)),
            )
            .when_some(
                (!self.chat_list.contacts_tab_open && !self.chat_list.calls_tab_open)
                    .then(|| chat_list_caption(mode, self.session()))
                    .flatten(),
                |this, caption| {
                    this.child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(caption),
                    )
                },
            );
        match mode {
            PaneMode::Synthetic => {
                list = list
                    .child(static_chat_row(
                        tr(keys::Q_CHATLIST_ADA_LOVELACE),
                        tr(keys::Q_CHATLIST_MIXED_HEIGHT_HISTORY),
                        true,
                        cx,
                    ))
                    .child(static_chat_row(
                        tr(keys::Q_CHATLIST_RTL_EMOJI_SAMPLES),
                        "שלום 👨‍👩‍👧‍👦",
                        false,
                        cx,
                    ));
            }
            PaneMode::Connecting => {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_CHATLIST_NO_CHAT_LIST_UNTIL_READY)),
                );
            }
            PaneMode::Ready => {
                if self.chat_list.contacts_tab_open {
                    list = list.child(self.contacts_panel(cx));
                } else if self.chat_list.calls_tab_open {
                    list = list.child(
                        div()
                            .id("calls-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.calls_list(cx)),
                    );
                } else {
                    // Searching spans every chat: the folder tabs step aside.
                    if !self.search_is_open() {
                        list = list.children(self.suggestion_card(cx));
                        list = list.child(self.folder_tabs_with_community_banner(cx));
                        // A shared folder whose owner added chats.
                        list = list.children(self.folder_new_chats_bar(cx));
                    }
                    // Stories strip: its tiles, and the collapsed stack
                    // that takes its place beside the search field once
                    // the list scrolls the strip away.
                    self.refresh_story_tiles();
                    let story_stack = if self.search_is_open() {
                        None
                    } else {
                        self.story_compact_stack(cx)
                    };
                    list = list.child(self.sidebar_search_field(story_stack, cx));
                    if self.share.new_secret_picker_open {
                        list = list.child(self.new_secret_picker_panel(cx));
                    }
                    if self.search_is_open() {
                        list = list.child(self.search_results(cx));
                    } else {
                        let folder = self.folders.tab;
                        let filter = self.chat_list.filter;
                        // Parity slice: folder names + tags flag for chat-row
                        // chips.
                        let (folder_names, show_folder_tags) = self.folder_tag_context();
                        // Borrowed, never cloned: the list can hold thousands
                        // of chats and this runs every render.
                        let mut chats: Vec<&ChatSummary> = self
                            .session()
                            .map(|s| match folder {
                                Some(folder_id) => s.ordered_folder_chats(folder_id),
                                None => s.ordered_chats(),
                            })
                            .unwrap_or_default();
                        // Slice CL2: the Unread category filters the loaded
                        // model (TGX `ChatFilter` unread predicate:
                        // unread_count > 0 or marked-as-unread,
                        // `ChatFilter.java:166`) — never the server query.
                        if filter == ChatListFilter::Unread {
                            chats.retain(|c| c.is_unread());
                        }
                        // Slice CL2: the Archived category shows only the
                        // archive section.
                        let show_main_list = filter != ChatListFilter::Archived;
                        // Parity slice `parity:communities-chatlist-mode`: narrow
                        // `chats` to the community's chats when the mode is
                        // active. Membership is the cached
                        // `communityFullInfo.chats` pack (state keeps no
                        // per-chat `community_id`); `updateCommunityFullInfo`
                        // replaces the pack wholesale, so a chat leaving the
                        // community drops out on the next render. Hidden
                        // chats (`is_hidden`) stay included — see
                        // `community_mode::community_member_ids`.
                        if let (ChatListFilter::Community(community_id), Some(session)) =
                            (filter, self.session())
                        {
                            community_mode::retain_community_chats(
                                &mut chats,
                                Some(community_id),
                                &session.groups.community_full_infos,
                            );
                        }
                        // Slice CL3: multi-select mode — rows toggle the
                        // check instead of opening the chat. Defined
                        // once here so the select bar, the main loop,
                        // and the archive loop all see it.
                        let selecting = !self.chat_list.selected.is_empty();
                        // Slice CL3: multi-select action bar (TGX
                        // `ChatsController` selection header): the
                        // selected count, the bulk actions, and cancel.
                        if selecting {
                            let count = self.chat_list.selected.len();
                            list = list.child(
                                div()
                                    .id("select-bar")
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(cx.theme().accent.opacity(0.12))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(format!("{count} selected")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Button::new("select-pin")
                                                    .label(tr(keys::CONTEXT_PIN_MSG))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_pins(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-read")
                                                    .label(tr(keys::Q_CHATLIST_READ))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.mark_selected_read(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-mute")
                                                    .label(tr(keys::Q_CHATLIST_MUTE))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_mute(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-archive")
                                                    .label(tr(keys::Q_CHATLIST_ARCHIVE))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_selected_archive(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-unread")
                                                    .label(tr(keys::Q_CHATLIST_SELECT_UNREAD))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.select_unread_chats(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-delete")
                                                    .label(tr(keys::CHAT_LINK_DELETE))
                                                    .ghost()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.delete_selected_chats(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("select-cancel")
                                                    .label("✕")
                                                    .ghost()
                                                    .tooltip(tr(keys::Q_CHATLIST_EXIT_SELECTION))
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.exit_select_mode(cx);
                                                    })),
                                            ),
                                    ),
                            );
                        }
                        if show_main_list && chats.is_empty() {
                            let loading =
                                self.session().is_some_and(|s| !s.chat_list.chats_exhausted);
                            list = if loading && folder.is_none() {
                                // kit Phase 9: skeleton rows while the first
                                // chat batch is still loading.
                                let mut loading_list = list;
                                for i in 0..4 {
                                    loading_list = loading_list.child(chat_list_skeleton_row(i));
                                }
                                loading_list
                            } else {
                                let (glyph, title, hint) =
                                    if let ChatListFilter::Community(community_id) = filter {
                                        // Parity slice
                                        // `parity:communities-chatlist-mode`: a
                                        // missing pack means the fetch hasn't
                                        // landed yet (entering the mode fires
                                        // `getCommunityFullInfo` when live) —
                                        // "Loading…", like the community info
                                        // panel. An empty pack / no matching
                                        // loaded chats is the genuine empty
                                        // state, never a crash.
                                        if self.session().is_some_and(|s| {
                                            !s.groups
                                                .community_full_infos
                                                .contains_key(&community_id)
                                        }) {
                                            (
                                                "⏳",
                                                tr(keys::Q_CHATLIST_LOADING_COMMUNITY),
                                                tr(keys::Q_CHATLIST_FETCHING_THE_COMMUNITYS_CHATS),
                                            )
                                        } else {
                                            (
                                                "👥",
                                                tr(keys::Q_CHATLIST_NO_CHATS_IN_THIS_COMMUNITY),
                                                tr(keys::Q_CHATLIST_CHATS_ADDED_TO_THE_COMMUNITY),
                                            )
                                        }
                                    } else if filter == ChatListFilter::Unread {
                                        (
                                            "🔕",
                                            tr(keys::Q_CHATLIST_NO_UNREAD_CHATS),
                                            tr(keys::Q_CHATLIST_YOU_ARE_ALL_CAUGHT_UP),
                                        )
                                    } else if folder.is_some() {
                                        (
                                            "📁",
                                            tr(keys::Q_CHATLIST_NO_CHATS_IN_THIS_FOLDER),
                                            tr(keys::Q_CHATLIST_ADD_CHATS_TO_THE_FOLDER),
                                        )
                                    } else {
                                        (
                                            "💬",
                                            tr(keys::Q_CHATLIST_NO_CHATS_YET),
                                            tr(keys::Q_CHATLIST_START_A_CONVERSATION_TO_SEE),
                                        )
                                    };
                                list.child(chat_list_empty_state(glyph, title, hint, cx))
                            };
                        }
                        // kit Phase 3: the chat rows (main list + archive
                        // section) render through a kit `VirtualList` — only
                        // the visible window builds elements. `open`,
                        // photos, pin-drag and multi-select state resolve
                        // per visible row in `chat_list_item_element`.
                        // Rebuilt every render; the list below only reads it.
                        let row_height = |chat: &ChatSummary| {
                            chat_row_height(
                                &chat_row_tags(chat, &folder_names, show_folder_tags, folder),
                                self.appearance.preview_lines,
                            )
                        };
                        let mut items: Vec<ChatListItem> = Vec::with_capacity(chats.len() + 2);
                        // tdesktop: "Archived chats" is a pinned-top entry
                        // of the unfiltered main list, a full row or (with
                        // `archiveCollapsed`) a slim bar, and absent when
                        // `archiveInMainMenu` moved it to the menu.
                        // Opening it shows the archive (here: the Archived
                        // category). The archived chats themselves never
                        // sit inside the main list.
                        let has_archived = self
                            .session()
                            .is_some_and(|s| !s.ordered_archived_chats().is_empty());
                        // tdesktop's stories strip is the first row: it
                        // scrolls away with the list (and collapses).
                        if !self.stories.strip.tiles.is_empty() {
                            items.push(ChatListItem::StoryStrip);
                        }
                        match quill::chatlist_archive::row_mode(
                            has_archived,
                            self.appearance.archive_collapsed,
                            self.appearance.archive_in_main_menu,
                            filter == ChatListFilter::All
                                && folder.is_none()
                                && !self.search_is_open(),
                        ) {
                            quill::chatlist_archive::ArchiveRowMode::Row => {
                                items.push(ChatListItem::ArchiveRow {
                                    height: chat_row_height(&[], self.appearance.preview_lines),
                                });
                            }
                            quill::chatlist_archive::ArchiveRowMode::Collapsed => {
                                items.push(ChatListItem::ArchiveBar);
                            }
                            quill::chatlist_archive::ArchiveRowMode::Hidden => {}
                        }
                        if show_main_list {
                            // A live pinned drag shows its swapped order
                            // (tdesktop moves rows while dragging); stable,
                            // so unpinned rows keep their place.
                            if let Some(reorder) = self.chat_list.pin_reorder.as_ref()
                                && reorder.dragging().is_some()
                                && !self.chat_list.pin_reorder_archived
                            {
                                chats.sort_by_key(|chat| {
                                    reorder.position(chat.id.0).unwrap_or(usize::MAX)
                                });
                            }
                            for chat in chats {
                                items.push(ChatListItem::Chat {
                                    id: chat.id,
                                    archived: false,
                                    height: row_height(chat),
                                });
                            }
                        }
                        // Parity slice: folder chats page eagerly — the driver
                        // re-requests `loadChats(chatListFolder)` after each
                        // `ok` until a 404 marks the folder exhausted, the
                        // same pattern as the main list. No "Load more"
                        // button: paging is automatic, not user-triggered.
                        if folder.is_none()
                            && matches!(filter, ChatListFilter::Archived | ChatListFilter::Unread)
                        {
                            let mut archived: Vec<&ChatSummary> = self
                                .session()
                                .map(|s| s.ordered_archived_chats())
                                .unwrap_or_default();
                            if filter == ChatListFilter::Unread {
                                archived.retain(|c| c.is_unread());
                            } else if let Some(reorder) = self.chat_list.pin_reorder.as_ref()
                                && reorder.dragging().is_some()
                                && self.chat_list.pin_reorder_archived
                            {
                                archived.sort_by_key(|chat| {
                                    reorder.position(chat.id.0).unwrap_or(usize::MAX)
                                });
                            }
                            if archived.is_empty() && filter == ChatListFilter::Archived {
                                list = list.child(chat_list_empty_state(
                                    "📦",
                                    tr(keys::Q_CHATLIST_NO_ARCHIVED_CHATS),
                                    tr(keys::Q_CHATLIST_CHATS_YOU_ARCHIVE_STAY_HERE),
                                    cx,
                                ));
                            }
                            for chat in archived {
                                items.push(ChatListItem::Chat {
                                    id: chat.id,
                                    archived: true,
                                    height: row_height(chat),
                                });
                            }
                        }
                        // Pinned-drag animation: keep frames coming while
                        // rows slide, drop the state once everything settled.
                        if let Some(reorder) = self.chat_list.pin_reorder.as_ref() {
                            let now = std::time::Instant::now();
                            if reorder.settled(now)
                                || (!self.frame.window_active.get() && reorder.dragging().is_none())
                            {
                                self.chat_list.pin_reorder = None;
                            } else {
                                self.request_animation_tick(60, cx);
                            }
                        }
                        // kit Phase 3: hand the flat item list to the kit
                        // `VirtualList`. Item heights are fixed by
                        // construction (see `chat_row_height`), so declared
                        // sizes always match the rendered rows.
                        let sizes: Rc<Vec<ItemSize<Pixels>>> = Rc::new(
                            items
                                .iter()
                                .map(|item| {
                                    let height = match item {
                                        ChatListItem::Chat { height, .. } => *height,
                                        ChatListItem::ArchiveRow { height } => *height,
                                        ChatListItem::ArchiveBar => {
                                            px(quill::chatlist_archive::COLLAPSED_BAR_HEIGHT)
                                        }
                                        ChatListItem::StoryStrip => {
                                            px(quill::stories_strip::FULL_HEIGHT)
                                        }
                                    };
                                    ItemSize::new(px(0.), height)
                                })
                                .collect(),
                        );
                        self.chat_list.items = items;
                        list = list.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                // Pinned drag: the pointer position drives
                                // the live reorder; a release anywhere
                                // ends it.
                                .on_drag_move(cx.listener(
                                    |this, event: &DragMoveEvent<PinnedChatDrag>, _, cx| {
                                        let drag = event.drag(cx).clone();
                                        this.pin_drag_move(
                                            &drag,
                                            f32::from(event.event.position.y),
                                            cx,
                                        );
                                    },
                                ))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.finish_pin_drag(cx)),
                                )
                                .on_mouse_up_out(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.finish_pin_drag(cx)),
                                )
                                .child(
                                    v_virtual_list(
                                        cx.entity(),
                                        "chat-list",
                                        sizes,
                                        |this: &mut QuillApp, range, _window, cx| {
                                            range
                                                .map(|ix| this.chat_list_item_element(ix, cx))
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                    // The app-owned handle keeps scroll position
                                    // across re-renders (scroll restoration).
                                    .track_scroll(&self.chat_list.scroll)
                                    .flex_1()
                                    .min_h_0()
                                    .w_full(),
                                ),
                        );
                    }
                }
            }
        }
        if mode == PaneMode::Ready {
            if self.folder_rail_active() {
                return div()
                    .id("sidebar-with-folder-rail")
                    .flex()
                    .w(self.frame.sidebar_width + px(self.folder_rail_width()))
                    .flex_none()
                    .h_full()
                    .child(self.folder_rail(cx))
                    .child(list)
                    .into_any_element();
            }
            return list.into_any_element();
        }
        list = list
            .child(
                div()
                    .mt_4()
                    .font_semibold()
                    .child(tr(keys::Q_CHATLIST_AUTHORIZATION)),
            )
            .child(
                div()
                    .id("auth-title")
                    .role(Role::Heading)
                    .aria_label(auth.title)
                    .text_sm()
                    .child(auth.title),
            )
            .child(
                div()
                    .id("auth-explanation")
                    .role(Role::Label)
                    .aria_label(auth.body.clone())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(auth.body.clone()),
            );
        list = list.when(
            matches!(auth.action, quill::auth::AuthAction::Register) && self.live.is_some(),
            |this| this.child(self.registration_form(cx)),
        );
        list.when(
            matches!(auth.action, quill::auth::AuthAction::EnterEmail) && self.live.is_some(),
            |this| {
                this.child(
                    Textarea::new(&self.auth_ui.email_input)
                        .aria_label(tr(keys::Q_CHATLIST_EMAIL_ADDRESS))
                        .h(px(40.)),
                )
                .child(
                    Button::new("submit-login-email")
                        .label(tr(keys::Q_CHATLIST_SUBMIT_EMAIL))
                        .on_click(cx.listener(|this, _, window, cx| this.submit_email(window, cx))),
                )
            },
        )
        .when(show_phone, |this| {
            this.child(
                div()
                    .mt_2()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::Q_CHATLIST_PHONE)),
            )
            .child(
                Textarea::new(&self.auth_ui.phone_input)
                    .aria_label(tr(keys::Q_CHATLIST_PHONE_NUMBER))
                    .h(px(40.)),
            )
            .child(
                Button::new("submit-phone")
                    .label(tr(keys::Q_CHATLIST_SUBMIT_PHONE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_phone(window, cx);
                    })),
            )
        })
        .when(
            quill::auth::can_request_qr_login(&self.current_auth())
                && (self.live.is_some() || self.auth_ui.demo_inputs),
            |this| {
                this.child(
                    Button::new("qr-login")
                        .label(tr(keys::Q_CHATLIST_SIGN_IN_WITH_QR_CODE))
                        .ghost()
                        .disabled(self.session().is_some_and(|s| s.requests.has_auth_submit()))
                        .on_click(cx.listener(|this, _, _, cx| this.request_qr_login(cx))),
                )
            },
        )
        .when(show_code, |this| {
            this.child(
                div()
                    .mt_2()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::Q_CHATLIST_CODE)),
            )
            .child(
                Textarea::new(&self.auth_ui.code_input)
                    .aria_label(tr(keys::Q_CHATLIST_SIGN_IN_CODE))
                    .h(px(40.)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("submit-code")
                            .label(tr(keys::Q_CHATLIST_SUBMIT_CODE))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_code(window, cx);
                            })),
                    )
                    .child(
                        Button::new("resend-code")
                            .label(tr(keys::Q_CHATLIST_RESEND_CODE))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.resend_code(cx);
                            })),
                    ),
            )
        })
        .when(show_password, |this| {
            // Slice A10: password / recovery-code entry lives in
            // `ui/auth_recovery.rs` (kit-first, named module).
            this.child(self.auth_password_section(cx))
        })
        .when(show_qr, |this| {
            // Slice A1: the QR payload rides on the auth state (envelope.rs
            // carries `link` through); rendered here, never logged.
            let link = match self.current_auth() {
                AuthorizationState::WaitOtherDeviceConfirmation { link } => link,
                _ => String::new(),
            };
            let qr: AnyElement = match self.qr_login_image(&link) {
                Some(image) => img(ImageSource::from(image))
                    .w(px(200.))
                    .h(px(200.))
                    .aspect_ratio(px(200.) / px(200.))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_CHATLIST_WAITING_FOR_THE_QR_PAYLOAD))
                    .into_any_element(),
            };
            this.child(
                div()
                    .mt_2()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::Q_CHATLIST_QR_CODE)),
            )
            .child(div().mt_1().child(qr))
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_CHATLIST_SCAN_WITH_A_LOGGED_IN)),
            )
        })
        .into_any_element()
    }
}
