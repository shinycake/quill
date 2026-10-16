//! The live story in the story viewer: joining it when it shows and
//! leaving when the viewer moves on (tdesktop `Controller::show` /
//! `unfocusReply` → `GroupCall::leave`), closing the viewer when the
//! stream ends, and the composer's and Stars button's actions.
//!
//! Quill has one call slot. tdesktop plays a live story as a call of its
//! own beside the one you are in (`Calls::Instance` keeps video streams
//! apart from the current call); here a joined call or a 1:1 call keeps
//! the slot, and the viewer asks first to leave it (tdesktop's
//! `confirmLeaveCurrent` box and words).
#![deny(clippy::indexing_slicing)]

use super::live_state::{LiveDelete, LiveStarsBox, SendAction, StarsBoxMode, send_action};
use super::*;
use crate::ui::media_panel::PanelTab;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use quill::calls::engine::StreamState;
use quill::calls::group_call_create::{BeforeCreate, before_create};
use quill::lang::trn_with;
use quill::live_story::{DEFAULT_STARS, QUICK_STARS, comment_text};
use quill::paid_reaction::{SLIDER_STEPS, amount_for_step, step_for_amount};
use quill::premium_hub::group_digits;
use quill::state::{Session, unix_ms_now};

/// How long the join waits for the other call to be left.
const RETRY_JOIN_LIMIT_MS: u64 = 10_000;

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
            self.join_watched_live_story(cx);
        } else if self.stories.live.retry_join.is_some() && self.stories.live.watching.is_some() {
            self.join_watched_live_story(cx);
        }
        self.check_live_story_ended(cx);
    }

    /// Joins the watched live story. When another call holds the slot the
    /// viewer asks to leave it; until then the Join button stays.
    fn join_watched_live_story(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, story_id)) = self.stories.live.watching else {
            return;
        };
        let retrying = self.stories.live.retry_join;
        if let Some(live) = self.live.as_mut()
            && live.driver.join_live_story(chat_id, story_id).is_ok()
        {
            self.stories.live.retry_join = None;
            return;
        }
        if let Some(since) = retrying {
            // The other call is still being left; past the limit (a 1:1
            // call stuck hanging up) the viewer says so and stops trying.
            if unix_ms_now().saturating_sub(since) > RETRY_JOIN_LIMIT_MS {
                self.stories.live.retry_join = None;
                self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_JOIN_THE_LIVE).into();
                cx.notify();
            }
            return;
        }
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let session = &live.driver.session;
        let to_channel = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.kind.is_channel());
        match before_create(session.current_call_for_create(None), chat_id, to_channel) {
            BeforeCreate::Ask(text) => self.stories.live.leave_other = Some(text),
            BeforeCreate::Proceed | BeforeCreate::ShowCurrent => {
                self.connection.status_note = tr(keys::Q_MEDIA_COULD_NOT_JOIN_THE_LIVE).into();
            }
        }
        cx.notify();
    }

    /// The confirm's Leave / End call: the other call goes and the join is
    /// tried on each tick until the slot is free.
    pub(super) fn leave_other_call_for_live_story(&mut self, cx: &mut Context<Self>) {
        if self.stories.live.leave_other.take().is_none() {
            return;
        }
        self.end_current_call_for_create(cx);
        self.stories.live.retry_join = Some(unix_ms_now());
        self.join_watched_live_story(cx);
        cx.notify();
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
        // The player's images leave every window's atlas with it.
        if let Some(player) = self.stories.live.player.take() {
            player.release_live(cx);
        }
        // The emoji panel the comment field opened goes with it.
        if self.stories.reply_open {
            self.end_story_reply(cx);
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
        // A new comment scrolls the list to it while the list is at its
        // newest; scrolled up, it stays put.
        let newest = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .and_then(|call| call.messages.last())
            .map(|message| message.message_id);
        if newest != self.stories.live.last_comment {
            let first = self.stories.live.last_comment.is_none();
            self.stories.live.last_comment = newest;
            if first || scrolled_to_bottom(&self.stories.live.scroll) {
                self.stories.live.scroll.scroll_to_bottom();
            }
        }
        self.ensure_live_custom_emoji_downloads();
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
        let markup = self.live_comment_markup(cx);
        let text = comment_text(&markup).0;
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
                if text.trim().is_empty() {
                    return;
                }
                let stars = if admin { 0 } else { chosen };
                if self.session().is_some_and(|s| s.live_needs_top_up(stars)) {
                    self.live_top_up(stars, cx);
                    return;
                }
                let sent = match self.live.as_mut() {
                    Some(live) => live.driver.send_live_story_comment(&markup, stars).is_ok(),
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
        if self.session().is_some_and(|s| s.live_needs_top_up(stars)) {
            self.live_top_up(stars, cx);
            return;
        }
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

    /// Not enough Stars (tdesktop `MaybeRequestBalanceIncrease`): "N Stars
    /// Needed" and Quill's Stars top-up, the Stars box with its packs open.
    /// The comment or reaction stays for another try once the balance is
    /// up.
    fn live_top_up(&mut self, stars: i64, cx: &mut Context<Self>) {
        let have = self
            .session()
            .and_then(Session::live_spendable_stars)
            .unwrap_or(0);
        let needed = stars.saturating_sub(have).max(1);
        self.connection.status_note = trn_with(
            keys::CREDITS_SMALL_BALANCE_TITLE,
            needed,
            &[("count", &group_digits(needed))],
        );
        self.open_stars(cx);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.star_packs.open = true;
            let _ = live.driver.maybe_fetch_star_packs();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.payments.hub.stars_open = true;
            session.payments.star_packs.open = true;
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
        let kept = self
            .stories
            .live
            .stars_before_top_up
            .take()
            .filter(|(kept_mode, _)| *kept_mode == mode)
            .map(|(_, stars)| stars);
        let current = match kept {
            Some(stars) => stars,
            None if mode == StarsBoxMode::Comment => self.stories.live.chosen_stars,
            None => 0,
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
        if boxed.mode != StarsBoxMode::Admin
            && self
                .session()
                .is_some_and(|s| s.live_needs_top_up(boxed.stars))
        {
            self.stories.live.stars_before_top_up = Some((boxed.mode, boxed.stars));
            if boxed.mode == StarsBoxMode::Comment {
                self.stories.live.chosen_stars = boxed.stars;
            }
            self.live_top_up(boxed.stars, cx);
            return;
        }
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
            ban: false,
        });
        cx.notify();
    }

    pub(super) fn confirm_delete_live_comment(&mut self, cx: &mut Context<Self>) {
        let Some(delete) = self.stories.live.delete.take() else {
            return;
        };
        let all_from = delete.delete_all.then_some(delete.sender);
        if let Some(live) = self.live.as_mut() {
            let deleted = live
                .driver
                .delete_live_story_comment(delete.message_id, delete.report_spam, all_from)
                .is_ok();
            // tdesktop `deleteConfirmed`: Ban User bans the sender from the
            // channel, or blocks them on a user's live story.
            let banned = !delete.ban || live.driver.ban_live_story_commenter(delete.sender).is_ok();
            if !deleted || !banned {
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

    /// The live overlay's Retry: join the watched story's call again.
    pub(super) fn retry_live_story(&mut self, cx: &mut Context<Self>) {
        if let Some((chat_id, story_id)) = self.stories.live.watching {
            self.join_live_story_from_viewer(chat_id, story_id, cx);
        }
    }

    /// The emoji button opens the shared panel for the comment field
    /// (emoji only: tdesktop's video stream field takes no stickers or
    /// GIFs). Closing it lets the reply row go too.
    pub(super) fn toggle_live_emoji(&mut self, cx: &mut Context<Self>) {
        self.stories.reply_open = true;
        self.toggle_story_reply_panel(PanelTab::Emoji, cx);
        if !self.story_reply_panel_active() {
            self.stories.reply_open = false;
        }
    }
}

/// Whether the comments list shows its newest comment (within a couple of
/// pixels), so a new one may scroll it.
fn scrolled_to_bottom(scroll: &ScrollHandle) -> bool {
    let max = scroll.max_offset().y;
    -scroll.offset().y >= max - px(2.)
}
