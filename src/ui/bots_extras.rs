//! Bot extras: fast buttons mode (keys 1 to 9 press the last message's inline
//! buttons) and adding a bot to a group or channel from its profile. Behaviour
//! follows tdesktop's `FastButtonsBots`, `AddBotToGroupBoxController` and
//! `InviteToChatButton` (see `docs/decisions/codex-bots-extras.md`).

use super::app::QuillApp;
use super::dialogs::ProfileDialog;
use super::groups::{ADMIN_RIGHT_LABELS, admin_right_get, admin_right_set};
use super::message_payments::button_is_pressable;
use super::pressable::action_row;
use super::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::bot_invite::{BotFacts, ChatFacts, Invite, Plan, choices, invite_about, invite_label};
use quill::ids::ChatId;
use quill::telegram::envelope::{ChatAdminRights, ChatKind, ReplyMarkup};

impl QuillApp {
    /// A digit key in an empty, focused composer presses the matching inline
    /// button of the chat's last message, when fast buttons mode is on for
    /// the bot (tdesktop `HistoryWidget::setupFastButtonMode`).
    pub(super) fn try_fast_button(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let composer = self.composer.read(cx);
        if !composer.focus_handle(cx).is_focused(window)
            || !composer.value().is_empty()
            || self.composer_ui.pending_edit.is_some()
            || !self.composer_ui.pending_attachments.is_empty()
        {
            return false;
        }
        let Some(session) = self.session() else {
            return false;
        };
        let Some(chat_id) = session.open_chat else {
            return false;
        };
        let Some(target) = session.fast_button_target(chat_id) else {
            return false;
        };
        if !quill::fast_buttons::is_enabled(target.bot_id) {
            return false;
        }
        let Some((row, col)) = quill::fast_buttons::locate(&target.row_lens, index) else {
            return false;
        };
        let Some(message) = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&target.message_id.0))
        else {
            return false;
        };
        let markup = message
            .ephemeral
            .as_ref()
            .and_then(|ephemeral| ephemeral.reply_markup.as_ref())
            .or(message.reply_markup.as_ref());
        let Some(ReplyMarkup::InlineKeyboard(keyboard)) = markup else {
            return false;
        };
        let Some(button) = keyboard.rows.get(row).and_then(|r| r.get(col)).cloned() else {
            return false;
        };
        if !button_is_pressable(&button.kind) {
            return false;
        }
        let game = match &message.content {
            quill::telegram::envelope::MessageContent::Game(game) => Some(game.short_name.clone()),
            _ => None,
        };
        self.activate_inline_button(
            chat_id,
            target.message_id,
            &button,
            game.as_deref(),
            window,
            cx,
        );
        true
    }

    /// What the bot accepts, once its `userFullInfo` has arrived.
    fn bot_invite_facts(&self, bot_id: i64) -> Option<BotFacts> {
        let session = self.session()?;
        let user = session.user(bot_id).filter(|user| user.is_bot)?;
        let info = session.bots.bot_info.get(&bot_id)?.as_ref();
        Some(BotFacts {
            can_join_groups: user.can_join_groups,
            group_rights: info.and_then(|info| info.group_admin_rights),
            channel_rights: info.and_then(|info| info.channel_admin_rights),
        })
    }

    /// Profile actions of a bot: add to a group or channel, and fast
    /// buttons mode. `None` for people.
    pub(super) fn bot_profile_actions(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.user(user_id).is_some_and(|user| user.is_bot) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut column = div().flex().flex_col().w_full().gap_0p5();
        // Mini apps: tdesktop's "Open App" for a bot with a main app.
        if session
            .user(user_id)
            .is_some_and(|user| user.has_main_web_app)
        {
            column = column.child(
                action_row(
                    "info-panel-open-app",
                    Some(IconName::PanelTop),
                    "Open App",
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_main_web_app(user_id, "", cx);
                })),
            );
        }
        if let Some(facts) = self.bot_invite_facts(user_id)
            && let Some(label) = invite_label(&facts)
        {
            column = column.child(
                action_row(
                    "info-panel-add-bot",
                    Some(IconName::UserPlus),
                    label,
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_add_bot_dialog(user_id, Invite::default(), cx);
                })),
            );
            if let Some(about) = invite_about(&facts) {
                column = column.child(div().px_2().text_xs().text_color(muted).child(about));
            }
        }
        let on = quill::fast_buttons::is_enabled(user_id);
        column = column.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .px_2()
                .py_1p5()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().child("Fast buttons mode"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("Keys 1 to 9 press the buttons under the last message."),
                        ),
                )
                .child(
                    Switch::new("info-panel-fast-buttons")
                        .checked(on)
                        .accessibility_label("Fast buttons mode")
                        .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                            quill::fast_buttons::set_enabled(user_id, next);
                            // Rows are cached per message revision; the
                            // digits appear on the next history refresh.
                            this.history.rows_key = None;
                            cx.notify();
                        })),
                ),
        );
        Some(column.into_any_element())
    }

    pub(super) fn open_add_bot_dialog(
        &mut self,
        bot_id: i64,
        invite: Invite,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // The rights a bot asks for ride on its full info.
            let _ = live.driver.fetch_user_full_info(bot_id);
        }
        self.dialogs.profile_dialog = Some(ProfileDialog::AddBot {
            bot_id,
            invite,
            target: None,
            rights: ChatAdminRights::default(),
        });
        cx.notify();
    }

    /// The groups and channels the person could add the bot to, by title.
    fn add_bot_chat_facts(&self) -> Vec<ChatFacts> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let mut chats: Vec<ChatFacts> = session
            .chats
            .values()
            .filter(|chat| chat.supported())
            .filter_map(|chat| {
                let is_channel = match chat.kind {
                    ChatKind::BasicGroup { .. } => false,
                    ChatKind::Supergroup { is_channel, .. } => is_channel,
                    _ => return None,
                };
                Some(ChatFacts {
                    chat_id: chat.id.0,
                    title: chat.title.clone(),
                    is_channel,
                    can_add_admins: session.chat_can_manage_admins(chat.id),
                    can_add_members: !is_channel && session.chat_can_add_members(chat.id),
                })
            })
            .collect();
        chats.sort_by_key(|chat| chat.title.to_lowercase());
        chats
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_add_bot(
        &mut self,
        bot_id: i64,
        chat_id: i64,
        is_channel: bool,
        plan: &Plan,
        rights: ChatAdminRights,
        start_parameter: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: adding a bot needs a live session.".into();
            cx.notify();
            return true;
        };
        let chat = ChatId(chat_id);
        let sent = match plan {
            Plan::Admin(_) => {
                let rights = quill::bot_invite::mask_rights(rights, is_channel);
                let sent = live.driver.promote_chat_member(chat, bot_id, &rights);
                // tdesktop sends the link's start payload once the bot is
                // an administrator.
                if matches!(sent, Ok(Some(_))) && !start_parameter.is_empty() {
                    let _ = live
                        .driver
                        .send_bot_start_message(chat, bot_id, start_parameter);
                }
                sent
            }
            // A start payload replaces the plain add: the start message
            // puts the bot in the group (tdesktop `AddBotToGroup`).
            Plan::Member if !start_parameter.is_empty() => {
                live.driver
                    .send_bot_start_message(chat, bot_id, start_parameter)
            }
            Plan::Member => live.driver.add_chat_members(chat, &[bot_id]),
        };
        let title = live
            .driver
            .session
            .chats
            .get(&chat_id)
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        self.connection.status_note = match sent {
            Ok(Some(_)) => format!("Adding the bot to {title}"),
            Ok(None) => "You can't add the bot to this chat.".into(),
            Err(_) => "Couldn't reach Telegram; try again.".into(),
        };
        cx.notify();
        matches!(sent, Ok(Some(_)))
    }

    /// Title, body and footer of the "add a bot" dialog: a chat picker, then
    /// the rights to grant (administrator) or a plain confirmation (member).
    pub(super) fn add_bot_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let Some(ProfileDialog::AddBot {
            bot_id,
            invite,
            target,
            rights,
        }) = self.dialogs.profile_dialog.as_ref()
        else {
            return None;
        };
        let (bot_id, target, rights) = (*bot_id, *target, *rights);
        let invite = invite.clone();
        let muted = cx.theme().muted_foreground;
        let bot_name = self
            .session()?
            .user(bot_id)
            .map(|user| user.display_name())
            .unwrap_or_else(|| "the bot".into());
        let Some(facts) = self.bot_invite_facts(bot_id) else {
            // The bot's full info (its requested rights) is still loading.
            let footer = div().flex().gap_2().child(
                Button::new("add-bot-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_profile_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                    })),
            );
            return Some((
                format!("Add {bot_name} to a chat"),
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Loading…")
                    .into_any_element(),
                footer.into_any_element(),
            ));
        };
        let rows = choices(&facts, &invite, &self.add_bot_chat_facts());
        let cancel = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_profile_dialog(cx);
                    this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                }))
        };
        if let Some(chat_id) = target
            && let Some(row) = rows.iter().find(|row| row.chat_id == chat_id).cloned()
        {
            let is_channel = row.is_channel;
            let mut body = div().flex().flex_col().gap_3();
            let confirm_label;
            match &row.plan {
                Plan::Admin(_) => {
                    confirm_label = "Add as admin";
                    body = body.child(div().text_sm().text_color(muted).child(format!(
                        "{bot_name} will become an administrator of {}. Choose what it can do.",
                        row.title
                    )));
                    let mut grid = div().flex().flex_col().gap_1();
                    for &index in quill::bot_invite::relevant_rights(is_channel) {
                        let enabled = admin_right_get(&rights, index);
                        grid = grid.child(
                            Checkbox::new(format!("add-bot-right-{index}"))
                                .checked(enabled)
                                .label(ADMIN_RIGHT_LABELS[index])
                                .on_click(cx.listener(move |this, &on: &bool, window, cx| {
                                    if let Some(ProfileDialog::AddBot { rights, .. }) =
                                        &mut this.dialogs.profile_dialog
                                    {
                                        admin_right_set(rights, index, on);
                                    }
                                    window.refresh();
                                    cx.notify();
                                })),
                        );
                    }
                    body = body.child(grid);
                }
                Plan::Member => {
                    confirm_label = "Add";
                    body = body.child(
                        div()
                            .text_sm()
                            .child(format!("Add {bot_name} to “{}”?", row.title)),
                    );
                }
            }
            let plan = row.plan.clone();
            let start_parameter = invite.start_parameter.clone();
            let back_invite = invite.clone();
            let footer =
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("add-bot-confirm")
                            .label(confirm_label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.submit_add_bot(
                                    bot_id,
                                    chat_id,
                                    is_channel,
                                    &plan,
                                    rights,
                                    &start_parameter,
                                    cx,
                                ) {
                                    this.close_profile_dialog(cx);
                                    this.select_listed_chat(ChatId(chat_id), window, cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    )
                    .child(Button::new("add-bot-back").label("Back").ghost().on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.dialogs.profile_dialog = Some(ProfileDialog::AddBot {
                                bot_id,
                                invite: back_invite.clone(),
                                target: None,
                                rights: ChatAdminRights::default(),
                            });
                            window.refresh();
                            cx.notify();
                        }),
                    ));
            return Some((
                format!("Add {bot_name}"),
                body.into_any_element(),
                footer.into_any_element(),
            ));
        }
        let mut body = div().flex().flex_col().gap_1();
        if rows.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("You have no groups or channels where you can add this bot."),
            );
        }
        let mut shown_members = false;
        let mut shown_admin = false;
        for row in rows {
            let heading = match (&row.plan, shown_admin, shown_members) {
                (Plan::Admin(_), false, _) => {
                    shown_admin = true;
                    Some("Groups and channels I manage")
                }
                (Plan::Member, _, false) => {
                    shown_members = true;
                    Some("Groups")
                }
                _ => None,
            };
            if let Some(heading) = heading {
                body = body.child(
                    div()
                        .px_2()
                        .pt_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(heading),
                );
            }
            let chat_id = row.chat_id;
            let row_invite = invite.clone();
            let requested = match &row.plan {
                Plan::Admin(rights) => quill::bot_invite::mask_rights(*rights, row.is_channel),
                Plan::Member => ChatAdminRights::default(),
            };
            body = body.child(
                action_row(
                    ("add-bot-chat", chat_id.unsigned_abs()),
                    None,
                    row.title.clone(),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dialogs.profile_dialog = Some(ProfileDialog::AddBot {
                        bot_id,
                        invite: row_invite.clone(),
                        target: Some(chat_id),
                        rights: requested,
                    });
                    window.refresh();
                    cx.notify();
                })),
            );
        }
        let footer = div()
            .flex()
            .gap_2()
            .child(cancel("add-bot-cancel", "Cancel", cx));
        Some((
            format!("Add {bot_name} to a chat"),
            body.into_any_element(),
            footer.into_any_element(),
        ))
    }
}

