//! Message selection mode, as in Telegram Desktop: once a message is
//! selected ("Select" in its menu), a click anywhere on a row toggles it,
//! each row shows a check circle, and ⌘C copies the selected messages.
//! Pressing and dragging over rows, and the keyboard, live in
//! `selection_drag` and `shortcut_pack`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::motion;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with, trn};
use quill::telegram::envelope::effective_content;

/// A history row's selection-mode decoration (see
/// [`QuillApp::selection_row`]).
#[derive(Default)]
pub(super) struct SelectionRow {
    pub(super) overlay: Option<AnyElement>,
    /// Opacity of the selected-row tint, `0..=1`.
    pub(super) tint: f32,
    /// How far an outgoing bubble slides left, px.
    pub(super) slide: f32,
}

impl QuillApp {
    /// Whether selection mode is on in `chat_id`.
    pub(super) fn selecting_in(&self, chat_id: ChatId) -> bool {
        self.forward_draft()
            .is_some_and(|draft| draft.from_chat_id == chat_id)
    }

    /// Follow selection mode for the motion state; returns the check fade
    /// (`0..=1`) and the top bar slide (`0..=1`). Asks the frame clock for
    /// frames while either moves. An inactive window snaps.
    pub(super) fn selection_motion(&self, chat_id: ChatId, cx: &mut Context<Self>) -> (f32, f32) {
        let now = std::time::Instant::now();
        // A drag that is about to select rows shows the mode already.
        let on = self.selection_visible_in(chat_id);
        let count = on.then(|| self.selection_shown_count(chat_id));
        let mut fx = self.frame.motion.selection.borrow_mut();
        if let Some(count) = count {
            fx.last_count = count;
        }
        fx.sync_mode(on, self.frame.window_active.get(), now);
        if fx.moving(now) {
            self.request_animation_tick(60, cx);
        }
        (fx.mode(now), fx.bar(now))
    }

