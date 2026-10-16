//! The live story in the story viewer: joining it when it shows and
//! leaving when the viewer moves on (tdesktop `Controller::show` /
//! `unfocusReply` → `GroupCall::leave`), closing the viewer when the
//! stream ends, and the composer's and Stars button's actions.
#![deny(clippy::indexing_slicing)]

use super::live_state::{LiveDelete, LiveStarsBox, SendAction, StarsBoxMode, send_action};
use super::*;
use crate::ui::media_panel::PanelTab;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use quill::calls::engine::StreamState;
use quill::live_story::{DEFAULT_STARS, QUICK_STARS, flatten_comment};
use quill::paid_reaction::{SLIDER_STEPS, amount_for_step, step_for_amount};
use quill::state::{Session, unix_ms_now};

/// The live story under the viewer: `(poster, story)` when the current
/// item is one.
pub(super) fn live_item(item: Option<&StoryViewerItem>) -> Option<(ChatId, i32)> {
    item.filter(|item| item.kind == StoryViewerKind::Live && item.live_call.is_some())
        .map(|item| (item.chat_id, item.story_id))
}

impl QuillApp {
    fn live_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    /// Joins the live story the viewer shows and leaves the one it moved
    /// away from. Runs on open, on every step and close, and on the tick.
    pub(super) fn sync_live_story(&mut self, cx: &mut Context<Self>) {
        let target = if self.stories.viewer.is_open() {
            live_item(self.stories.viewer.current())
        } else {
            None
        };
        let watching = self.stories.live.watching;
        if watching.is_some() && watching != target {
            self.leave_live_story_view(cx);
        }
        if target.is_some() && self.stories.live.watching.is_none() {
            self.stories.live.watching = target;
            if let (Some(live), Some((chat_id, story_id))) = (self.live.as_mut(), target) {
                // A 1:1 call or another group call keeps the slot: the Join
                // button stays as the fallback (`qll_media_could_not_join…`).
                if live.driver.join_live_story(chat_id, story_id).is_err() {
                    self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_JOIN_THE_LIVE).into();
                }
            }
        }
        self.check_live_story_ended(cx);
    }

    /// tdesktop closes the viewer when the stream's call finishes or fails.
    fn check_live_story_ended(&mut self, cx: &mut Context<Self>) {
        if self.stories.live.watching.is_none() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let Some(call_id) = session.stories.live.as_ref().map(|l| l.group_call_id) else {
            return;
        };
        let call = session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.id == call_id);
        match call {
            Some(call) if call.stream == StreamState::Ended => {
                self.close_story_viewer(cx);
            }
            Some(_) => self.stories.live.seen_call = true,
            None if self.stories.live.seen_call => self.close_story_viewer(cx),
            None => {}
        }
    }

    /// Leaves the live story (pending Stars are dropped) and frees its
    /// picture.
    pub(super) fn leave_live_story_view(&mut self, cx: &mut Context<Self>) {
        if self.stories.live.watching.take().is_none() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.leave_live_story();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.end_live_story();
        }
        for image in self.stories.live.picture.clear() {
            cx.drop_image(image, None);
        }
        self.stories.live.reset();
    }

    /// Per tick: commit Stars whose Undo time is over and pick up new Star
    /// effects.
    pub(super) fn live_story_tick(&mut self, cx: &mut Context<Self>) {
        self.sync_live_story(cx);
        if self.stories.live.watching.is_none() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.live_story_reaction_tick(unix_ms_now());
        }
        let now = Instant::now();
        let effects = self
            .live_session_mut()
            .and_then(|s| s.stories.live.as_mut())
            .map(|live| std::mem::take(&mut live.effects))
            .unwrap_or_default();
        for effect in effects {
            self.stories.live.fly(effect.sender, effect.stars, now);
        }
        self.stories.live.prune_flying(now);
        // New comments scroll the list to the newest.
        let count = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map_or(0, |call| call.messages.len());
        if count != self.stories.live.seen_comments {
            self.stories.live.seen_comments = count;
            self.stories.live.scroll.scroll_to_bottom();
        }
    }

    /// The comments toggle.
    pub(super) fn toggle_live_comments(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self
            .live_session_mut()
            .and_then(|s| s.stories.live.as_mut())
        {
            live.comments.toggle();
        }
        cx.notify();
    }

    /// The pinned strip shows the comments again.
    pub(super) fn show_live_comments(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self
            .live_session_mut()
            .and_then(|s| s.stories.live.as_mut())
        {
            live.comments.show_from_pinned();
        }
        cx.notify();
    }

    /// Whether this is your own live story (tdesktop `canManage`).
    pub(super) fn live_story_admin(&self) -> bool {
        self.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.is_live_story && call.can_be_managed)
    }

    /// The send button (or Enter): send the comment, or open the Stars
    /// box when it needs more Stars (tdesktop `Reply::send`).
    pub(super) fn send_live_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.stories.live.input.read(cx).value().to_string();
        let Some(session) = self.session() else {
            return;
        };
        let price = session
            .calls
            .active_group_call
            .as_ref()
            .map_or(0, |call| call.paid_message_star_count);
        let admin = self.live_story_admin();
        let levels = session.live_levels();
        let chosen = self.stories.live.chosen_stars;
        match send_action(&levels, &text, chosen, price, admin) {
            SendAction::EditPrice => {
                self.open_live_stars_box(StarsBoxMode::Comment, 0, false, cx);
            }
            SendAction::NeedsStars(required) => {
                self.open_live_stars_box(StarsBoxMode::Comment, required, true, cx);
            }
            SendAction::Send => {
                if flatten_comment(&text).is_empty() {
                    return;
                }
                let stars = if admin { 0 } else { chosen };
                let sent = match self.live.as_mut() {
                    Some(live) => live.driver.send_live_story_comment(&text, stars).is_ok(),
                    None => {
                        self.connection.status_note =
                            tr(keys::Q_MEDIA_DEMO_LIVE_STORIES_JOIN_WITH).into();
                        false
                    }
                };
                if sent {
                    self.stories.live.input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                    // tdesktop goes back to the price of a plain comment.
                    self.stories.live.chosen_stars = 0;
                } else {
                    self.surface_live_note();
                }
            }
        }
        cx.notify();
    }

    /// The driver's last refusal, in the status line.
    fn surface_live_note(&mut self) {
        if let Some(note) = self
            .session()
            .and_then(|s| s.stories.live.as_ref())
            .and_then(|live| live.note.clone())
        {
            self.connection.status_note = note;
        }
    }

    /// A left click on the Stars button: one Star with the Undo toast; on
    /// your own live story it opens the box (tdesktop `Reply::paidReact`).
    pub(super) fn tap_live_stars(&mut self, cx: &mut Context<Self>) {
        if self.live_story_admin() {
            self.open_live_stars_box(StarsBoxMode::Admin, 0, false, cx);
            return;
        }
        self.add_live_stars(QUICK_STARS, cx);
    }

    fn add_live_stars(&mut self, stars: i64, cx: &mut Context<Self>) {
        let now = unix_ms_now();
        if let Some(live) = self.live.as_mut() {
            if live.driver.add_live_story_reaction(stars, now).is_err() {
                self.surface_live_note();
            }
        } else if let Some(live) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.stories.live.as_mut())
        {
            live.pending_reaction = Some(quill::live_story::PendingLiveReaction::add(
                live.pending_reaction,
                stars,
                now,
            ));
        }
        cx.notify();
    }

    /// The toast's Undo.
    pub(super) fn undo_live_stars(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.undo_live_story_reaction();
        } else if let Some(live) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.stories.live.as_mut())
        {
            live.pending_reaction = None;
        }
        cx.notify();
    }

    /// Opens the Stars box. `min` is the least it allows (the Stars a
    /// waiting comment needs); `then_send` sends that comment after.
    pub(super) fn open_live_stars_box(
        &mut self,
        mode: StarsBoxMode,
        min: i64,
        then_send: bool,
        cx: &mut Context<Self>,
    ) {
        let max = self.session().map_or(2, Session::live_max_stars);
        let min = min.clamp(1, max);
        let current = if mode == StarsBoxMode::Comment {
            self.stories.live.chosen_stars
        } else {
            0
        };
        let start = if current > 0 { current } else { DEFAULT_STARS };
        let stars = start.clamp(min, max);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(SLIDER_STEPS as f32)
                .step(1.)
                .default_value(step_for_amount(stars, max) as f32)
        });
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            let max = this.session().map_or(2, Session::live_max_stars);
            if let Some(boxed) = this.stories.live.stars_box.as_mut() {
                boxed.stars = amount_for_step(value.end().round() as i64, max).max(boxed.min);
            }
            cx.notify();
        })
        .detach();
        self.stories.live.stars_box = Some(LiveStarsBox {
            mode,
            stars,
            min,
            slider,
            then_send,
        });
        if let Some(live) = self.live.as_mut() {
            // The balance under the button.
            let _ = live.driver.maybe_fetch_star_transactions();
        }
        cx.notify();
    }

    pub(super) fn close_live_stars_box(&mut self, cx: &mut Context<Self>) {
        self.stories.live.stars_box = None;
        cx.notify();
    }

    /// The box's button: the comment gets these Stars, or they are sent
    /// as a reaction.
    pub(super) fn submit_live_stars_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(boxed) = self.stories.live.stars_box.take() else {
            return;
        };
        match boxed.mode {
            StarsBoxMode::Admin => {}
            StarsBoxMode::Reaction => self.add_live_stars(boxed.stars, cx),
            StarsBoxMode::Comment => {
                self.stories.live.chosen_stars = boxed.stars;
                if boxed.then_send {
                    self.send_live_comment(window, cx);
                }
            }
        }
        cx.notify();
    }

    /// "Remove Stars" from the send button's menu.
    pub(super) fn clear_live_stars(&mut self, cx: &mut Context<Self>) {
        self.stories.live.chosen_stars = 0;
        cx.notify();
    }

    /// "Send message as...": who comments are sent as.
    pub(super) fn pick_live_sender(&mut self, sender: MessageSender, cx: &mut Context<Self>) {
        self.stories.live.send_as_open = false;
        if let Some(live) = self.live.as_mut() {
            if live.driver.set_live_story_sender(sender).is_err() {
                self.surface_live_note();
            }
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.active_group_call.as_mut())
        {
            call.message_sender_id = Some(sender);
        }
        cx.notify();
    }

    /// Copy Text from a comment's menu.
    pub(super) fn copy_live_comment(&mut self, text: String, cx: &mut Context<Self>) {
        self.stories.live.menu = None;
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        cx.notify();
    }

    /// Delete from a comment's menu: the confirm box.
    pub(super) fn ask_delete_live_comment(
        &mut self,
        message_id: i32,
        sender: MessageSender,
        cx: &mut Context<Self>,
    ) {
        self.stories.live.menu = None;
        let name = self
            .session()
            .map(|s| s.sender_label(sender))
            .unwrap_or_default();
        self.stories.live.delete = Some(LiveDelete {
            message_id,
            sender,
            name,
            report_spam: false,
            delete_all: false,
        });
        cx.notify();
    }

    pub(super) fn confirm_delete_live_comment(&mut self, cx: &mut Context<Self>) {
        let Some(delete) = self.stories.live.delete.take() else {
            return;
        };
        let all_from = delete.delete_all.then_some(delete.sender);
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .delete_live_story_comment(delete.message_id, delete.report_spam, all_from)
                .is_err()
            {
                self.surface_live_note();
            }
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.active_group_call.as_mut())
        {
            call.messages.retain(|m| match all_from {
                Some(sender) => m.sender_id != sender,
                None => m.message_id != delete.message_id,
            });
        }
        cx.notify();
    }

    /// The header's sound toggle (tdesktop's volume toggle; Quill has
    /// mute only).
    pub(super) fn toggle_live_story_sound(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let muted = live.driver.group_call_playback_muted();
            if let Err(err) = live.driver.set_group_call_playback_muted(!muted) {
                self.connection.status_note = err.to_string();
            }
        }
        cx.notify();
    }

    /// The emoji button opens the shared panel for the comment field.
    pub(super) fn toggle_live_emoji(&mut self, cx: &mut Context<Self>) {
        self.stories.reply_open = true;
        self.toggle_story_reply_panel(PanelTab::Emoji, cx);
    }
}