impl QuillApp {
    /// A `t.me/<bot>?game=<name>` link: pick the chat to send the game to
    /// (tdesktop `ShowShareGameBox`).
    pub(super) fn open_share_game_dialog(
        &mut self,
        bot_id: i64,
        game_short_name: String,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.profile_dialog = Some(ProfileDialog::ShareGame {
            bot_id,
            game_short_name,
            target: None,
        });
        cx.notify();
    }

    /// Chats the game can go to, in chat-list order.
    fn share_game_destinations(&self) -> Vec<(i64, String, bool)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        session
            .forward_destinations("")
            .into_iter()
            .filter(|chat| {
                let is_self = matches!(
                    chat.kind,
                    ChatKind::Private { user_id } if session.my_user_id == Some(user_id.0)
                );
                quill::game_share::can_share_to(quill::game_share::Destination {
                    is_self,
                    is_channel: chat.is_channel(),
                    can_post: chat.can_post(),
                })
            })
            .take(60)
            .map(|chat| {
                (
                    chat.id.0,
                    chat.title.clone(),
                    matches!(
                        chat.kind,
                        ChatKind::Private { .. } | ChatKind::Secret { .. }
                    ),
                )
            })
            .collect()
    }

    fn send_shared_game(
        &mut self,
        bot_id: i64,
        game_short_name: &str,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: sharing needs a live session.".into();
            cx.notify();
            return true;
        };
        match live
            .driver
            .send_game_message(ChatId(chat_id), bot_id, game_short_name)
        {
            Ok(_) => {
                self.connection.status_note = "Game sent".into();
                true
            }
            Err(_) => {
                self.connection.status_note = "Couldn't send the game here.".into();
                cx.notify();
                false
            }
        }
    }

    /// Title, body and footer of the "share a game" dialog.
    pub(super) fn share_game_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let Some(ProfileDialog::ShareGame {
            bot_id,
            game_short_name,
            target,
        }) = self.dialogs.profile_dialog.as_ref()
        else {
            return None;
        };
        let (bot_id, target) = (*bot_id, *target);
        let game_short_name = game_short_name.clone();
        let muted = cx.theme().muted_foreground;
        let destinations = self.share_game_destinations();
        let cancel = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_profile_dialog(cx);
                    this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                }))
        };
        if let Some(chat_id) = target {
            let (title, is_private) = destinations
                .iter()
                .find(|(id, _, _)| *id == chat_id)
                .map(|(_, title, private)| (title.clone(), *private))
                .unwrap_or_default();
            let body = div()
                .text_sm()
                .child(quill::game_share::confirm_text(&title, is_private));
            let send_name = game_short_name.clone();
            let back_name = game_short_name;
            let footer = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("share-game-send")
                        .label("Send")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.send_shared_game(bot_id, &send_name, chat_id, cx) {
                                this.close_profile_dialog(cx);
                                this.select_listed_chat(ChatId(chat_id), window, cx);
                            }
                            this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                        })),
                )
                .child(
                    Button::new("share-game-back")
                        .label("Back")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dialogs.profile_dialog = Some(ProfileDialog::ShareGame {
                                bot_id,
                                game_short_name: back_name.clone(),
                                target: None,
                            });
                            window.refresh();
                            cx.notify();
                        })),
                );
            return Some((
                "Share game".into(),
                body.into_any_element(),
                footer.into_any_element(),
            ));
        }
        let mut body = div().flex().flex_col().gap_1().child(
            div()
                .text_xs()
                .text_color(muted)
                .pb_1()
                .child("Choose where to send the game."),
        );
        if destinations.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("No chats loaded yet."),
            );
        }
        for (id, title, _) in destinations {
            let name = game_short_name.clone();
            body = body.child(
                action_row(
                    ("share-game-chat", id.unsigned_abs()),
                    None,
                    title,
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dialogs.profile_dialog = Some(ProfileDialog::ShareGame {
                        bot_id,
                        game_short_name: name.clone(),
                        target: Some(id),
                    });
                    window.refresh();
                    cx.notify();
                })),
            );
        }
        let footer = div()
            .flex()
            .gap_2()
            .child(cancel("share-game-cancel", "Cancel", cx));
        Some((
            "Share game".into(),
            body.into_any_element(),
            footer.into_any_element(),
        ))
    }

    /// "Copy game link" of a game message: `t.me/<bot>?game=<name>`.
    pub(super) fn copy_game_link(
        &mut self,
        chat_id: ChatId,
        message_id: quill::ids::MessageId,
        cx: &mut Context<Self>,
    ) {
        let link = self.session().and_then(|session| {
            let message = session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))?;
            let quill::telegram::envelope::MessageContent::Game(game) = &message.content else {
                return None;
            };
            let user_id = match message.sender {
                Some(quill::telegram::envelope::MessageSender::User { user_id }) => user_id,
                _ => session.bot_user_id_for_chat(chat_id)?,
            };
            let username = session.user(user_id)?.username.clone();
            quill::game_share::game_link(&username, &game.short_name)
        });
        match link {
            Some(link) => {
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.set_status_note("Game link copied to clipboard.", cx);
            }
            None => self.set_status_note("This game has no shareable link.", cx),
        }
    }

    /// A game or add-bot deep link resolved to the bot's private chat. The
    /// bot's chat stays closed; the picker opens over the current view.
    pub(super) fn run_bot_link(
        &mut self,
        chat_id: ChatId,
        action: &quill::state::DeepLinkAction,
        cx: &mut Context<Self>,
    ) -> bool {
        use quill::state::DeepLinkAction;
        let Some(bot_id) = self
            .session()
            .and_then(|session| session.private_chat_user_id(chat_id))
        else {
            return false;
        };
        match action {
            DeepLinkAction::ShareGame {
                game_short_name, ..
            } => self.open_share_game_dialog(bot_id, game_short_name.clone(), cx),
            DeepLinkAction::AddBot { invite, .. } => {
                self.open_add_bot_dialog(bot_id, invite.clone(), cx);
            }
            DeepLinkAction::OpenWebAppLink { .. }
            | DeepLinkAction::OpenMainWebApp { .. }
            | DeepLinkAction::OpenAttachmentBot { .. } => {
                return self.run_web_app_link(chat_id, bot_id, action, cx);
            }
            _ => return false,
        }
        true
    }
}
