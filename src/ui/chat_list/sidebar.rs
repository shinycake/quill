//! The chat-list column: header, folder tabs, search field, the chat rows
//! or the search results, and the sign-in steps until the account is
//! ready.
//!
//! Each part is built in its own function. A debug build gives every
//! by-value builder step its own stack slot for the life of the function
//! (`Div` is 1.4 KB, `Button` 7 KB), and the whole column in one function
//! took 187 KB of the main thread's stack on every frame
//! (docs/decisions/main-thread-stack.md).

use super::*;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(in crate::ui) fn sidebar(
        &mut self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.pane_mode();
        // The list (or the search results) rings while it has the keyboard.
        let focus_visible = self.chat_list_focus_visible(window);
        let list = self.sidebar_column(mode, cx);
        let list = match mode {
            PaneMode::Synthetic => sidebar_synthetic_rows(list, cx),
            PaneMode::Connecting => list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_CHATLIST_NO_CHAT_LIST_UNTIL_READY)),
            ),
            PaneMode::Ready => self.sidebar_ready(list, focus_visible, window, cx),
        };
        let list = if mode == PaneMode::Ready {
            list.children(self.agent_connected_strip(cx))
                .children(self.chat_list_update_button(cx))
        } else {
            list
        };
        if mode == PaneMode::Ready {
            if self.folder_rail_active() {
                return div()
                    .id("sidebar-with-folder-rail")
                    .flex()
                    .w(self.list_width() + px(self.folder_rail_width()))
                    .flex_none()
                    .h_full()
                    .child(self.folder_rail(cx))
                    .child(list)
                    .into_any_element();
            }
            return list.into_any_element();
        }
        self.sidebar_auth(
            list,
            auth,
            show_phone,
            show_code,
            show_password,
            show_qr,
            cx,
        )
    }

    /// The column itself: its frame, the header and the list caption.
    #[inline(never)]
    fn sidebar_column(&self, mode: PaneMode, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("sidebar")
            .when(mode != PaneMode::Ready, |this| this.overflow_y_scroll())
            .track_focus(&self.focus_sidebar)
            .map(|this| self.chat_list_key_handlers(this, cx))
            .w(self.list_width())
            .flex_none()
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .gap_2()
            .child(self.sidebar_header(mode, cx))
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
            )
    }

    /// The Ready column: contacts, calls, or the chats with the folder
    /// tabs, stories and search field above them.
    #[inline(never)]
    fn sidebar_ready(
        &mut self,
        mut list: Stateful<Div>,
        focus_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.chat_list.contacts_tab_open {
            return list.child(self.contacts_panel(cx));
        }
        if self.chat_list.calls_tab_open {
            return list.child(
                div()
                    .id("calls-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.calls.history_scroll)
                    .on_scroll_wheel(crate::ui::scroll_load::watch_scroll(
                        cx,
                        Self::calls_history_more,
                    ))
                    .child(self.calls_list(cx)),
            );
        }
        // Searching spans every chat: the folder tabs step aside.
        if !self.search_is_open() {
            list = list.children(self.suggestion_card(cx));
            list = list.child(self.folder_tabs_with_community_banner(cx));
            // A shared folder whose owner added chats.
            list = list.children(self.folder_new_chats_bar(cx));
        }
        // Stories strip: its tiles, and the collapsed stack that takes its
        // place beside the search field once the list scrolls the strip
        // away.
        self.refresh_story_tiles();
        self.follow_story_pull(cx);
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
            return list.child(self.search_results(focus_visible, window, cx));
        }
        self.sidebar_chats(list, focus_visible, window, cx)
    }

    /// The chat rows: the select bar, the empty state, and the main list
    /// and archive section in one virtual list.
    #[inline(never)]
    fn sidebar_chats(
        &mut self,
        mut list: Stateful<Div>,
        focus_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let folder = self.folders.tab;
        let filter = self.chat_list.filter;
        // Parity slice: folder names + tags flag for chat-row chips.
        let folder_tags = self.folder_tag_context();
        let mut chats: Vec<&ChatSummary> = self.listed_main_chats();
        // Slice CL2: the Archived category shows only the archive section.
        let show_main_list = filter != ChatListFilter::Archived;
        // Slice CL3: multi-select mode — rows toggle the check instead of
        // opening the chat. Defined once here so the select bar, the main
        // loop, and the archive loop all see it.
        let mut archived: Vec<&ChatSummary> = self.listed_archived_chats();
        let selected_chats = self.selected_among_listed(&chats, &archived);
        let selecting = !selected_chats.is_empty();
        // Slice CL3: multi-select action bar (TGX `ChatsController`
        // selection header): the selected count, the bulk actions, and
        // cancel.
        if selecting {
            let count = selected_chats.len();
            list = list.child(self.chat_select_bar(count, cx));
        }
        if show_main_list && chats.is_empty() {
            let loading = self.session().is_some_and(|s| !s.chat_list.chats_exhausted);
            list = if loading && folder.is_none() {
                // kit Phase 9: skeleton rows while the first chat batch is
                // still loading.
                let mut loading_list = list;
                for i in 0..4 {
                    loading_list = loading_list.child(chat_list_skeleton_row(i));
                }
                loading_list
            } else {
                list.child(self.chat_list_empty(filter, folder, cx))
            };
        }
        // kit Phase 3: the chat rows (main list + archive section) render
        // through a kit `VirtualList` — only the visible window builds
        // elements. `open`, photos, pin-drag and multi-select state resolve
        // per visible row in `chat_list_item_element`. Rebuilt every
        // render; the list below only reads it.
        let row_height = |chat: &ChatSummary| {
            chat_row_height_tagged(
                chat_row_has_tags(chat, &folder_tags.names, folder_tags.show, folder),
                self.session().map_or(self.appearance.preview_lines, |s| {
                    s.chat_row_lines(chat.id, self.appearance.preview_lines)
                }),
            )
        };
        let mut items: Vec<ChatListItem> = Vec::with_capacity(chats.len() + 2);
        // tdesktop: "Archived chats" is a pinned-top entry of the
        // unfiltered main list, a full row or (with `archiveCollapsed`) a
        // slim bar, and absent when `archiveInMainMenu` moved it to the
        // menu. Opening it shows the archive (here: the Archived category).
        // The archived chats themselves never sit inside the main list.
        let has_archived = self.session().is_some_and(|s| s.has_archived_chats());
        // tdesktop's stories strip: collapsed at rest, pulled open at the
        // top of the list, then the first row (it scrolls away with the
        // list and collapses).
        if self.stories.strip.pull.expanded() {
            items.push(ChatListItem::StoryStrip);
        }
        match quill::chatlist_archive::row_mode(
            has_archived,
            self.appearance.archive_collapsed,
            self.appearance.archive_in_main_menu,
            filter == ChatListFilter::All && folder.is_none() && !self.search_is_open(),
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
            // A file drag over the list freezes its order.
            self.keep_frozen_order(&mut chats, |chat| chat.id.0);
            // A live pinned drag shows its swapped order (tdesktop moves
            // rows while dragging); stable, so unpinned rows keep their
            // place.
            if let Some(reorder) = self.chat_list.pin_reorder.as_ref()
                && reorder.dragging().is_some()
                && !self.chat_list.pin_reorder_archived
            {
                chats.sort_by_key(|chat| reorder.position(chat.id.0).unwrap_or(usize::MAX));
            }
            for chat in chats {
                items.push(ChatListItem::Chat {
                    id: chat.id,
                    archived: false,
                    height: row_height(chat),
                });
            }
        }
        // Parity slice: folder chats page eagerly — the driver re-requests
        // `loadChats(chatListFolder)` after each `ok` until a 404 marks the
        // folder exhausted, the same pattern as the main list. No "Load
        // more" button: paging is automatic, not user-triggered.
        if folder.is_none() && matches!(filter, ChatListFilter::Archived | ChatListFilter::Unread) {
            if filter != ChatListFilter::Unread
                && let Some(reorder) = self.chat_list.pin_reorder.as_ref()
                && reorder.dragging().is_some()
                && self.chat_list.pin_reorder_archived
            {
                archived.sort_by_key(|chat| reorder.position(chat.id.0).unwrap_or(usize::MAX));
            }
            self.keep_frozen_order(&mut archived, |chat| chat.id.0);
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
        // Pinned-drag animation: keep frames coming while rows slide, drop
        // the state once everything settled.
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
        // kit Phase 3: hand the flat item list to the kit `VirtualList`.
        // Item heights are fixed by construction (see `chat_row_height`),
        // so declared sizes always match the rendered rows.
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
                        ChatListItem::StoryStrip => px(quill::stories_strip::FULL_HEIGHT),
                    };
                    ItemSize::new(px(0.), height)
                })
                .collect(),
        );
        self.chat_list.items = items;
        self.chat_list.folder_tags = folder_tags;
        list.child(self.chat_list_rows(sizes, focus_visible, window, cx))
    }

    /// The empty main list: the reason it is empty and what to do.
    #[inline(never)]
    fn chat_list_empty(
        &self,
        filter: ChatListFilter,
        folder: Option<i32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, title, hint) = if let ChatListFilter::Community(community_id) = filter {
            // Parity slice `parity:communities-chatlist-mode`: a missing
            // pack means the fetch hasn't landed yet (entering the mode
            // fires `getCommunityFullInfo` when live) — "Loading…", like
            // the community info panel. An empty pack / no matching loaded
            // chats is the genuine empty state, never a crash.
            if self
                .session()
                .is_some_and(|s| !s.groups.community_full_infos.contains_key(&community_id))
            {
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
        chat_list_empty_state(glyph, title, hint, cx).into_any_element()
    }

    /// The virtual list of chat rows with its pointer handlers and the
    /// stories pull.
    #[inline(never)]
    fn chat_list_rows(
        &mut self,
        sizes: Rc<Vec<ItemSize<Pixels>>>,
        focus_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pull_gap = self.story_pull_gap(cx);
        let rows = v_virtual_list(
            cx.entity(),
            "chat-list",
            sizes,
            |this: &mut QuillApp, range, _window, cx| {
                range
                    .map(|ix| this.chat_list_item_element(ix, cx))
                    .collect::<Vec<_>>()
            },
        )
        // The app-owned handle keeps scroll position across re-renders
        // (scroll restoration).
        .track_scroll(&self.chat_list.scroll)
        .flex_1()
        .min_h_0()
        .w_full();
        // The kit's bounce stretches either end of the list; a pull at the
        // top also opens the stories strip.
        let bounce = self.chat_list_bounce(
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(rows)
                .children(pull_gap),
        );
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .rounded_md()
            .when(focus_visible, |this| this.focus_ring_style(window, cx))
            // A mouse move takes the selection back from the keys.
            .on_mouse_move(cx.listener(|this, _, _, cx| this.chat_list_mouse_moved(cx)))
            // Pinned drag: the pointer position drives the live reorder; a
            // release anywhere ends it.
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<PinnedChatDrag>, _, cx| {
                    let drag = event.drag(cx).clone();
                    this.pin_drag_move(&drag, f32::from(event.event.position.y), cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_pin_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_pin_drag(cx)),
            )
            .child(bounce)
            .into_any_element()
    }

    /// The sign-in steps below the column's header until the account is
    /// ready.
    #[inline(never)]
    fn sidebar_auth(
        &mut self,
        list: Stateful<Div>,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = list
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
        let list = list.when(
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

/// The two fixed rows of the synthetic (no account) column.
#[inline(never)]
fn sidebar_synthetic_rows(list: Stateful<Div>, cx: &mut Context<QuillApp>) -> Stateful<Div> {
    list.child(static_chat_row(
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
    ))
}
