//! Main menu entries beyond the basics (tdesktop `window_main_menu.cpp`):
//! Set Emoji Status, My Stories, and the My Groups / My Channels lists
//! (`window_main_menu_helpers.cpp` `AddMyChannelsBox`). The emoji status
//! selector it opens lives in `media_panel/status.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::main_menu::{emoji_status_label, is_my_created_chat};
use quill::telegram::envelope::ChatKind;

/// One row of the My Groups / My Channels list.
struct MyChatRow {
    id: ChatId,
    title: String,
    kind: &'static str,
}

impl QuillApp {
    /// The menu's emoji status entry text, from the account's current status.
    pub(super) fn emoji_status_menu_label(&self) -> &'static str {
        let has_status = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
            .is_some_and(|u| u.emoji_status_id != 0);
        emoji_status_label(has_status)
    }

    /// Set Emoji Status: opens the status selector under the menu, or
    /// the Premium preview when the account has no Premium (tdesktop
    /// `MainMenu::chooseEmojiStatus`).
    pub(super) fn open_emoji_status_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session().is_some_and(|s| s.my_is_premium()) {
            self.open_premium_at(quill::premium_hub::PremiumFeature::EmojiStatus, cx);
            return;
        }
        self.open_status_selector(point(px(8.), px(52.)), window, cx);
    }

    /// My Stories: the account's own story page.
    pub(super) fn open_my_stories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(me) = self.session().and_then(|s| s.my_user_id) {
            self.open_story_page(ChatId(me), window, cx);
        }
    }

    pub(super) fn open_my_chats(&mut self, channels: bool, cx: &mut Context<Self>) {
        self.chat_list.my_chats_open = Some(channels);
        cx.notify();
    }

    /// Set `id` as the status for `secs` (`0` keeps it until removed);
    /// `None` removes the status. A collectible goes out as its gift.
    pub(super) fn change_emoji_status_for(
        &mut self,
        id: Option<i64>,
        secs: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let collectible = id.is_some_and(|id| {
                live.driver
                    .session
                    .stickers
                    .emoji
                    .upgraded_gift_statuses
                    .iter()
                    .any(|s| {
                        s.gift
                            .as_ref()
                            .is_some_and(|g| g.model_custom_emoji_id == id)
                    })
            });
            let sent = match id {
                Some(id) if collectible => live.driver.change_collectible_emoji_status(id, secs),
                _ => live.driver.change_emoji_status(id, secs),
            };
            self.connection.set_note(match sent {
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MISC_UPDATING_EMOJI_STATUS)),
                Ok(None) => StatusNote::silent(tr(keys::Q_MISC_AN_EMOJI_STATUS_IS_ALREADY)),
                Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_EMOJI_STATUS)),
            });
        } else if let Some(session) = self.demo_session.as_mut() {
            // The offline demo accepts the change the way TDLib would.
            if let Some(me) = session.my_user_id
                && let Some(user) = session.users.get_mut(&me)
            {
                user.emoji_status_id = id.unwrap_or(0);
            }
        }
        cx.notify();
    }

    fn my_chat_rows(&self, channels: bool) -> Vec<MyChatRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let mut rows: Vec<_> = session
            .chats
            .values()
            .filter(|c| is_my_created_chat(c, channels))
            .map(|c| MyChatRow {
                id: c.id,
                title: c.title.clone(),
                kind: match c.kind {
                    ChatKind::Supergroup {
                        is_channel: true, ..
                    } => tr(keys::Q_MISC_CHANNEL),
                    ChatKind::Supergroup { .. } => tr(keys::Q_MISC_SUPERGROUP),
                    _ => tr(keys::Q_MISC_GROUP),
                },
            })
            .collect();
        rows.sort_by_key(|r| r.title.to_lowercase());
        rows
    }

    pub(super) fn build_my_chats_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::MyChats, |this, _, cx| {
            this.chat_list.my_chats_open = None;
            cx.notify();
        });
        app.update(cx, |this, cx| {
            let channels = this.chat_list.my_chats_open.unwrap_or(false);
            let rows = this.my_chat_rows(channels);
            let muted = cx.theme().muted_foreground;
            let mut body = div().id("my-chats").flex().flex_col().gap_1();
            if rows.is_empty() {
                body = body.child(div().text_sm().text_color(muted).child(if channels {
                    tr(keys::Q_MISC_YOU_HAVE_NOT_CREATED_ANY)
                } else {
                    tr(keys::Q_MISC_YOU_HAVE_NOT_CREATED_ANY_2)
                }));
            }
            for row in rows {
                let chat_id = row.id;
                body = body.child(
                    Button::new(SharedString::from(format!("my-chat-{}", chat_id.0)))
                        .label(format!("{} · {}", row.title, row.kind))
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.chat_list.my_chats_open = None;
                            this.select_listed_chat(chat_id, window, cx);
                            this.close_kit_dialog_if_done(DialogKind::MyChats, window, cx);
                        })),
                );
            }
            let footer = div().flex().justify_end().child(
                Button::new("my-chats-close")
                    .label(tr(keys::CLOSE))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.chat_list.my_chats_open = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::MyChats, window, cx);
                    })),
            );
            finish_dialog(
                dialog,
                if channels {
                    tr(keys::Q_MISC_MY_CHANNELS)
                } else {
                    tr(keys::Q_MISC_MY_GROUPS)
                }
                .to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The main menu's My Groups / My Channels list.
    MyChats => DialogSpec::new(
        1410,
        |app| app.chat_list.my_chats_open.is_some(),
        QuillApp::build_my_chats_dialog,
    ),
}
