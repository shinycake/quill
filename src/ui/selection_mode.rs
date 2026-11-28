//! Message selection mode, as in Telegram Desktop: once a message is
//! selected ("Select" in its menu), a click anywhere on a row toggles it,
//! each row shows a check circle, and ⌘C copies the selected messages.
//! Pressing and dragging over rows, and the keyboard, live in
//! `selection_drag` and `shortcut_pack`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_ui_state::A11yFocus;
use super::motion::{self, CheckOf};
use super::select_check;
use crate::ui::status_note::StatusNote;
use crate::ui::vanish::Deletion;
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
    /// The row a Shift+click or Shift+arrow range runs from: the selection's
    /// own, so it is gone with the selection.
    pub(super) fn selection_anchor(&self) -> Option<MessageId> {
        self.share
            .forward
            .draft()
            .and_then(|draft| draft.range_anchor)
    }

    /// Make `id` the range anchor of the selection on now; nothing without one.
    pub(super) fn set_selection_anchor(&mut self, id: MessageId) {
        if let Some(draft) = self.share.forward.draft_mut() {
            draft.range_anchor = Some(id);
        }
    }

    /// Whether selection mode is on in `chat_id`, in the conversation the
    /// view shows there (`forward_draft` hides one made in another topic).
    /// A forward bar is no selection mode, in the chat it waits in or the
    /// one its messages came from: a bar into the selection's own forum
    /// kept "3 selected" and a Delete over hidden ids in every topic.
    pub(super) fn selecting_in(&self, chat_id: ChatId) -> bool {
        self.share.forward.selection().is_some()
            && self
                .forward_draft()
                .is_some_and(|draft| draft.is_from(chat_id, self.open_topic_id(chat_id)))
    }

    /// Whether selection motion animates: in the active window, and never
    /// in a test, where the wall clock would make the drawn frame flaky
    /// (as `surface_reveal::animate`).
    fn selection_animates(&self) -> bool {
        self.frame.window_active.get() && !cfg!(test)
    }

    /// Follow selection mode for the motion state; returns the check fade
    /// (`0..=1`) and the top bar slide (`0..=1`). Asks the frame clock for
    /// frames while either moves. An inactive window snaps.
    pub(super) fn selection_motion(&self, chat_id: ChatId, cx: &mut Context<Self>) -> (f32, f32) {
        let now = std::time::Instant::now();
        // A drag that is about to select rows shows the mode already.
        let on = self.selection_visible_in(chat_id);
        let count = on.then(|| self.selection_shown_count(chat_id));
        // A re-entrant frame skips this one's motion instead of panicking.
        let Ok(mut fx) = self.frame.motion.selection.try_borrow_mut() else {
            return (if on { 1. } else { 0. }, if on { 1. } else { 0. });
        };
        if let Some(count) = count {
            fx.last_count = count;
        }
        fx.sync_mode(on, self.selection_animates(), now);
        if fx.moving(now) {
            self.request_animation_tick(60, cx);
        }
        (fx.mode(now), fx.bar(now))
    }

    /// Picked-progress (`0..=1`) of one check, moving for a few frames
    /// after `selected` flips.
    fn check_progress(&self, chat_id: ChatId, check: CheckOf, selected: bool) -> f32 {
        // A re-entrant frame shows the settled state.
        self.frame.motion.selection.try_borrow_mut().map_or(
            if selected { 1. } else { 0. },
            |mut fx| {
                fx.check(
                    chat_id.0,
                    check,
                    selected,
                    self.selection_animates(),
                    std::time::Instant::now(),
                )
            },
        )
    }

    /// What the rows of `chat_id` sit on: its wallpaper, else the theme's
    /// own background.
    fn history_ground(&self, chat_id: ChatId, cx: &App) -> select_check::Ground {
        match self.chat_look(Some(chat_id.0), cx).wallpaper {
            Some(_) => select_check::Ground::Scene,
            None => select_check::Ground::Surface,
        }
    }

    /// The mark of one album item (a photo, a video, a file): its own
    /// check fading in with selection mode, filled once it is picked.
    pub(super) fn tile_mark(
        &self,
        chat_id: ChatId,
        id: MessageId,
        cx: &mut Context<Self>,
    ) -> select_check::Mark {
        let (mode, _) = self.selection_motion(chat_id, cx);
        let selected = self.selection_shown(chat_id, id);
        if mode <= 0. && !selected {
            return select_check::Mark::default();
        }
        select_check::Mark {
            mode,
            picked: self.check_progress(chat_id, CheckOf::Tile(id.0), selected),
        }
    }

    /// What a history row shows for selection mode: the fading check
    /// circle (an overlay that also takes the row's clicks while the mode
    /// is on), the selection tint, and how far an outgoing bubble slides
    /// aside (`Message::draw`, `history_view_message.cpp:2313`).
    /// An album row passes all its messages: its check and tint show when
    /// every one of them is selected (`isSelectedAsGroup`).
    pub(super) fn selection_row(
        &self,
        chat_id: ChatId,
        ids: &[MessageId],
        // The wide layout's check position from the history's left edge
        // (`chat_wide::selection_right`); `None` puts it at the right edge.
        wide_right: Option<f32>,
        cx: &mut Context<Self>,
    ) -> SelectionRow {
        let Some(&message_id) = ids.first() else {
            return SelectionRow::default();
        };
        let (mode, _) = self.selection_motion(chat_id, cx);
        // The row's state, or the drag's while it covers the row.
        let selected = ids.iter().all(|id| self.selection_shown(chat_id, *id));
        if mode <= 0. && !selected {
            return SelectionRow::default();
        }
        let checked = self.check_progress(chat_id, CheckOf::Row(message_id.0), selected);
        let interactive = self.selecting_in(chat_id);
        let focused = interactive
            && self
                .chat_ui
                .focus
                .row()
                .is_some_and(|row| ids.contains(&row));
        let ring_color = cx.theme().ring;
        let forward = ids
            .iter()
            .find_map(|id| self.selection_forward_drag(chat_id, *id));
        let ground = self.history_ground(chat_id, cx);
        let overlay = (mode > 0.).then(|| {
            // The check is centred in the gutter the bubble slides out
            // of, and slides in from the right edge while it fades.
            let pad = (motion::SELECTION_OFFSET - select_check::SIZE) / 2.;
            let edge = pad - 15. * (1. - mode);
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
                                this.selection_press_row(chat_id, message_id, event, window, cx);
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
                                - select_check::SIZE)),
                            None => this.right(px(edge)),
                        })
                        // Level with a one-line bubble, and with the last
                        // line of a taller one.
                        .bottom_0()
                        .h_full()
                        .max_h(px(motion::CHECK_LANE))
                        .flex()
                        .items_center()
                        .opacity(mode)
                        .child(
                            div()
                                .debug_selector(move || format!("select-check-{}", message_id.0))
                                .child(select_check::select_check(checked, ground, cx)),
                        ),
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
        let topic = self.open_topic_id(chat_id);
        let Some(draft) = self.share.forward.selection_mut() else {
            return;
        };
        let room = quill::selection_pin::room_for(draft.count());
        let fresh: Vec<_> = ids
            .into_iter()
            .filter(|id| !draft.contains(*id))
            .take(room)
            .collect();
        for id in fresh {
            draft.toggle(chat_id, topic, id, false);
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
        // Albums on the way join whole (`changeSelectionAsGroup`).
        let span = self.with_whole_albums(chat_id, span);
        self.add_to_selection(chat_id, span);
        self.set_selection_anchor(message_id);
        self.chat_ui.focus = A11yFocus::Row(message_id);
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

    /// Loaded, sent messages the chat's view draws (the open thread's,
    /// topic's or Saved view's own rows), in history order.
    pub(super) fn loaded_selectable_ids(&self, chat_id: ChatId) -> Vec<MessageId> {
        self.shown_history(chat_id)
            .map(|messages| {
                messages
                    .values()
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
        self.set_note(StatusNote::info(tr(keys::TEXT_COPIED)));
        cx.notify();
    }

    /// The media messages of the selection, oldest first.
    fn selected_media(&self) -> Vec<(MessageId, quill::message_menu::MediaTarget)> {
        let Some(draft) = self.forward_draft() else {
            return Vec::new();
        };
        let Some(messages) = self.shown_history(draft.from_chat_id) else {
            return Vec::new();
        };
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        ids.into_iter()
            .filter_map(|id| {
                let message = messages.get(&id.0)?;
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
        let sent = self
            .live
            .as_mut()
            .is_none_or(|live| live.driver.ensure_media_files(&missing).is_ok());
        self.set_note(match missing.len() {
            _ if !sent => StatusNote::error(tr(keys::Q_OPS_REQUEST_NOT_SENT)),
            1 => StatusNote::silent(tr(keys::Q_CHAT_DOWNLOADING_1_FILE)),
            n => StatusNote::silent(trn(keys::Q_CHAT_DOWNLOADING_FILES, n)),
        });
        cx.notify();
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
        if draft.from_chat_id != chat_id
            || session.is_saved_messages(chat_id)
            || session.is_secret_chat(chat_id)
        {
            return false;
        }
        let Some(messages) = self.shown_history(chat_id) else {
            return false;
        };
        !draft.message_ids.is_empty()
            && draft.message_ids.iter().all(|id| {
                messages
                    .get(&id.0)
                    .is_some_and(|m| !m.is_outgoing && !m.pending() && m.id.0 > 0)
            })
    }

    /// "Report N": the Report flow for every selected message.
    pub(super) fn report_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.forward_draft() else {
            return;
        };
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        self.end_selection(window, cx);
        self.open_message_report(draft.from_chat_id, ids, cx);
    }

    /// The selected messages as text, as Telegram Desktop copies them: a
    /// single message as its text; several as "[date time] Name: text"
    /// lines, oldest first.
    pub(super) fn selected_messages_text(&self) -> Option<(ChatId, String)> {
        let draft = self.forward_draft()?;
        let session = self.session()?;
        let shown = self.shown_history(draft.from_chat_id)?;
        let mut ids = draft.message_ids.clone();
        ids.sort_by_key(|id| id.0);
        let messages: Vec<_> = ids.iter().filter_map(|id| shown.get(&id.0)).collect();
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
        // Every message must be yours and still open to "delete for
        // everyone" (TDLib refuses the whole batch otherwise).
        let all_outgoing = self.shown_history(chat_id).is_some_and(|shown| {
            ids.iter().all(|id| {
                shown.get(&id.0).is_some_and(|message| {
                    message.is_outgoing
                        && session
                            .messages
                            .menu_actions
                            .allows_revoke(chat_id, *id, true)
                })
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
        let account = session.account.clone();
        // A secret chat always deletes for both, so there is nothing to choose.
        let revoke_label =
            (offer_revoke && !session.is_secret_chat(chat_id)).then(|| {
                match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
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
                }
            });
        let question = if ids.len() == 1 {
            tr(keys::SELECTED_DELETE_SURE_THIS).to_string()
        } else {
            trn(keys::Q_CHAT_DO_YOU_WANT_TO_DELETE, ids.len())
        };
        let revoke = std::rc::Rc::new(std::cell::Cell::new(true));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, ids, revoke) = (app.clone(), ids.clone(), revoke.clone());
            let account = account.clone();
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
                        // The box was opened for one account: after a
                        // switch it must not delete in the new one.
                        if this.session().is_some_and(|s| s.account == account) {
                            this.delete_selection(chat_id, &ids, revoke, cx);
                        }
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
        // A refused send keeps the selection, so the delete can be retried.
        if self.delete_messages(chat_id, ids, revoke) {
            self.share.forward = super::forward_stage::ForwardStage::None;
        }
        cx.notify();
    }

    /// Delete `ids` of `chat_id` and nothing else: the selection and the
    /// forward draft stay as they are. `true` when the request went out (or
    /// the demo applied it), or when every message was gone already; `false`
    /// when the send was refused, with the note saying so.
    pub(super) fn delete_messages(
        &mut self,
        chat_id: ChatId,
        ids: &[MessageId],
        revoke: bool,
    ) -> bool {
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
        let mut went_out = true;
        if !ids.is_empty() {
            if let Some(live) = self.live.as_mut() {
                match live.driver.delete_selected(chat_id, ids, revoke) {
                    Err(_) => {
                        went_out = false;
                        self.set_note(StatusNote::error(tr(
                            keys::Q_CHAT_COULD_NOT_DELETE_MESSAGES,
                        )));
                    }
                    Ok(request) => {
                        if self.begin_vanish(chat_id, ids, Deletion::Sent(request))
                            && let Some(live) = self.live.as_mut()
                        {
                            live.driver.session.acks.expect(request);
                        }
                    }
                }
            } else if self.demo_session.is_some() {
                self.begin_vanish(chat_id, ids, Deletion::Local);
                for id in ids {
                    self.apply_demo_delete(chat_id, *id);
                }
            }
        }
        went_out
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "selection_mode_delete_tests.rs"]
mod delete_tests;

mod delete_dialog;
mod save_destination;
mod save_selection;
use save_destination::unique_destination;
pub(super) use save_selection::SaveSelection;
