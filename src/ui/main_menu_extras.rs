//! Main menu entries beyond the basics (tdesktop `window_main_menu.cpp`):
//! Set Emoji Status, My Stories, and the My Groups / My Channels lists
//! (`window_main_menu_helpers.cpp` `AddMyChannelsBox`), plus the "For ..."
//! duration menu on an emoji status (`info_profile_emoji_status_panel.cpp`).

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::main_menu::{
    EMOJI_STATUS_DURATIONS, EMOJI_STATUS_OTHER, emoji_status_label, is_my_created_chat,
};
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

    /// Set Emoji Status: opens the status picker, or Premium when the
    /// account cannot set one (tdesktop shows the Premium preview).
    pub(super) fn open_emoji_status_picker(&mut self, cx: &mut Context<Self>) {
        let premium = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
            .is_some_and(|u| u.is_premium);
        if !premium {
            self.open_premium(cx);
            return;
        }
        self.open_emoji_sets(cx);
        if let Some(live) = self.live.as_mut()
            && live.driver.load_emoji_status_choices().is_err()
        {
            self.connection.status_note = "Could not load emoji statuses. Retry the action.".into();
        }
        cx.notify();
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

    /// Set `id` as the status for `secs` (`0` keeps it until removed).
    pub(super) fn change_emoji_status_for(&mut self, id: i64, secs: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.change_emoji_status(Some(id), secs) {
                Ok(Some(_)) => "Updating emoji status…".into(),
                Ok(None) => "An emoji status is already updating.".into(),
                Err(_) => "Could not update emoji status. Retry the action.".into(),
            };
        }
        cx.notify();
    }

    /// Right-click on a status: For 1 hour, 2 hours, 8 hours, 2 days, Other...
    pub(super) fn with_status_duration_menu(
        &self,
        cell: Stateful<Div>,
        id: i64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !enabled {
            return cell.into_any_element();
        }
        let owner = cx.entity().downgrade();
        cell.context_menu(move |mut menu, _, _| {
            for (label, secs) in EMOJI_STATUS_DURATIONS {
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.change_emoji_status_for(id, secs, cx));
                }));
            }
            let owner = owner.clone();
            menu.item(
                PopupMenuItem::new(EMOJI_STATUS_OTHER).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.chat_list.status_other_for = Some(id);
                        this.connection.status_note =
                            "Enter the hours below, then press Use custom duration.".into();
                        cx.notify();
                    });
                }),
            )
        })
        .into_any_element()
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
                    } => "Channel",
                    ChatKind::Supergroup { .. } => "Supergroup",
                    _ => "Group",
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
                    "You have not created any channels."
                } else {
                    "You have not created any groups."
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
                    .label("Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.chat_list.my_chats_open = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::MyChats, window, cx);
                    })),
            );
            finish_dialog(
                dialog,
                if channels { "My Channels" } else { "My Groups" }.to_string(),
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
