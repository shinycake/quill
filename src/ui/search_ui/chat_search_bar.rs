//! Methods moved out of `search_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn chat_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.search.chat_search.status)
            .unwrap_or(SearchStatus::Closed);
        let query = session
            .map(|s| s.search.chat_search.query.clone())
            .unwrap_or_default();
        let position = session
            .map(|s| s.search.chat_search.position_label())
            .unwrap_or_default();
        let jump_note = session.map(chat_search_jump_note).unwrap_or_default();
        let can_pick = session.is_some_and(|s| s.chat_search_can_pick_sender());
        let sender_label = session.and_then(|s| {
            s.search
                .chat_search
                .sender
                .map(|sender| s.sender_label(sender))
        });
        let media = session
            .map(|s| s.search.chat_search.media)
            .unwrap_or_default();
        let picker = session.and_then(|s| s.search.chat_search.from_picker.clone());
        let hits: Vec<(MessageId, String, i32, bool)> = session
            .map(|s| {
                s.search
                    .chat_search
                    .hits
                    .iter()
                    .enumerate()
                    .map(|(i, hit)| {
                        (
                            hit.message_id,
                            hit.preview.clone(),
                            hit.date,
                            s.search.chat_search.selected == Some(i),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let subject = if !query.is_empty() {
            format!("“{query}”")
        } else if let Some(name) = &sender_label {
            format!("messages from {name}")
        } else {
            media.label().to_lowercase()
        };
        let caption = match status {
            SearchStatus::Idle => "Type to search this chat.".to_string(),
            SearchStatus::Searching => format!("Searching {subject}…"),
            SearchStatus::Ready => {
                if jump_note.is_empty() {
                    format!("Results for {subject}")
                } else {
                    format!("Results for {subject} · {jump_note}")
                }
            }
            SearchStatus::Empty => format!("No messages match {subject}."),
            SearchStatus::Failed => "Search in chat failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        let picker_open = picker.is_some();
        // A hashtag or cashtag: tdesktop's This chat / My messages / Public
        // posts tabs. "This chat" is where we are.
        let mut tag_chips = Vec::new();
        if let Some(tag) = tag_query(&query).filter(|_| !picker_open) {
            let tag = tag.to_string();
            tag_chips.push(
                Self::filter_chip(
                    "chat-search-scope-chat",
                    "This chat",
                    true,
                    |_, _, _| {},
                    cx,
                )
                .into_any_element(),
            );
            for scope in SearchScope::ALL {
                let tag = tag.clone();
                tag_chips.push(
                    Self::filter_chip(
                        ("chat-search-scope", scope as u64),
                        scope.label(),
                        false,
                        move |this, window, cx| this.search_tag_in_scope(&tag, scope, window, cx),
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        let mut media_chips = Vec::new();
        for kind in SearchMediaKind::ALL {
            media_chips.push(
                Self::filter_chip(
                    ("chat-search-media", kind as u64),
                    kind.label(),
                    media == kind,
                    move |this, _, cx| this.chat_search_pick_media(kind, cx),
                    cx,
                )
                .into_any_element(),
            );
        }
        div()
            .id("chat-search")
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                // One row: field, position, older/newer, calendar, close.
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().id("chat-search-field").flex_1().child(
                            Textarea::new(&self.search_ui.chat_input)
                                .aria_label(if picker_open {
                                    "Search members"
                                } else {
                                    "Search this conversation"
                                })
                                .h(px(36.)),
                        ),
                    )
                    .when(!position.is_empty() && !picker_open, |this| {
                        this.child(
                            div()
                                .px_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(position),
                        )
                    })
                    .child(
                        Button::new("chat-search-older")
                            .icon(gpui_kit::assets::IconName::ChevronUp)
                            .ghost()
                            .tooltip("Older match")
                            .accessibility_label("Older match")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_older(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-newer")
                            .icon(gpui_kit::assets::IconName::ChevronDown)
                            .ghost()
                            .tooltip("Newer match")
                            .accessibility_label("Newer match")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_newer(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-calendar")
                            .icon(gpui_kit::assets::IconName::Calendar)
                            .ghost()
                            .tooltip("Jump to date")
                            .accessibility_label("Jump to date")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_jump_date_ui(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip("Close search")
                            .accessibility_label("Close search")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_chat_search_ui(window, cx);
                            })),
                    ),
            )
            .child(
                // Filters: who wrote it, what kind of message.
                div()
                    .id("chat-search-filters")
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .when(can_pick, |this| {
                        let label = sender_label
                            .as_ref()
                            .map_or_else(|| "From…".to_string(), |name| format!("From: {name}"));
                        this.child(Self::filter_chip(
                            "chat-search-from",
                            &label,
                            sender_label.is_some() || picker_open,
                            |this, window, cx| {
                                if this
                                    .session()
                                    .is_some_and(|s| s.search.chat_search.sender.is_some())
                                {
                                    this.chat_search_pick_sender(None, window, cx);
                                } else {
                                    this.toggle_chat_search_picker(window, cx);
                                }
                            },
                            cx,
                        ))
                    })
                    .children(tag_chips)
                    .children(media_chips),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if picker_open { String::new() } else { caption }),
            )
            .when_some(picker, |this, picker| {
                this.child(self.chat_search_picker(&picker, cx))
            })
            .when(!hits.is_empty() && !picker_open, |this| {
                let mut list = div().id("chat-search-hits").flex().flex_col().gap_1();
                for (message_id, preview, date, selected) in hits {
                    list = list.child(chat_search_hit_row(
                        message_id, preview, date, &query, selected, cx,
                    ));
                }
                this.child(list)
            })
    }

    pub(in crate::ui) fn sync_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_search_query(query) {
                Ok(SearchQueryOutcome::Sent(_)) => {
                    self.connection.status_note = "searching…".into()
                }
                Ok(SearchQueryOutcome::Debounced { token }) => {
                    self.schedule_search_commit(token, cx);
                }
                Ok(SearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.connection.status_note = "search chats and messages".into();
                }
                Ok(SearchQueryOutcome::Unchanged) => {}
                Err(_) => self.connection.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.search.search.open
                && session.search.search.query == trimmed
                && !matches!(session.search.search.status, SearchStatus::Closed)
            {
                cx.notify();
                return;
            }
            session.apply_local_search_filter(query);
        }
        cx.notify();
    }

    pub(in crate::ui) fn schedule_search_commit(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(live) = this.live.as_mut() {
                    match live.driver.commit_debounced_search(token) {
                        Ok(Some(_)) => this.connection.status_note = "searching…".into(),
                        Ok(None) => {}
                        Err(_) => this.connection.status_note = "could not search".into(),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::ui) fn activate_first_search_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat = self
            .session()
            .and_then(|session| session.search.search.merged_chat_ids().first().copied());
        let message = self.session().and_then(|session| {
            session
                .search
                .search
                .messages
                .first()
                .map(|hit| (hit.chat_id, hit.message_id))
        });
        if let Some(chat_id) = chat {
            self.select_search_chat(chat_id, window, cx);
        } else if let Some((chat_id, message_id)) = message {
            self.select_search_message(chat_id, message_id, window, cx);
        }
    }

    pub(in crate::ui) fn select_search_chat(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .select_search_chat(chat_id, &text, reply, now_ms)
            {
                Ok(_) => "chat selected".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat
                && prev != chat_id
            {
                let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                let reply_to = stored.as_ref().and(reply);
                let draft = stored.map(|body| ChatDraft {
                    text: body,
                    reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
                    quote: reply_to.and_then(|reply| reply.quote),
                });
                session.store_composer_draft(prev, draft);
            }
            session.close_search();
            session.open_chat(chat_id);
            self.connection.status_note = "chat selected".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_ui
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(in crate::ui) fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .select_search_message(chat_id, message_id, &text, reply, now_ms)
            {
                Ok(_) => "opened chat".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat
                && prev != chat_id
            {
                let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                let reply_to = stored.as_ref().and(reply);
                let draft = stored.map(|body| ChatDraft {
                    text: body,
                    reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
                    quote: reply_to.and_then(|reply| reply.quote),
                });
                session.store_composer_draft(prev, draft);
            }
            session.promote_search_message(chat_id, message_id);
            session.close_search();
            session.open_chat(chat_id);
            self.connection.status_note = "opened chat".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_ui
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(in crate::ui) fn sidebar_search_field(
        &self,
        story_stack: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("sidebar-search")
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("sidebar-search-field")
                    .flex_1()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.search_is_open() {
                            this.open_search_ui(window, cx);
                        }
                    }))
                    .child(
                        Textarea::new(&self.search_ui.input)
                            .aria_label("Search")
                            .h(px(40.)),
                    ),
            )
            .children(story_stack)
            .when(self.search_is_open(), |this| {
                this.child(
                    Button::new("search-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .ghost()
                        .small()
                        .tooltip("Close search")
                        .accessibility_label("Close search")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_search(window, cx);
                        })),
                )
            })
    }

    /// Slice (communities-search-filter): community filter chips at the top of
    /// the typed-search panel — "All chats" (null filter) plus one chip per
    /// accessible community from `SessionState.groups.communities` (fed by
    /// `updateCommunity`; TDLib 1.8.67 has no list-communities method, and
    /// `searchMessagesChatTypeFilterCommunity` requires a `community_id`,
    /// schema line 6344). Omitted when there is nothing to filter by.
    /// Same kit-Button chip pattern as the event-log filters.
    pub(in crate::ui) fn search_community_filter_chips(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let selected = session.search.search.community_filter;
        let mut communities: Vec<(i64, String)> = session
            .groups
            .communities
            .iter()
            .filter(|(_, community)| community.have_access)
            .map(|(id, community)| (*id, community.name.clone()))
            .collect();
        if communities.is_empty() {
            return None;
        }
        communities.sort_by_key(|a| a.1.to_lowercase());
        let mut chips = div()
            .id("search-community-filter")
            .flex()
            .flex_wrap()
            .items_center()
            .w_full()
            .gap_1();
        let all_label = if selected.is_none() {
            "✓ All chats"
        } else {
            "All chats"
        };
        chips = chips.child(
            Button::new("search-community-filter-all")
                .label(all_label)
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_search_community_filter(None, cx);
                })),
        );
        for (id, name) in communities {
            let label = if selected == Some(id) {
                format!("✓ {name}")
            } else {
                name
            };
            chips = chips.child(
                Button::new(("search-community-filter", id as u64))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_search_community_filter(Some(id), cx);
                    })),
            );
        }
        Some(chips.into_any_element())
    }

    /// Global search narrowing, shown once there is a query: chat type
    /// (tdesktop `lng_search_filter_*`), content tab and date window.
    pub(in crate::ui) fn search_filter_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let filters = session.search.search.filters;
        if session.search.search.query.trim().is_empty() && filters.scope != SearchScope::Apps {
            return None;
        }
        let query = session.search.search.query.trim().to_string();
        let has_chat = session.open_chat.is_some();
        // tdesktop's search tabs: This chat / My messages / Public posts.
        let mut scopes = Vec::new();
        if has_chat {
            let query = query.clone();
            scopes.push(
                Self::filter_chip(
                    "search-scope-this-chat",
                    "This chat",
                    false,
                    move |this, window, cx| this.search_query_in_this_chat(&query, window, cx),
                    cx,
                )
                .into_any_element(),
            );
        }
        for scope in SearchScope::ALL {
            scopes.push(
                Self::filter_chip(
                    ("search-scope", scope as u64),
                    scope.label(),
                    filters.scope == scope,
                    move |this, _, cx| {
                        let mut next = this
                            .session()
                            .map(|s| s.search.search.filters)
                            .unwrap_or_default();
                        next.scope = scope;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let archive_chip = Self::filter_chip(
            "search-filter-archived",
            "From archive",
            filters.archived,
            move |this, _, cx| {
                let mut next = this
                    .session()
                    .map(|s| s.search.search.filters)
                    .unwrap_or_default();
                next.archived = !next.archived;
                this.set_search_filters(next, cx);
            },
            cx,
        )
        .into_any_element();
        let mut types = Vec::new();
        for kind in SearchChatType::ALL {
            types.push(
                Self::filter_chip(
                    ("search-filter-type", kind as u64),
                    kind.label(),
                    filters.chat_type == kind,
                    move |this, _, cx| {
                        let mut next = this
                            .session()
                            .map(|s| s.search.search.filters)
                            .unwrap_or_default();
                        next.chat_type = kind;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut media = Vec::new();
        for kind in SearchMediaKind::ALL {
            media.push(
                Self::filter_chip(
                    ("search-filter-media", kind as u64),
                    kind.label(),
                    filters.media == kind,
                    move |this, _, cx| {
                        let mut next = this
                            .session()
                            .map(|s| s.search.search.filters)
                            .unwrap_or_default();
                        next.media = kind;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut dates = Vec::new();
        for range in SearchDateRange::ALL {
            dates.push(
                Self::filter_chip(
                    ("search-filter-date", range as u64),
                    range.label(),
                    filters.date == range,
                    move |this, _, cx| {
                        let mut next = this
                            .session()
                            .map(|s| s.search.search.filters)
                            .unwrap_or_default();
                        next.date = range;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let muted = cx.theme().muted_foreground;
        let group = |label: &'static str, chips: Vec<AnyElement>| {
            div()
                .flex()
                .items_start()
                .gap_1()
                .child(
                    div()
                        .w(px(56.))
                        .flex_none()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .children(chips),
                )
        };
        types.push(archive_chip);
        let public = filters.scope == SearchScope::PublicPosts;
        Some(
            div()
                .id("search-filters")
                .flex()
                .flex_col()
                .gap_1()
                .child(group("Search", scopes))
                // Public posts have no chat-type, content or date narrowing.
                .when(!public, |this| {
                    this.child(group("Chats", types))
                        .child(group("Content", media))
                        .child(group("Date", dates))
                })
                .into_any_element(),
        )
    }

    /// tdesktop's "Frequent contacts" strip on an empty search: avatar and
    /// first name per person; right-click opens "Remove from Recent" /
    /// "Remove all & Disable".
    pub(super) fn frequent_contacts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if session.search.search.top_chats_disabled || session.search.search.top_chats.is_empty() {
            return None;
        }
        let menu = session.search.search.top_menu;
        let tiles: Vec<(ChatId, String)> = session
            .search
            .search
            .top_chats
            .iter()
            .map(|id| {
                let title = session
                    .chats
                    .get(&id.0)
                    .map_or_else(|| format!("chat {}", id.0), |chat| chat.title.clone());
                (*id, title)
            })
            .collect();
        let muted = cx.theme().muted_foreground;
        let mut strip = div()
            .id("search-frequent-strip")
            .flex()
            .items_start()
            .gap_1()
            .overflow_x_scroll();
        for (id, title) in tiles {
            let photo = self.chat_photo_for_row(id);
            let first = title.split_whitespace().next().unwrap_or("").to_string();
            let name = title.clone();
            strip = strip.child(
                div()
                    .id(("search-frequent", id.0 as u64))
                    .flex_none()
                    .w(px(64.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .py_1()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "{name}. Right-click to remove from frequent contacts"
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .when(menu == Some(id), |this| this.bg(cx.theme().selection))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_search_chat(id, window, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| {
                            this.set_search_prompt(None, Some(id), cx);
                        }),
                    )
                    .child(super::super::chat_row::chat_avatar(
                        &title,
                        photo.as_deref(),
                        44.,
                    ))
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_center()
                            .truncate()
                            .text_color(muted)
                            .child(super::super::bidi_line::one_line_plain(first)),
                    ),
            );
        }
        let mut block = div()
            .id("search-frequent")
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().font_semibold().child("Frequent contacts"))
            .child(strip);
        if let Some(target) = menu {
            let name = session
                .chats
                .get(&target.0)
                .map_or_else(|| "this contact".to_string(), |chat| chat.title.clone());
            block = block.child(
                div()
                    .id("search-frequent-menu")
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("search-frequent-remove")
                            .label(format!("Remove {name} from Recent"))
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_top_chat(target, cx);
                            })),
                    )
                    .child(
                        Button::new("search-frequent-disable")
                            .label("Remove all & Disable")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_search_prompt(
                                    Some(SearchConfirm::DisableTopChats),
                                    None,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("search-frequent-cancel")
                            .label("Cancel")
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_search_prompt(None, None, cx);
                            })),
                    ),
            );
        }
        Some(block.into_any_element())
    }

    /// The inline "are you sure" of the search panel (tdesktop
    /// `lng_recent_clear_sure` / `lng_recent_hide_sure`).
    pub(super) fn search_confirm_row(
        &self,
        confirm: SearchConfirm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (text, action) = match confirm {
            SearchConfirm::ClearRecents => {
                ("Do you want to clear your search history?", "Clear all")
            }
            SearchConfirm::DisableTopChats => (
                "Clear and disable the frequent contacts list? You can turn it back on in Settings > Privacy > Suggest frequent contacts.",
                "Hide",
            ),
        };
        div()
            .id("search-confirm")
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().child(text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("search-confirm-accept")
                            .label(action)
                            .small()
                            .custom(super::super::security::quiet_danger(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.accept_search_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("search-confirm-cancel")
                            .label("Cancel")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_search_prompt(None, None, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