    /// What a history row shows for selection mode: the fading check
    /// circle (an overlay that also takes the row's clicks while the mode
    /// is on), the selection tint, and how far an outgoing bubble slides
    /// aside (`Message::draw`, `history_view_message.cpp:2313`).
    pub(super) fn selection_row(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        pending: bool,
        // The wide layout's check position from the history's left edge
        // (`chat_wide::selection_right`); `None` puts it at the right edge.
        wide_right: Option<f32>,
        cx: &mut Context<Self>,
    ) -> SelectionRow {
        let (mode, _) = self.selection_motion(chat_id, cx);
        // The row's state, or the drag's while it covers the row.
        let selected = self.selection_shown(chat_id, message_id);
        if mode <= 0. && !selected {
            return SelectionRow::default();
        }
        let now = std::time::Instant::now();
        let checked = self.frame.motion.selection.borrow_mut().check(
            message_id.0,
            selected,
            self.frame.window_active.get(),
            now,
        );
        let interactive = self.selecting_in(chat_id);
        let focused = interactive && self.message_ui.selection_focus == Some(message_id);
        let ring_color = cx.theme().ring;
        let forward = self.selection_forward_drag(chat_id, message_id);
        let overlay = (mode > 0.).then(|| {
            let ring = cx.theme().background;
            let (fill, tick) = motion::check_frame(checked);
            let size = motion::CHECK_SIZE;
            // The circle slides in from the right edge while it fades.
            let edge = 11.5 - 15. * (1. - mode);
            div()
                .id(("selection-overlay", message_id.0 as u64))
                .absolute()
                .inset_0()
                .when(focused, |this| {
                    // Keyboard focus (Up / Down while selecting).
                    this.rounded_md().border_2().border_color(ring_color)
                })
                .when(interactive, |this| {
                    // Takes the row's clicks (links and media don't fire)
                    // but lets the wheel through to the list.
                    this.block_mouse_except_scroll()
                        .cursor_pointer()
                        // A click toggles the row on release, Shift+click
                        // selects the range from the last clicked row, and
                        // a drag selects or deselects the rows it covers
                        // (`selection_drag`).
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                this.selection_press_row(
                                    chat_id, message_id, pending, event, window, cx,
                                );
                            }),
                        )
                })
                // A press on a selected row, moved, lifts the whole selection
                // for a chat row (`uponSelected`).
                .map(|this| {
                    super::forward_drag_source::draggable(
                        this,
                        interactive.then_some(forward).flatten(),
                    )
                })
                .child(super::anim_layer::occluder(
                    div()
                        .absolute()
                        .map(|this| match wide_right {
                            // Right of the wide column, not at the edge
                            // (`Message::draw`, `ElementChatMode::Wide`).
                            Some(right) => this.left(px(right
                                - 2. * super::chat_wide_ui::ROW_INSET
                                - edge
                                - size)),
                            None => this.right(px(edge)),
                        })
                        .bottom(px(motion::CHECK_BOTTOM_SKIP + 1.))
                        .size(px(size))
                        .opacity(mode)
                        .rounded_full()
                        .border_2()
                        .border_color(ring)
                        .bg(gpui_kit::black().opacity(0.3))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(fill > 0., |this| {
                            this.child(
                                div()
                                    .size(px((size - 4.) * fill))
                                    .rounded_full()
                                    .bg(cx.theme().success),
                            )
                        })
                        .when(tick > 0., |this| {
                            // The tick wipes in from the left.
                            this.child(
                                div()
                                    .absolute()
                                    .left(px(2.))
                                    .top(px(2.))
                                    .w(px(12. * tick))
                                    .h(px(12.))
                                    .overflow_hidden()
                                    .child(
                                        div().flex_none().size(px(12.)).child(
                                            Icon::new(gpui_kit::assets::IconName::Check)
                                                .size(px(12.))
                                                .text_color(gpui_kit::white()),
                                        ),
                                    ),
                            )
                        }),
                ))
                .into_any_element()
        });
        SelectionRow {
            overlay,
            tint: checked,
            slide: motion::bubble_slide(mode),
        }
    }

    /// The conversation header while selecting: Telegram Desktop swaps the
    /// top bar for "Forward N", "Delete N" and "Cancel", sliding the
    /// buttons down over 150 ms (`toggleSelectedControls`).
    pub(super) fn with_selection_bar(
        &self,
        chat_id: ChatId,
        header: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (_, slide) = self.selection_motion(chat_id, cx);
        if slide <= 0. {
            return header.into_any_element();
        }
        let bar = self.selection_bar(chat_id, slide, cx);
        div()
            .relative()
            .overflow_hidden()
            .child(header)
            .child(bar)
            .into_any_element()
    }

    /// Select `ids` that are not selected yet, up to the selection limit
    /// (`Data::MaxSelectedItems`).
    pub(super) fn add_to_selection(&mut self, chat_id: ChatId, ids: Vec<MessageId>) {
        self.drop_deleted_selection();
        let Some(draft) = self.share.pending_forward.as_mut() else {
            return;
        };
        let room = quill::selection_pin::room_for(draft.count());
        let fresh: Vec<_> = ids
            .into_iter()
            .filter(|id| !draft.contains(*id))
            .take(room)
            .collect();
        for id in fresh {
            draft.toggle(chat_id, id, false);
        }
    }

    /// "Select up to this message" (`selectItemsUpTo`): fill the gap
    /// between the message and the nearest selected one.
    pub(super) fn select_up_to(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        let ids = self.loaded_selectable_ids(chat_id);
        let span = quill::selection_pin::up_to(
            &ids,
            &draft.message_ids,
            message_id,
            quill::selection_pin::room_for(draft.count()),
        );
        self.add_to_selection(chat_id, span);
        self.message_ui.selection_anchor = Some(message_id);
        self.message_ui.selection_focus = Some(message_id);
        cx.notify();
    }

    /// Whether "Select up to this message" applies to `message_id`: a
    /// selection is on in the chat and the message is not part of it, with
    /// a selected message among the loaded ones.
    pub(super) fn can_select_up_to(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        let Some(draft) = self.forward_draft() else {
            return false;
        };
        draft.from_chat_id == chat_id
            && !draft.contains(message_id)
            && quill::selection_pin::room_for(draft.count()) > 0
            && !quill::selection_pin::up_to(
                &self.loaded_selectable_ids(chat_id),
                &draft.message_ids,
                message_id,
                1,
            )
            .is_empty()
    }

    /// Loaded, sent messages of the chat in history order.
    pub(super) fn loaded_selectable_ids(&self, chat_id: ChatId) -> Vec<MessageId> {
        self.session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .map(|history| {
                history
                    .ordered()
                    .into_iter()
                    .filter(|m| m.can_pin())
                    .map(|m| m.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// "Copy Selected as Text" (Cmd/Ctrl+C does the same).
    pub(super) fn copy_selection_text(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, text)) = self.selected_messages_text() else {
            return;
        };
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.connection
            .set_note(StatusNote::info(tr(keys::TEXT_COPIED)));
        cx.notify();
    }

    /// The media messages of the selection, oldest first.
    fn selected_media(&self) -> Vec<(MessageId, quill::message_menu::MediaTarget)> {
        let Some(draft) = self.forward_draft() else {
            return Vec::new();
        };
        let Some(history) = self
            .session()
            .and_then(|s| s.histories.get(&draft.from_chat_id.0))
        else {
            return Vec::new();
        };
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        ids.into_iter()
            .filter_map(|id| {
                let message = history.messages.get(&id.0)?;
                let target = quill::message_menu::media_target(effective_content(
                    &message.content,
                    message.ephemeral.as_ref(),
                ))?;
                Some((id, target))
            })
            .collect()
    }

    /// Whether the selection holds anything to save.
    pub(super) fn selection_has_media(&self, chat_id: ChatId) -> bool {
        self.selecting_in(chat_id) && !self.selected_media().is_empty()
    }

    /// Files of the selected media that are not on disk yet.
    pub(super) fn selection_missing_files(&self, chat_id: ChatId) -> Vec<quill::ids::FileId> {
        if !self.selecting_in(chat_id) {
            return Vec::new();
        }
        self.selected_media()
            .into_iter()
            .filter(|(id, _)| self.menu_media_local(chat_id, *id).is_none())
            .map(|(_, target)| target.file_id)
            .collect()
    }

    /// "Download Selected": start the downloads of every selected media
    /// message that is not on disk yet (Save then copies them out).
    pub(super) fn download_selection(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.forward_draft().map(|d| d.from_chat_id) else {
            return;
        };
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let missing = self.selection_missing_files(chat_id);
        if missing.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_media_files(&missing);
        }
        self.connection.set_note(match missing.len() {
            1 => StatusNote::silent(tr(keys::Q_CHAT_DOWNLOADING_1_FILE)),
            n => StatusNote::silent(trn(keys::Q_CHAT_DOWNLOADING_FILES, n)),
        });
        cx.notify();
    }

    /// "Save Selected": pick a folder and copy the downloaded media of the
    /// selection into it; media still downloading is requested first.
    pub(super) fn save_selection(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.forward_draft().map(|d| d.from_chat_id) else {
            return;
        };
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let media = self.selected_media();
        if media.is_empty() {
            return;
        }
        let mut files = Vec::new();
        let mut missing = Vec::new();
        for (id, target) in &media {
            match self.menu_media_local(chat_id, *id) {
                Some((_, path)) => {
                    let local = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();
                    files.push((
                        path,
                        quill::message_menu::suggested_save_name(target, &local),
                    ));
                }
                None => missing.push(target.file_id),
            }
        }
        if !missing.is_empty() {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.ensure_media_files(&missing);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DOWNLOADING_CHOOSE_SAVE_AGAIN_WHEN,
            )));
            cx.notify();
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(keys::SETTINGS_SAVE).into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picker.await else {
                return;
            };
            let Some(dir) = dirs.into_iter().next() else {
                return;
            };
            let saved = cx
                .background_executor()
                .spawn(async move {
                    let mut used = std::collections::HashSet::new();
                    let mut saved = 0usize;
                    for (path, name) in files {
                        let (dest, claimed) = unique_destination(&dir, &name, &mut used);
                        if std::fs::copy(&path, &dest).is_ok() {
                            saved += 1;
                        } else if claimed {
                            // Drop the empty file `unique_destination` created,
                            // never a path it did not create.
                            let _ = std::fs::remove_file(&dest);
                        }
                    }
                    saved
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection.set_note(match saved {
                    1 => StatusNote::silent(tr(keys::Q_CHAT_SAVED_1_FILE)),
                    n => StatusNote::silent(trn(keys::Q_CHAT_SAVED_FILES, n)),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether the selection can be reported: other people's sent
    /// messages in a chat that is not Saved Messages (Telegram Desktop's
    /// `suggestReport`).
    pub(super) fn selection_reportable(&self, chat_id: ChatId) -> bool {
        let Some(draft) = self.forward_draft() else {
            return false;
        };
        let Some(session) = self.session() else {
            return false;
        };
        if draft.from_chat_id != chat_id || session.is_saved_messages(chat_id) {
            return false;
        }
        let Some(history) = session.histories.get(&chat_id.0) else {
            return false;
        };
        !draft.message_ids.is_empty()
            && draft.message_ids.iter().all(|id| {
                history
                    .messages
                    .get(&id.0)
                    .is_some_and(|m| !m.is_outgoing && !m.pending && m.id.0 > 0)
            })
    }

    /// "Report N": the Report flow for every selected message.
    pub(super) fn report_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        self.clear_forward(window, cx);
        self.open_message_report(draft.from_chat_id, ids, cx);
    }

    /// The selected messages as text, as Telegram Desktop copies them: a
    /// single message as its text; several as "[date time] Name: text"
    /// lines, oldest first.
    pub(super) fn selected_messages_text(&self) -> Option<(ChatId, String)> {
        let draft = self.forward_draft()?;
        let session = self.session()?;
        let history = session.histories.get(&draft.from_chat_id.0)?;
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        let messages: Vec<_> = ids
            .iter()
            .filter_map(|id| history.messages.get(&id.0))
            .collect();
        let body = |message: &quill::state::HistoryMessage| {
            Self::message_copyable_text(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
            .filter(|text| !text.is_empty())
            // Media without text reads as its kind ("Sticker", "Video"…).
            .unwrap_or_else(|| quill::state::effective_preview(message))
        };
        let text = match messages.as_slice() {
            [] => return None,
            [only] => body(only),
            many => many
                .iter()
                .map(|message| {
                    let date = quill::local_time::civil_local(i64::from(message.date));
                    format!(
                        "[{:02}.{:02}.{:02} {:02}:{:02}] {}: {}",
                        date.day,
                        date.month,
                        date.year.rem_euclid(100),
                        date.hour,
                        date.minute,
                        session.message_author_name(message),
                        body(message)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        (!text.trim().is_empty()).then_some((draft.from_chat_id, text))
    }

    /// "Delete N" for the selection: Telegram Desktop's confirmation, with
    /// "delete for everyone" (checked by default) when every selected
    /// message is yours and the chat isn't Saved Messages.
    pub(super) fn confirm_delete_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        let (chat_id, ids) = (draft.from_chat_id, draft.message_ids.clone());
        let Some(session) = self.session() else {
            return;
        };
        let all_outgoing = session.histories.get(&chat_id.0).is_some_and(|history| {
            ids.iter().all(|id| {
                history
                    .messages
                    .get(&id.0)
                    .is_some_and(|message| message.is_outgoing)
            })
        });
        let offer_revoke = all_outgoing && !session.is_saved_messages(chat_id);
        self.confirm_delete_ids(chat_id, ids, offer_revoke, window, cx);
    }

    /// The delete box for `ids`; `offer_revoke` adds the checked-by-default
    /// "delete for everyone" box.
    pub(super) fn confirm_delete_ids(
        &mut self,
        chat_id: ChatId,
        ids: Vec<MessageId>,
        offer_revoke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let revoke_label =
            offer_revoke.then(
                || match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
                    Some(quill::telegram::envelope::ChatKind::Private { user_id }) => session
                        .user(user_id.0)
                        .map(|user| {
                            tr_with(
                                keys::Q_CHAT_ALSO_DELETE_FOR,
                                &[("first_name", &user.first_name.to_string())],
                            )
                        })
                        .unwrap_or_else(|| tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into()),
                    _ => tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into(),
                },
            );
        let question = if ids.len() == 1 {
            tr(keys::SELECTED_DELETE_SURE_THIS).to_string()
        } else {
            trn(keys::Q_CHAT_DO_YOU_WANT_TO_DELETE, ids.len())
        };
        let revoke = std::rc::Rc::new(std::cell::Cell::new(true));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids, revoke) = (app.clone(), ids.clone(), revoke.clone());
            let checkbox_state = revoke.clone();
            let label = revoke_label.clone();
            alert
                .description(question.clone())
                .when_some(label, |alert, label| {
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        content.child(
                            gpui_kit::component::checkbox::Checkbox::new("delete-selection-revoke")
                                .label(label.clone())
                                .checked(state.get())
                                .on_click(move |checked, window, _| {
                                    state.set(*checked);
                                    window.refresh();
                                }),
                        )
                    })
                })
                .ok_text(tr(keys::BOX_DELETE))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let revoke = revoke.get();
                    let ids = ids.clone();
                    let _ = app.update(cx, |this, cx| {
                        this.delete_selection(chat_id, &ids, revoke, cx);
                    });
                    true
                })
        });
    }

    pub(super) fn delete_selection(
        &mut self,
        chat_id: ChatId,
        ids: &[MessageId],
        revoke: bool,
        cx: &mut Context<Self>,
    ) {
        // The box may have sat open while some of them went away.
        let ids: Vec<MessageId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.session()
                    .is_none_or(|session| !session.message_deleted(chat_id, *id))
            })
            .collect();
        let ids = ids.as_slice();
        if !ids.is_empty() {
            self.begin_vanish(chat_id, ids);
            if let Some(live) = self.live.as_mut() {
                if live.driver.delete_selected(chat_id, ids, revoke).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_CHAT_COULD_NOT_DELETE_MESSAGES,
                    )));
                }
            } else if self.demo_session.is_some() {
                for id in ids {
                    self.apply_demo_delete(chat_id, *id);
                }
            }
        }
        self.share.pending_forward = None;
        self.share.forward_picker_open = false;
        cx.notify();
    }

    /// Telegram Desktop's delete box for one message: "Do you want to delete
    /// this message?" with "Also delete for {name}" (private chats) or
    /// "Delete for everyone", checked by default, when deleting for everyone
    /// is possible.
    pub(super) fn open_delete_dialog(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_delete_dialog_with(confirm, None, window, cx);
    }

    /// The delete box, with the admin checkboxes of a group moderator
    /// (Report Spam, Delete all from the user, Ban the user) when
    /// `moderation` offers them.
    pub(super) fn open_delete_dialog_with(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        moderation: Option<super::message_menu_ui::ModerationOffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = confirm.can_revoke.then(|| {
            let session = self.session();
            match session
                .and_then(|s| s.chats.get(&confirm.chat_id.0))
                .map(|chat| &chat.kind)
            {
                Some(quill::telegram::envelope::ChatKind::Private { user_id }) => session
                    .and_then(|s| s.user(user_id.0))
                    .map(|user| {
                        tr_with(
                            keys::Q_CHAT_ALSO_DELETE_FOR,
                            &[("first_name", &user.first_name.to_string())],
                        )
                    })
                    .unwrap_or_else(|| tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into()),
                _ => tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into(),
            }
        });
        let revoke = std::rc::Rc::new(std::cell::Cell::new(confirm.revoke));
        // Report Spam / Delete all / Ban: unchecked until the admin ticks them.
        let choice = std::rc::Rc::new(std::cell::Cell::new(
            quill::connect::ModerationChoice::default(),
        ));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, revoke, confirm) = (app.clone(), revoke.clone(), confirm.clone());
            let checkbox_state = revoke.clone();
            let (moderation_state, moderation_offer) = (choice.clone(), moderation.clone());
            let on_ok_choice = choice.clone();
            let on_ok_offer = moderation.clone();
            alert
                .description(tr(keys::SELECTED_DELETE_SURE_THIS))
                .when(label.is_some() || moderation_offer.is_some(), |alert| {
                    let label = label.clone();
                    alert.content(move |content, _, _| {
                        let state = checkbox_state.clone();
                        let mut content = content;
                        if let Some(label) = label.clone() {
                            content = content.child(
                                gpui_kit::component::checkbox::Checkbox::new(
                                    "delete-message-revoke",
                                )
                                .label(label)
                                .checked(state.get())
                                .on_click(
                                    move |checked, window, _| {
                                        state.set(*checked);
                                        window.refresh();
                                    },
                                ),
                            );
                        }
                        if let Some(offer) = moderation_offer.clone() {
                            type Get = fn(quill::connect::ModerationChoice) -> bool;
                            type Set = fn(&mut quill::connect::ModerationChoice, bool);
                            let name = offer.user_name.clone();
                            let rows: [(&'static str, String, bool, Get, Set); 3] = [
                                (
                                    "delete-moderate-spam",
                                    tr(keys::Q_CHAT_REPORT_SPAM_2).to_string(),
                                    offer.options.report_spam,
                                    |c| c.report_spam,
                                    |c, v| c.report_spam = v,
                                ),
                                (
                                    "delete-moderate-all",
                                    tr_with(
                                        keys::DELETE_ALL_FROM_USER,
                                        &[("user", &name.to_string())],
                                    ),
                                    offer.options.delete_all,
                                    |c| c.delete_all,
                                    |c, v| c.delete_all = v,
                                ),
                                (
                                    "delete-moderate-ban",
                                    tr_with(keys::Q_CHAT_BAN, &[("name", &name.to_string())]),
                                    offer.options.ban_or_restrict,
                                    |c| c.ban,
                                    |c, v| c.ban = v,
                                ),
                            ];
                            for (id, text, available, get, set) in rows {
                                if !available {
                                    continue;
                                }
                                let state = moderation_state.clone();
                                let checked = get(state.get());
                                content = content.child(
                                    gpui_kit::component::checkbox::Checkbox::new(id)
                                        .label(text)
                                        .checked(checked)
                                        .on_click(move |checked, window, _| {
                                            let mut now = state.get();
                                            set(&mut now, *checked);
                                            state.set(now);
                                            window.refresh();
                                        }),
                                );
                            }
                            // tdesktop's expander under "Ban": keep the
                            // member, take away what they may send.
                            if offer.can_restrict_instead && moderation_state.get().ban {
                                let state = moderation_state.clone();
                                content = content.child(
                                    gpui_kit::component::checkbox::Checkbox::new(
                                        "delete-moderate-restrict",
                                    )
                                    .label(tr(keys::Q_CHAT_ONLY_RESTRICT_THEY_STAY_AND))
                                    .checked(state.get().restrict_instead)
                                    .on_click(
                                        move |checked, window, _| {
                                            let mut now = state.get();
                                            now.restrict_instead = *checked;
                                            state.set(now);
                                            window.refresh();
                                        },
                                    ),
                                );
                            }
                        }
                        content
                    })
                })
                .ok_text(tr(keys::BOX_DELETE))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let mut confirm = confirm.clone();
                    confirm.revoke = confirm.can_revoke && revoke.get();
                    let offer = on_ok_offer.clone();
                    let picked = match &offer {
                        Some(offer) => on_ok_choice.get().clamp(offer.options),
                        None => quill::connect::ModerationChoice::default(),
                    };
                    let _ = app.update(cx, |this, cx| {
                        // Reports and "delete all" name the user's
                        // messages, so they go out before the delete.
                        if let (true, Some(offer), Some(live)) =
                            (picked.any(), offer, this.live.as_mut())
                        {
                            let _ = live.driver.moderate_message(
                                offer.chat_id,
                                &[confirm.message_id],
                                offer.user_id,
                                picked,
                            );
                        }
                        this.message_ui.pending_delete = Some(confirm);
                        this.confirm_delete(cx);
                    });
                    true
                })
        });
    }
}

mod save_destination;
use save_destination::unique_destination;
