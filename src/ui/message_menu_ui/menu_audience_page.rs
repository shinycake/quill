//! The page behind the menu's "N Seen" row: the people who saw or reacted.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
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
            .menu_actions
            .get(chat_id, message_id)
            .is_some_and(|a| a.can_delete_reactions);
        let items = &self.frame.menu.items;
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
                tr(keys::Q_CHAT_UNKNOWN).to_string()
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
                    .menu_key_target(row_key(10), items, cx)
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
                                .aria_label(tr(keys::Q_CHAT_DELETE_REACTION))
                                .child(tr(keys::MAC_MENU_DELETE))
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
            let mut entries: Vec<(Option<ReactionType>, String)> = vec![(
                None,
                tr_with(keys::Q_CHAT_ALL, &[("total", &total.to_string())]),
            )];
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
                        .child(if loading {
                            tr(keys::Q_CHAT_LOADING)
                        } else {
                            tr(keys::Q_CHAT_SHOW_MORE)
                        })
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
                tr(keys::Q_CHAT_LOADING),
            ));
        } else if tab_state.is_some_and(|state| state.is_failed()) {
            // A tab that failed is asked again by its Retry (selecting it
            // again does the same).
            rows.push(info_row(
                10,
                "menu-audience-tab-failed",
                None,
                tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN),
            ));
            let retry_tab = tab.clone();
            rows.push(menu_row(
                10,
                IconName::RefreshCw,
                "menu-audience-tab-retry",
                tr(keys::Q_CHAT_RETRY),
                false,
                &self.frame.menu.items,
                cx,
                move |this, _, cx| {
                    this.select_audience_tab(chat_id, message_id, retry_tab.clone(), cx);
                },
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
            if audience.viewers.is_failed() || audience.reactions.is_failed() {
                // Asking the message's properties again chains the lists
                // they allow from the start.
                rows.push(info_row(
                    10,
                    "menu-audience-failed",
                    None,
                    tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN),
                ));
                rows.push(menu_row(
                    10,
                    IconName::RefreshCw,
                    "menu-audience-retry",
                    tr(keys::Q_CHAT_RETRY),
                    false,
                    &self.frame.menu.items,
                    cx,
                    move |this, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.fetch_message_menu_actions(chat_id, message_id);
                        }
                        cx.notify();
                    },
                ));
                return rows;
            }
            let text = if audience.viewers.is_loading() || audience.reactions.is_loading() {
                tr(keys::Q_CHAT_LOADING).to_string()
            } else {
                seen_label(SeenKind::Seen, 0)
            };
            rows.push(info_row(10, "menu-audience-empty", None, text));
        }
        rows
    }
}
