//! The Chatbots screen ("Chat Automation", tdesktop
//! `settings_chatbots.cpp`): find a bot by username, add it, choose the
//! chats it reaches and its rights, or remove it. Saved when the screen is
//! left. Rules and requests: `quill::business_bots`.

use super::{ChatbotDraft, Leave, Recipients};
use crate::ui::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_bots::{
    BotRight, BotRights, ConnectedBot, RIGHT_GROUPS, merge_found_bots, parse_username_query,
};
use quill::business_settings::BusinessRecipients;
use quill::lang::{keys, tr, tr_with};
use std::time::Duration;

fn about() -> &'static str {
    tr(keys::CHAT_AUTOMATION_ABOUT)
}
fn add_about() -> &'static str {
    tr(keys::CHAT_AUTOMATION_ADD_ABOUT)
}

/// What the lookup shows under the username field.
enum Found {
    Nothing,
    Loading,
    Bots(Vec<i64>),
    Missing,
    Unsupported,
}

impl QuillApp {
    pub(super) fn new_chatbot_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChatbotDraft {
        let username = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx)
                .placeholder(tr(keys::CHATBOTS_PLACEHOLDER))
        });
        // tdesktop debounces the lookup by 400 ms.
        cx.subscribe_in(
            &username,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let text = input.read(cx).value().to_string();
                let input = input.clone();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if input.read(cx).value() != text.as_str() {
                            return;
                        }
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.lookup_business_bot(&text);
                        }
                        cx.notify();
                    });
                })
                .detach();
                let _ = this;
                cx.notify();
            },
        )
        .detach();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_connected_bot();
            let _ = live.driver.lookup_business_bot("");
        }
        let mut draft = ChatbotDraft {
            username,
            committed: None,
            recipients: BusinessRecipients::everyone(),
            rights: BotRights::default(),
            loaded: false,
            warning: None,
            confirm_leave: false,
        };
        self.fill_chatbot_draft(&mut draft);
        draft
    }

    fn server_bot(&self) -> Option<Option<ConnectedBot>> {
        self.session()
            .and_then(|s| s.settings.business_extra.connected_bot.clone())
    }

    /// Take the connected bot into the draft once TDLib reported it.
    fn fill_chatbot_draft(&self, draft: &mut ChatbotDraft) {
        let Some(server) = self
            .server_bot()
            .or_else(|| self.live.is_none().then_some(None))
        else {
            return;
        };
        draft.loaded = true;
        if let Some(bot) = server {
            draft.committed = Some(bot.bot_user_id);
            draft.recipients = bot.recipients.make_valid();
            draft.rights = bot.rights;
        }
    }

    /// Wait for `getBusinessConnectedBot` while the screen is open.
    pub(super) fn business_bot_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..50 {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(done) = this.update(cx, |this, cx| {
                    let Some(mut draft) = this.business_mut().and_then(|d| d.chatbot.take()) else {
                        return true;
                    };
                    if !draft.loaded {
                        this.fill_chatbot_draft(&mut draft);
                    }
                    let done = draft.loaded;
                    if let Some(d) = this.business_mut() {
                        d.chatbot = Some(draft);
                    }
                    cx.notify();
                    done
                }) else {
                    break;
                };
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn business_bot_usable(&self, user_id: i64) -> bool {
        self.session()
            .and_then(|s| s.users.get(&user_id))
            .is_some_and(|u| u.is_bot && u.can_connect_to_business)
    }

    fn business_bot_name(&self, user_id: i64) -> (String, String) {
        self.session()
            .and_then(|s| s.users.get(&user_id))
            .map_or_else(
                || {
                    (
                        tr_with(keys::Q_MEDIA_BOT_3, &[("user_id", &user_id.to_string())]),
                        String::new(),
                    )
                },
                |u| (u.display_name(), u.username.clone()),
            )
    }

    fn business_found_bots(
        &self,
        username: &Entity<gpui_kit::component::input::InputState>,
        cx: &App,
    ) -> Found {
        let Some(lookup) = self.session().map(|s| &s.settings.business_extra.lookup) else {
            return Found::Nothing;
        };
        let text = username.read(cx).value().to_string();
        if parse_username_query(&text).is_none() || lookup.query.is_empty() {
            return Found::Nothing;
        }
        if !lookup.done() {
            return Found::Loading;
        }
        let exact = lookup.exact.flatten();
        let bots = merge_found_bots(
            exact,
            lookup.mine.as_deref().unwrap_or_default(),
            lookup.global.as_deref().unwrap_or_default(),
            |id| self.business_bot_usable(id),
        );
        if !bots.is_empty() {
            return Found::Bots(bots);
        }
        let exact_bot = exact
            .and_then(|id| self.session()?.users.get(&id))
            .is_some_and(|u| u.is_bot);
        if exact_bot {
            Found::Unsupported
        } else {
            Found::Missing
        }
    }

    pub(super) fn add_business_bot(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let (name, _) = self.business_bot_name(user_id);
        if let Some(bot) = self.business_mut().and_then(|d| d.chatbot.as_mut()) {
            bot.committed = Some(user_id);
            bot.confirm_leave = false;
        }
        self.business_note(
            tr_with(keys::CHATBOTS_ADDED_SUCCESS, &[("bot", &name.to_string())]),
            cx,
        );
    }

    fn remove_business_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bot) = self.business_mut().and_then(|d| d.chatbot.as_mut()) {
            bot.committed = None;
            bot.recipients = BusinessRecipients::everyone();
            bot.rights = BotRights::default();
            let input = bot.username.clone();
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.lookup_business_bot("");
        }
        cx.notify();
    }

    pub(super) fn set_business_bot_right(
        &mut self,
        right: BotRight,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let committed = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.chatbot.as_ref())
            .and_then(|b| b.committed);
        let name = committed.map(|id| self.business_bot_name(id).0);
        if let Some(bot) = self.business_mut().and_then(|d| d.chatbot.as_mut()) {
            let was = bot.rights;
            bot.rights.set(right, on);
            bot.warning = BotRights::warning(&was, &bot.rights).map(|text| {
                text.replace(
                    "{bot}",
                    name.as_deref().unwrap_or(tr(keys::Q_MEDIA_THIS_BOT)),
                )
            });
        }
        cx.notify();
    }

    fn business_bot_row(&self, user_id: i64, can_add: bool, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let (name, username) = self.business_bot_name(user_id);
        div()
            .flex()
            .items_center()
            .gap_2()
            .py_1()
            .child(self.business_avatar(user_id, &name))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().truncate().child(name.clone()))
                    .when(!username.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("@{username}")),
                        )
                    }),
            )
            .when(can_add, |this| {
                this.child(
                    Button::new(("business-bot-add", user_id as u64))
                        .label(tr(keys::CHATBOTS_ADD))
                        .primary()
                        .small()
                        .accessibility_label(tr_with(
                            keys::Q_MEDIA_ADD_3,
                            &[("name", &name.to_string())],
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.add_business_bot(user_id, cx);
                        })),
                )
            })
    }

    fn business_bot_rights(&self, rights: BotRights, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut block = div().flex().flex_col().gap_1().child(
            div()
                .text_sm()
                .font_semibold()
                .child(tr(keys::CHATBOTS_PERMISSIONS_TITLE)),
        );
        for (group, (title, list)) in RIGHT_GROUPS.iter().enumerate() {
            if let Some(title) = title {
                block = block.child(
                    div()
                        .pt_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(*title),
                );
            }
            // "Read Messages" is always granted (tdesktop disables it).
            if group == 0 {
                block = block.child(
                    Checkbox::new("business-bot-right-read")
                        .checked(true)
                        .disabled(true)
                        .label(tr(keys::CHATBOTS_READ))
                        .tooltip(tr(keys::RIGHTS_PERMISSION_CANT_EDIT)),
                );
            }
            for (index, (right, label)) in list.iter().enumerate() {
                let right = *right;
                block = block.child(
                    Checkbox::new(("business-bot-right", (group * 16 + index) as u64))
                        .checked(rights.has(right))
                        .label(*label)
                        .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                            this.set_business_bot_right(right, on, cx);
                        })),
                );
            }
        }
        block
    }

    pub(super) fn business_chatbots_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((username, committed, rights, loaded, warning, confirm_leave)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.chatbot.as_ref())
            .map(|b| {
                (
                    b.username.clone(),
                    b.committed,
                    b.rights,
                    b.loaded,
                    b.warning.clone(),
                    b.confirm_leave,
                )
            })
        else {
            return div();
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(about()));
        if !loaded {
            return body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MEDIA_LOADING)),
            );
        }
        if let Some(warning) = warning {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().warning)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(tr(keys::CHATBOTS_WARNING_TITLE)),
                    )
                    .child(div().text_sm().child(warning))
                    .child(
                        div().flex().justify_end().child(
                            Button::new("business-bot-warning-ok")
                                .label(tr(keys::BOX_OK))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(bot) =
                                        this.business_mut().and_then(|d| d.chatbot.as_mut())
                                    {
                                        bot.warning = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                    ),
            );
        }
        if confirm_leave {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(tr(keys::CHATBOTS_LEAVE_WITHOUT_ADDED_TITLE)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::CHATBOTS_LEAVE_WITHOUT_ADDED_TEXT)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("business-bot-stay")
                                    .label(tr(keys::CANCEL))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(bot) =
                                            this.business_mut().and_then(|d| d.chatbot.as_mut())
                                        {
                                            bot.confirm_leave = false;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("business-bot-leave")
                                    .label(tr(keys::BOX_LEAVE))
                                    .primary()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        // Clearing the field drops the found bots,
                                        // so leaving no longer asks.
                                        if let Some(bot) =
                                            this.business_mut().and_then(|d| d.chatbot.as_mut())
                                        {
                                            let input = bot.username.clone();
                                            input.update(cx, |input, cx| {
                                                input.set_value("", window, cx)
                                            });
                                        }
                                        this.business_back(window, cx);
                                    })),
                            ),
                    ),
            );
        }
        let found = self.business_found_bots(&username, cx);
        match committed {
            Some(bot) => body = body.child(self.business_bot_row(bot, false, cx)),
            None => {
                body = body.child(Input::new(&username).aria_label(tr(keys::Q_MEDIA_BOT_USERNAME)));
                match &found {
                    Found::Loading => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(tr(keys::Q_MEDIA_SEARCHING)),
                        );
                    }
                    Found::Bots(bots) => {
                        for bot in bots {
                            body = body.child(self.business_bot_row(*bot, true, cx));
                        }
                    }
                    Found::Missing => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .text_center()
                                .child(tr(keys::CHATBOTS_NOT_FOUND)),
                        );
                    }
                    Found::Unsupported => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .text_center()
                                .child(tr(keys::CHATBOTS_NOT_SUPPORTED)),
                        );
                    }
                    Found::Nothing => {}
                }
            }
        }
        body = body
            .child(div().text_xs().text_color(muted).child(add_about()))
            .child(self.business_recipients_section(Recipients::Bot, cx));
        // Rights show for the added bot, or the first one found.
        let shown = committed.or(match &found {
            Found::Bots(bots) => bots.first().copied(),
            _ => None,
        });
        if shown.is_some() {
            body = body.child(self.business_bot_rights(rights, cx));
        }
        if committed.is_some() {
            body = body.child(
                crate::ui::pressable::action_row(
                    "business-bot-remove",
                    None,
                    tr(keys::CHATBOTS_REMOVE_BOT),
                    true,
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.remove_business_bot(window, cx);
                })),
            );
        }
        body
    }

    /// Save of the chatbot screen (when it is left). Found but not added
    /// bots ask first (tdesktop's "No Bot Added").
    pub(super) fn save_business_chatbot(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Leave {
        let Some(draft) = self.business_mut().and_then(|d| d.chatbot.take()) else {
            return Leave::Go(None);
        };
        let found = matches!(
            self.business_found_bots(&draft.username, cx),
            Found::Bots(_)
        );
        let ask = draft.committed.is_none() && found && !draft.confirm_leave;
        let wanted = draft.committed.map(|bot_user_id| ConnectedBot {
            bot_user_id,
            recipients: draft.recipients.clone(),
            rights: draft.rights,
        });
        let loaded = draft.loaded;
        if let Some(d) = self.business_mut() {
            d.chatbot = Some(draft);
        }
        if ask {
            if let Some(bot) = self.business_mut().and_then(|d| d.chatbot.as_mut()) {
                bot.confirm_leave = true;
            }
            cx.notify();
            return Leave::Stay;
        }
        let server = self.server_bot().flatten();
        if !loaded || wanted == server {
            return Leave::Go(None);
        }
        let previous = server.map(|b| b.bot_user_id);
        self.business_send(tr(keys::Q_MEDIA_CHATBOT_SETTINGS), |driver| {
            driver.save_connected_bot(wanted.as_ref(), previous).is_ok()
        })
    }
}
