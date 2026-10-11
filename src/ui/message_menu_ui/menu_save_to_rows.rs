//! Methods moved out of `message_menu_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// The "Save to..." page: Profile, Saved Messages, Downloads
    /// (`lng_context_save_music_*`).
    pub(in crate::ui) fn menu_save_to_rows(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let message = session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0));
        let Some(target) = message.and_then(|m| {
            media_target(quill::telegram::envelope::effective_content(
                &m.content,
                m.ephemeral.as_ref(),
            ))
        }) else {
            return Vec::new();
        };
        let saved = session.is_saved_messages(chat_id);
        let mut rows = vec![menu_row(
            1,
            IconName::User,
            "menu-save-to-profile",
            "... Profile",
            false,
            cx,
            {
                let target = target.clone();
                move |this, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        this.connection.status_note = match live.driver.save_audio_to_profile(
                            target.file_id,
                            target.duration,
                            &target.title,
                            &target.performer,
                        ) {
                            Ok(_) => "saving to your profile…".into(),
                            Err(_) => "could not save to your profile".into(),
                        };
                    } else {
                        this.connection.status_note =
                            "demo — saving to the profile needs live TDLib".into();
                    }
                    this.message_ui.menu = None;
                    cx.notify();
                }
            },
        )];
        if !saved {
            rows.push(menu_row(
                2,
                IconName::Bookmark,
                "menu-save-to-saved",
                "... Saved Messages",
                false,
                cx,
                move |this, _, cx| {
                    let me = this.session().and_then(|s| s.my_user_id);
                    let draft =
                        quill::composer::ForwardDraft::from_message(chat_id, message_id, false);
                    if let (Some(live), Some(me), Some(draft)) = (this.live.as_mut(), me, draft) {
                        this.connection.status_note =
                            match live.driver.forward_messages(ChatId(me), &draft) {
                                Ok(_) => "saved to Saved Messages".into(),
                                Err(_) => "could not save to Saved Messages".into(),
                            };
                    } else {
                        this.connection.status_note =
                            "demo — Saved Messages needs live TDLib".into();
                    }
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        rows.push(menu_row(
            3,
            IconName::Download,
            "menu-save-to-downloads",
            "... Downloads",
            false,
            cx,
            move |this, _, cx| {
                this.save_message_media_as(chat_id, message_id, cx);
                this.message_ui.menu = None;
                cx.notify();
            },
        ));
        rows
    }

    /// The up-to-three people the row shows, reactors first.
    pub(super) fn audience_faces(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Vec<MessageSender> {
        let Some(audience) = self
            .session()
            .and_then(|s| s.messages.message_audience.as_ref())
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
        else {
            return Vec::new();
        };
        let mut faces: Vec<MessageSender> = Vec::new();
        if let Some(page) = audience.reactions.ready() {
            faces.extend(page.reactions.iter().map(|r| r.sender));
        }
        if let Some(viewers) = audience.viewers.ready() {
            faces.extend(
                viewers
                    .iter()
                    .map(|v| MessageSender::User { user_id: v.user_id }),
            );
        }
        let mut unique = Vec::new();
        for face in faces {
            if !unique.contains(&face) {
                unique.push(face);
            }
            if unique.len() == 3 {
                break;
            }
        }
        unique
    }

    /// The "N Seen" / "N Reacted" row of a group, or the read-date line of
    /// a private chat, followed by the date lines.
    pub(in crate::ui) fn menu_audience_rows(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let mut rows = Vec::new();
        let Some(session) = self.session() else {
            return rows;
        };
        let message_id = message.id;
        let kind = seen_kind(quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ));
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let audience = session
            .messages
            .message_audience
            .as_ref()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id);
        if let Some(audience) = audience {
            // A private chat: the other side's read time.
            if let Audience::Ready(read) = &audience.read_date {
                let label = read_status_label(*read, &now);
                rows.push(info_row(
                    order::AUDIENCE,
                    "menu-read-date",
                    Some(IconName::Eye),
                    label,
                ));
            } else if audience.viewers.is_loading() || audience.reactions.is_loading() {
                rows.push(info_row(
                    order::AUDIENCE,
                    "menu-seen-loading",
                    Some(IconName::Eye),
                    "Loading...",
                ));
            }
            let seen = audience.viewers.ready().map(|v| v.len());
            let reacted = audience
                .reactions
                .ready()
                .map(|p| p.total_count.max(0) as usize);
            let label = match (seen, reacted) {
                (Some(seen), Some(reacted)) if reacted > 0 && seen > 0 && reacted <= seen => {
                    Some(format!("{reacted}/{seen} Reacted"))
                }
                (_, Some(reacted)) if reacted > 0 => Some(reacted_label(reacted)),
                (Some(seen), _) => Some(seen_label(kind, seen)),
                _ => None,
            };
            if let Some(label) = label {
                let faces = self.audience_faces(chat_id, message_id);
                let roots = self.media_display_roots();
                let avatars: Vec<(String, Option<PathBuf>)> = faces
                    .iter()
                    .map(|face| super::super::history::reactor_avatar(face, Some(session), &roots))
                    .collect();
                let row_hover = cx.theme().accent;
                rows.push((
                    order::AUDIENCE,
                    div()
                        .id("menu-audience")
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(text_menu())
                        .hover(|style| style.bg(row_hover))
                        .role(gpui_kit::Role::MenuItem)
                        .aria_label(label.clone())
                        .child(Icon::new(IconName::Eye).size(px(16.)))
                        .child(label)
                        .child(div().flex().items_center().ml_auto().children(
                            avatars.into_iter().enumerate().map(|(ix, (name, photo))| {
                                div()
                                    .when(ix > 0, |this| this.ml(px(-6.)))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(bg_canvas())
                                    .child(super::super::message_text::kit_avatar_element(
                                        &name,
                                        photo.as_deref(),
                                        px(18.),
                                    ))
                            }),
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.message_ui.menu_ui.page = MessageMenuPage::Audience;
                            this.message_ui.menu_ui.audience_tab = None;
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
        }
        if message.date > 0 && message_id.0 > 0 && !message.pending {
            let sent = quill::local_time::civil_local(i64::from(message.date));
            rows.push(info_row(
                order::SENT,
                "menu-sent",
                None,
                quill::message_menu::sent_label(&sent, &now),
            ));
            if message.extras.edit_date > 0 {
                let edited = quill::local_time::civil_local(i64::from(message.extras.edit_date));
                rows.push(info_row(
                    order::SENT + 1,
                    "menu-edited",
                    None,
                    quill::message_menu::edited_label(&edited, &now),
                ));
            }
        }
        rows
    }

    /// Right-click on a reaction chip: the menu opens on the reactor list
    /// of that reaction (tdesktop `ShowWhoReactedMenu`).
    pub(in crate::ui) fn open_reactors_menu(
        &mut self,
        menu: super::super::menu_states::MessageMenuState,
        reaction: ReactionType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_message_menu(menu, window, cx);
        let several = self
            .session()
            .and_then(|s| s.histories.get(&menu.chat_id.0))
            .and_then(|h| h.messages.get(&menu.message_id.0))
            .is_some_and(|m| m.reaction_chips().len() > 1);
        self.message_ui.menu_ui.page = MessageMenuPage::Audience;
        self.message_ui.menu_ui.audience_tab = several.then(|| reaction.clone());
        if several && let Some(live) = self.live.as_mut() {
            live.driver.session.messages.wanted_reactor_tab =
                Some((menu.chat_id, menu.message_id, reaction));
        }
        cx.notify();
    }

    /// Switch the "who reacted" tab; a tab loads its first page once.
    pub(super) fn select_audience_tab(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        tab: Option<ReactionType>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .fetch_reactors_tab(chat_id, message_id, tab.as_ref(), false);
        }
        self.message_ui.menu_ui.audience_tab = tab;
        cx.notify();
    }

    /// The page behind the "N Seen" row: reactors with their reaction and
    /// time, then viewers with theirs.
    pub(in crate::ui) fn menu_audience_page(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let message_id = message.id;
        let kind = seen_kind(quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ));
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let roots = self.media_display_roots();
        let Some(audience) = session
            .messages
            .message_audience
            .as_ref()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
        else {
            return Vec::new();
        };
        // An admin may drop one member's reaction when TDLib says this
        // message allows it (`messageProperties.can_delete_reactions`).
        let can_delete_reactions = session
            .messages
            .message_menu_actions
            .is_some_and(|(c, m, a)| c == chat_id && m == message_id && a.can_delete_reactions);
        let person = |ix: u64,
                      sender: MessageSender,
                      detail: Option<String>,
                      when: i32,
                      deletable: bool,
                      cx: &mut Context<Self>|
         -> MenuRow {
            let (name, photo) =
                super::super::history::reactor_avatar(&sender, Some(session), &roots);
            let name = if name.is_empty() {
                "Unknown".to_string()
            } else {
                name
            };
            let when_label = (when > 0)
                .then(|| read_date_label(&quill::local_time::civil_local(i64::from(when)), &now));
            let row_hover = cx.theme().accent;
            (
                10,
                div()
                    .id(("menu-audience-person", ix))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(row_hover))
                    .role(gpui_kit::Role::MenuItem)
                    .aria_label(name.clone())
                    .child(super::super::message_text::kit_avatar_element(
                        &name,
                        photo.as_deref(),
                        px(28.),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(text_menu())
                                    .truncate()
                                    .child(name),
                            )
                            .when_some(when_label, |this, label| {
                                this.child(div().text_xs().text_color(text_muted()).child(label))
                            }),
                    )
                    .when_some(detail, |this, detail| {
                        this.child(div().ml_auto().pl_3().text_base().child(detail))
                    })
                    .when(deletable, |this| {
                        this.child(
                            div()
                                .id(("menu-audience-delete-reaction", ix))
                                .ml_2()
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .text_xs()
                                .text_color(danger_bright())
                                .hover(|style| style.bg(row_hover))
                                .role(gpui_kit::Role::Button)
                                .aria_label("Delete reaction")
                                .child("Delete")
                                // Keep the row's own click (open profile) out of it.
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.delete_member_reaction(chat_id, message_id, sender, cx);
                                })),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.message_ui.menu = None;
                        this.open_avatar_profile(sender, window, cx);
                    }))
                    .into_any_element(),
            )
        };
        let heading = |text: String, id: u64| -> MenuRow {
            (
                10,
                div()
                    .id(("menu-audience-heading", id))
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .font_semibold()
                    .text_color(text_muted())
                    .child(text)
                    .into_any_element(),
            )
        };
        let mut rows = Vec::new();
        let mut ix = 0u64;
        // Tabs per reaction (tdesktop `Ui::ReactionsList` / the "All" tab
        // and one tab per reaction when there is more than one).
        let tab = self.message_ui.menu_ui.audience_tab.clone();
        let chips: Vec<(ReactionType, i32)> = message
            .reaction_chips()
            .into_iter()
            .map(|chip| (chip.reaction_type.clone(), chip.total_count))
            .collect();
        if chips.len() > 1 && audience.reactions.ready().is_some() {
            let total: i32 = chips.iter().map(|(_, count)| *count).sum();
            let mut tabs = div()
                .id("menu-audience-tabs")
                .flex()
                .flex_wrap()
                .gap_1()
                .px_2()
                .py_1();
            let all_selected = tab.is_none();
            let mut entries: Vec<(Option<ReactionType>, String)> =
                vec![(None, format!("All {total}"))];
            for (reaction, count) in &chips {
                let glyph = match reaction {
                    ReactionType::Emoji { emoji } => {
                        super::super::reactions::emoji_presentation(emoji)
                    }
                    ReactionType::Paid => "⭐".to_string(),
                    _ => "✦".to_string(),
                };
                entries.push((Some(reaction.clone()), format!("{glyph} {count}")));
            }
            for (tab_ix, (reaction, label)) in entries.into_iter().enumerate() {
                let selected = if tab_ix == 0 {
                    all_selected
                } else {
                    tab == reaction
                };
                let accent_bg = cx.theme().accent;
                tabs = tabs.child(
                    div()
                        .id(("menu-audience-tab", tab_ix as u64))
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .text_xs()
                        .cursor_pointer()
                        .text_color(text_menu())
                        .when(selected, |this| this.bg(accent_bg).font_semibold())
                        .hover(|style| style.bg(accent_bg))
                        .role(gpui_kit::Role::Button)
                        .aria_label(label.clone())
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_audience_tab(chat_id, message_id, reaction.clone(), cx);
                        })),
                );
            }
            rows.push((10, tabs.into_any_element()));
        }
        let tab_state = match &tab {
            None => Some(&audience.reactions),
            Some(reaction) => audience
                .filtered
                .get(&quill::state::reaction_filter_key(reaction)),
        };
        let tab_filter = tab.as_ref().map_or(0, quill::state::reaction_filter_key);
        if let Some(page) = tab_state.and_then(|state| state.ready())
            && !page.reactions.is_empty()
        {
            rows.push(heading(reacted_label(page.total_count.max(0) as usize), 0));
            for reaction in &page.reactions {
                ix += 1;
                let glyph = match &reaction.reaction_type {
                    ReactionType::Emoji { emoji } => emoji.clone(),
                    ReactionType::Paid => "⭐".to_string(),
                    _ => "✦".to_string(),
                };
                rows.push(person(
                    ix,
                    reaction.sender,
                    Some(glyph),
                    reaction.date,
                    can_delete_reactions,
                    cx,
                ));
            }
            if !page.next_offset.is_empty() {
                let loading = audience.more_loading.contains(&tab_filter);
                let more_tab = tab.clone();
                let hover = cx.theme().accent;
                rows.push((
                    10,
                    div()
                        .id("menu-audience-more")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .text_sm()
                        .text_color(text_muted())
                        .when(!loading, |this| {
                            this.cursor_pointer().hover(|style| style.bg(hover))
                        })
                        .role(gpui_kit::Role::Button)
                        .child(if loading { "Loading..." } else { "Show more" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.fetch_reactors_tab(
                                    chat_id,
                                    message_id,
                                    more_tab.as_ref(),
                                    true,
                                );
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
        } else if tab_state.is_some_and(|state| state.is_loading()) {
            rows.push(info_row(
                10,
                "menu-audience-tab-loading",
                None,
                "Loading...",
            ));
        }
        if let Some(viewers) = audience.viewers.ready()
            && !viewers.is_empty()
        {
            rows.push(heading(seen_label(kind, viewers.len()), 1));
            for viewer in viewers {
                ix += 1;
                rows.push(person(
                    ix,
                    MessageSender::User {
                        user_id: viewer.user_id,
                    },
                    None,
                    viewer.view_date,
                    false,
                    cx,
                ));
            }
        }
        if rows.is_empty() {
            let text = if audience.viewers.is_loading() || audience.reactions.is_loading() {
                "Loading...".to_string()
            } else {
                seen_label(SeenKind::Seen, 0)
            };
            rows.push(info_row(10, "menu-audience-empty", None, text));
        }
        rows
    }

    /// Start reporting messages (the menu's "Report", or the selection
    /// bar's): opens the dialog and sends the first `reportChat`.
    pub(in crate::ui) fn open_message_report(
        &mut self,
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        cx: &mut Context<Self>,
    ) {
        self.message_ui.menu = None;
        self.message_ui.menu_ui.report_open = true;
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .report_messages(chat_id, &message_ids, "", "", None)
                .is_err()
            {
                self.message_ui.menu_ui.report_open = false;
                self.connection.status_note = "could not start the report".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.begin_message_report(chat_id, message_ids);
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_message_report(&mut self, cx: &mut Context<Self>) {
        self.message_ui.menu_ui.report_open = false;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.clear_message_report();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_message_report();
        }
        cx.notify();
    }

    pub(super) fn message_report_flow(&self) -> Option<quill::state::MessageReportFlow> {
        self.session()
            .and_then(|s| s.messages.message_report.clone())
    }

    /// The user chose a reason.
    pub(in crate::ui) fn pick_message_report_option(
        &mut self,
        option: ReportOption,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.message_report_flow() else {
            return;
        };
        let MessageReportStage::PickOption { title, options } = flow.stage.clone() else {
            return;
        };
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .report_messages(
                    flow.chat_id,
                    &flow.message_ids,
                    &option.id,
                    "",
                    Some((option.text.clone(), title, options)),
                )
                .is_err()
        {
            self.connection.status_note = "could not send the report".into();
        }
        cx.notify();
    }

    /// Send the details text (or skip an optional one).
    pub(in crate::ui) fn send_message_report_text(
        &mut self,
        skip: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.message_report_flow() else {
            return;
        };
        let MessageReportStage::TextRequired {
            option_id,
            is_optional,
        } = flow.stage.clone()
        else {
            return;
        };
        let text = if skip {
            String::new()
        } else {
            self.message_ui
                .menu_ui
                .report_text
                .read(cx)
                .value()
                .trim()
                .to_string()
        };
        if text.is_empty() && !is_optional {
            self.connection.status_note = "add a comment to send the report".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .report_messages(flow.chat_id, &flow.message_ids, &option_id, &text, None)
                .is_err()
        {
            self.connection.status_note = "could not send the report".into();
        }
        self.message_ui
            .menu_ui
            .report_text
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn message_report_back(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.message_report_back();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.message_report_back();
        }
        cx.notify();
    }

    /// Telegram Desktop's report box (`ShowReportFlowBox`), step by step.
    pub(in crate::ui) fn build_message_report_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::MessageReport, |this, _, cx| {
                this.close_message_report(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(flow) = this.message_report_flow() else {
                return dialog
                    .title(shell::dialog_title("Report message"))
                    .on_close(on_close);
            };
            let close_button = |label: &'static str| {
                Button::new("message-report-close")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_message_report(cx);
                        this.close_kit_dialog_if_done(DialogKind::MessageReport, window, cx);
                    }))
            };
            let back_button = (!flow.trail.is_empty()).then(|| {
                Button::new("message-report-back")
                    .label("Back")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.message_report_back(cx)))
            });
            let mut body = div().flex().flex_col().gap_2().w(px(360.));
            let mut footer = div().flex().justify_end().gap_2();
            match &flow.stage {
                MessageReportStage::Checking | MessageReportStage::Sending => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child("Reporting…"),
                    );
                    footer = footer.child(close_button("Cancel"));
                }
                MessageReportStage::PickOption { title, options } => {
                    body = body.child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(text_menu())
                            .child(if title.is_empty() {
                                "Why are you reporting this message?".to_string()
                            } else {
                                title.clone()
                            }),
                    );
                    for (index, option) in options.iter().enumerate() {
                        let picked = option.clone();
                        body = body.child(
                            div()
                                .id(("message-report-option", index as u64))
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(border())
                                .cursor_pointer()
                                .text_sm()
                                .text_color(text_menu())
                                .hover(|style| style.bg(cx.theme().accent))
                                .role(gpui_kit::Role::Button)
                                .aria_label(option.text.clone())
                                .child(option.text.clone())
                                .child(Icon::new(IconName::ChevronRight).size(px(14.)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.pick_message_report_option(picked.clone(), cx);
                                })),
                        );
                    }
                    if let Some(back) = back_button {
                        footer = footer.child(back);
                    }
                    footer = footer.child(close_button("Close"));
                }
                MessageReportStage::TextRequired { is_optional, .. } => {
                    let is_optional = *is_optional;
                    if let Some(reason) = flow.trail.last() {
                        body = body.child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(text_menu())
                                .child(reason.clone()),
                        );
                    }
                    body = body
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Please help us by telling what is wrong with the message you have selected"),
                        )
                        .child(
                            Textarea::new(&this.message_ui.menu_ui.report_text)
                                .aria_label(if is_optional {
                                    "Add Comment (Optional)"
                                } else {
                                    "Add Comment"
                                })
                                .h(px(72.)),
                        );
                    if let Some(back) = back_button {
                        footer = footer.child(back);
                    }
                    if is_optional {
                        footer = footer.child(
                            Button::new("message-report-skip")
                                .label("Skip")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.send_message_report_text(true, window, cx);
                                })),
                        );
                    }
                    footer = footer.child(
                        Button::new("message-report-send")
                            .label("Report")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.send_message_report_text(false, window, cx);
                            })),
                    );
                }
                MessageReportStage::Reported => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(success())
                            .child("Thank you! Your report will be reviewed by our team."),
                    );
                    footer = footer.child(close_button("Close"));
                }
                MessageReportStage::Failed(message) => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(danger())
                            .child(message.clone()),
                    );
                    footer = footer.child(close_button("Close"));
                }
            }
            dialog
                .title(shell::dialog_title("Report message"))
                .content(shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}
