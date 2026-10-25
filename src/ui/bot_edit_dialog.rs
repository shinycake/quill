//! Edit screen of a bot the viewer owns (Telegram Desktop's group-style
//! edit box, `boxes/peers/edit_peer_info_box.cpp`, bot branch): photo, name
//! and about text, then rows that open @BotFather for the intro, the
//! commands and the settings. Rules and requests: `quill::owned_bots`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::owned_bots::{self, ABOUT_MAX, ManagerScreen, NAME_MAX};
use quill::state::DeepLinkAction;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

pub(super) struct BotEditDialog {
    bot_id: i64,
    name: Entity<InputState>,
    about: Entity<TextareaState>,
    /// The fields hold the server's texts (set once they arrive).
    filled: bool,
    /// A problem found before anything was sent.
    local_error: Option<&'static str>,
}

/// Whether a picked file can be a profile photo.
fn is_photo_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| matches!(e.as_str(), "jpg" | "jpeg" | "png" | "webp"))
}

impl QuillApp {
    /// The profile row for a bot the viewer owns (`userTypeBot.can_be_edited`).
    pub(super) fn bot_edit_row(&self, bot_id: i64, cx: &mut Context<Self>) -> Option<AnyElement> {
        let user = self.session()?.user(bot_id)?;
        if !user.is_bot || !user.can_be_edited {
            return None;
        }
        Some(
            action_row(
                "info-panel-edit-bot",
                Some(IconName::Pencil),
                tr(keys::EDIT_BOT_TITLE),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_bot_edit(bot_id, window, cx);
            }))
            .into_any_element(),
        )
    }

    pub(super) fn open_bot_edit(
        &mut self,
        bot_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MEDIA_DEMO_MODE_EDITING_A_BOT)));
            cx.notify();
            return;
        };
        if live.driver.begin_bot_edit(bot_id).is_err() {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_THE_BOT)));
            cx.notify();
            return;
        }
        // The Toncoin, Stars and Affiliate Program rows.
        let _ = live.driver.ensure_earn_app_config();
        let _ = live
            .driver
            .fetch_earn_balances(quill::earn::EarnPeer::User(bot_id), true, true);
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::DLG_NEW_BOT_NAME))
                .submit_on_enter(false)
        });
        let about = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_ABOUT))
                .auto_grow(2, 5)
                .submit_on_enter(false)
        });
        self.dialogs.bot_edit_dialog = Some(BotEditDialog {
            bot_id,
            name,
            about,
            filled: false,
            local_error: None,
        });
        // The texts arrive from the session on the poll; fill the fields once.
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..100 {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let Ok(done) =
                    this.update_in(cx, |this, window, cx| this.fill_bot_edit(window, cx))
                else {
                    break;
                };
                if done {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Put the loaded texts in the fields; `true` when nothing is left to do.
    fn fill_bot_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.dialogs.bot_edit_dialog.as_ref() else {
            return true;
        };
        if dialog.filled {
            return true;
        }
        let Some(edit) = self.session().and_then(|s| s.bots.owned.edit.clone()) else {
            return true;
        };
        if edit.load_failed {
            return true;
        }
        let (Some(name), Some(about)) = (edit.name, edit.about) else {
            return false;
        };
        let (name_input, about_input) = (dialog.name.clone(), dialog.about.clone());
        name_input.update(cx, |input, cx| input.set_value(name, window, cx));
        about_input.update(cx, |input, cx| input.set_value(about, window, cx));
        if let Some(dialog) = self.dialogs.bot_edit_dialog.as_mut() {
            dialog.filled = true;
        }
        cx.notify();
        true
    }

    pub(super) fn close_bot_edit(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.bot_edit_dialog.take().is_some()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.end_bot_edit();
        }
        cx.notify();
    }

    fn save_bot_edit(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.bot_edit_dialog.as_ref() else {
            return;
        };
        let bot_id = dialog.bot_id;
        let name = dialog.name.read(cx).value().to_string();
        let about = dialog.about.read(cx).value().to_string();
        let Some(edit) = self.session().and_then(|s| s.bots.owned.edit.clone()) else {
            return;
        };
        let (Some(current_name), Some(current_about)) = (edit.name, edit.about) else {
            return;
        };
        let changes = match owned_bots::edit_changes(&current_name, &current_about, &name, &about) {
            Ok(changes) => changes,
            Err(problem) => {
                if let Some(dialog) = self.dialogs.bot_edit_dialog.as_mut() {
                    dialog.local_error = Some(problem);
                }
                cx.notify();
                return;
            }
        };
        if let Some(dialog) = self.dialogs.bot_edit_dialog.as_mut() {
            dialog.local_error = None;
        }
        if let Some(live) = self.live.as_mut()
            && !changes.is_empty()
            && live.driver.save_bot_edit(bot_id, &changes).is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_SEND_THE_CHANGES,
            )));
        }
        cx.notify();
    }

    fn choose_bot_photo(&mut self, bot_id: i64, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MEDIA_CHOOSE_PHOTO).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.set_bot_photo_from(bot_id, &path, cx));
            }
        })
        .detach();
    }

    fn set_bot_photo_from(&mut self, bot_id: i64, path: &Path, cx: &mut Context<Self>) {
        if !is_photo_file(path) {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_CHOOSE_A_JPEG_PNG_OR)));
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .set_bot_photo(bot_id, &path.to_string_lossy())
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_SEND_THE_PHOTO,
            )));
        }
        cx.notify();
    }

    /// Open @BotFather at the screen for this bot (tdesktop `toggleBotManager`).
    fn open_bot_manager(&mut self, bot_id: i64, screen: ManagerScreen, cx: &mut Context<Self>) {
        let Some(username) = self
            .session()
            .and_then(|s| s.user(bot_id))
            .map(|u| u.username.clone())
            .filter(|u| !u.is_empty())
        else {
            return;
        };
        self.close_bot_edit(cx);
        self.note_deep_link_origin();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.resolve_deep_link(DeepLinkAction::OpenUsername {
                domain: owned_bots::BOT_MANAGER_USERNAME.to_string(),
                start_param: Some(screen.start_parameter(&username)),
                post: None,
                story_id: None,
            });
        }
    }

    pub(super) fn build_bot_edit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::BotEdit, |this, _, cx| {
            this.close_bot_edit(cx);
        });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.bot_edit_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let (bot_id, name, about) = (state.bot_id, state.name.clone(), state.about.clone());
            let local_error = state.local_error;
            let muted = cx.theme().muted_foreground;
            let danger = cx.theme().danger;
            let edit = this.session().and_then(|s| s.bots.owned.edit.clone());
            let loaded = edit
                .as_ref()
                .is_some_and(|e| e.name.is_some() && e.about.is_some());
            let load_failed = edit.as_ref().is_some_and(|e| e.load_failed);
            let saving = edit.as_ref().is_some_and(|e| e.saving > 0);
            let saved = edit.as_ref().is_some_and(|e| e.saved > 0);
            let server_error = edit.as_ref().and_then(|e| e.error.clone());
            let username = this
                .session()
                .and_then(|s| s.user(bot_id))
                .map(|u| u.username.clone())
                .unwrap_or_default();
            let name_len = name.read(cx).value().chars().count();
            let about_len = about.read(cx).value().chars().count();

            let status = if load_failed {
                Some((tr(keys::Q_MEDIA_COULD_NOT_LOAD_THE_BOTS).to_string(), true))
            } else if let Some(problem) = local_error {
                Some((problem.to_string(), true))
            } else if let Some(error) = server_error {
                Some((error, true))
            } else if saving {
                Some((tr(keys::Q_MEDIA_SAVING).to_string(), false))
            } else if saved {
                Some((tr(keys::Q_MEDIA_SAVED).to_string(), false))
            } else if !loaded {
                Some((tr(keys::Q_MEDIA_LOADING).to_string(), false))
            } else {
                None
            };

            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::Q_MEDIA_NAME)),
                        )
                        .child(Input::new(&name).aria_label(tr(keys::DLG_NEW_BOT_NAME)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(if name_len > NAME_MAX { danger } else { muted })
                                .child(format!("{name_len}/{NAME_MAX}")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::Q_MEDIA_ABOUT)),
                        )
                        .child(Textarea::new(&about).aria_label(tr(keys::Q_MEDIA_BOT_ABOUT_TEXT)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(if about_len > ABOUT_MAX { danger } else { muted })
                                .child(format!(
                                    "{about_len}/{ABOUT_MAX}. Shown on the bot's profile."
                                )),
                        ),
                )
                .child(
                    action_row(
                        "bot-edit-photo",
                        Some(IconName::Image),
                        tr(keys::Q_MEDIA_CHANGE_PROFILE_PHOTO),
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_bot_photo(bot_id, cx);
                    })),
                );
            if let Some((line, is_error)) = status {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(if is_error { danger } else { muted })
                        .child(line),
                );
            }
            body = body.children(this.bot_edit_earn_rows(bot_id, cx));
            if !username.is_empty() {
                let mut rows = div().flex().flex_col().gap_0p5();
                for screen in ManagerScreen::ALL {
                    rows = rows.child(
                        action_row(
                            ("bot-edit-manager", screen as u32 as u64),
                            Some(if screen == ManagerScreen::Settings {
                                IconName::Settings
                            } else {
                                IconName::Pencil
                            }),
                            screen.label(),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_bot_manager(bot_id, screen, cx);
                                this.close_kit_dialog_if_done(DialogKind::BotEdit, window, cx);
                            },
                        )),
                    );
                }
                body = body.child(rows).child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MEDIA_THESE_OPEN_BOTFATHER_WITH_THIS)),
                );
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("bot-edit-close")
                        .label(tr(keys::CLOSE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_bot_edit(cx);
                            this.close_kit_dialog_if_done(DialogKind::BotEdit, window, cx);
                        })),
                )
                .child(
                    Button::new("bot-edit-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .disabled(!loaded || saving || name_len > NAME_MAX || about_len > ABOUT_MAX)
                        .on_click(cx.listener(|this, _, _, cx| this.save_bot_edit(cx))),
                );
            let content = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .child(footer)
                .into_any_element();
            let content = Rc::new(RefCell::new(Some(content)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::EDIT_BOT_TITLE)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |slot, _, _| {
                        let body = content
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        slot.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    BotEdit => DialogSpec::new(
        5420,
        |app| app.dialogs.bot_edit_dialog.is_some(),
        QuillApp::build_bot_edit_dialog,
    ),
}
