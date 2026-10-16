//! The story viewer's state for the live story it plays: the stream's
//! live player, the comment field, the Stars the
//! composer will add, the boxes it opened and the Star effects in flight.
//! The call, the comments and the donors live in the session
//! (`stories.live`). The menus are the kit's and keep their own state.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::calls::demo_feed::DemoFeed;
use crate::ui::video_player::VideoPlayer;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::live_story::{level_for_stars, pin_finish};
use quill::telegram::envelope::{GroupCallMessageLevel, MessageSender, ParsedGroupCallMessage};
use std::time::{Duration, Instant};

/// tdesktop `StarEffect`: a badge rises from the Stars button for 2 s.
pub(super) const EFFECT_DURATION: Duration = Duration::from_millis(2000);
/// At most four effects at once.
pub(super) const MAX_EFFECTS: usize = 4;
/// tdesktop `kMessageBgOpacity`: an admin's comment background.
pub(super) const ADMIN_BG_OPACITY: f32 = 0.8;
/// tdesktop `kColoredMessageBgOpacity`: a paid comment's background.
pub(super) const PAID_BG_OPACITY: f32 = 0.65;
/// tdesktop `kDarkOverOpacity`: the price pill and the pin progress.
pub(super) const DARK_OVER_OPACITY: f32 = 0.25;
/// tdesktop `kAdminBadgeTextOpacity`.
pub(super) const ADMIN_BADGE_OPACITY: f32 = 0.6;

/// What the Stars box is for (tdesktop `VideoStreamStarsBoxArgs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum StarsBoxMode {
    /// The composer's Stars for the next comment ("Highlight and Pin").
    Comment,
    /// Stars sent as a reaction ("React with Stars").
    Reaction,
    /// Your own live story: the box only explains ("Receive Stars from
    /// Viewers").
    Admin,
}

pub(in crate::ui) struct LiveStarsBox {
    pub(super) mode: StarsBoxMode,
    pub(super) stars: i64,
    pub(super) min: i64,
    pub(super) slider: Entity<SliderState>,
    /// The comment waiting for these Stars (a send that needed more).
    pub(super) then_send: bool,
}

/// The delete box of one comment (tdesktop `Messages::deleteConfirmed`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct LiveDelete {
    pub(super) message_id: i32,
    pub(super) sender: MessageSender,
    pub(super) name: String,
    pub(super) report_spam: bool,
    pub(super) delete_all: bool,
    pub(super) ban: bool,
}

/// One Stars effect on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct FlyingStars {
    pub(super) sender: MessageSender,
    pub(super) stars: i64,
    pub(super) started: Instant,
}

pub(in crate::ui) struct LiveStoryUi {
    /// The comment field.
    pub(in crate::ui) input: Entity<TextareaState>,
    /// The stream's live player, over the driver's feed (also the live
    /// PiP's, `calls::live_pip`).
    pub(in crate::ui) player: Option<VideoPlayer>,
    /// The live story the viewer joined: `(poster, story)`.
    pub(in crate::ui) watching: Option<(ChatId, i32)>,
    /// The call was seen tracked since the join (so its end is noticed).
    pub(super) seen_call: bool,
    /// Stars the next comment carries (0 = free; tdesktop `_starsToSend`).
    pub(super) chosen_stars: i64,
    pub(super) stars_box: Option<LiveStarsBox>,
    pub(super) delete: Option<LiveDelete>,
    /// The Stars box's amount when it sent the viewer to top up, so the
    /// box opens on it again.
    pub(super) stars_before_top_up: Option<(StarsBoxMode, i64)>,
    pub(super) flying: Vec<FlyingStars>,
    /// The comments list; it follows new comments while at the bottom.
    pub(super) scroll: ScrollHandle,
    /// The newest comment seen.
    pub(super) last_comment: Option<i32>,
    /// The screenshot demo's feed (no driver, no stream).
    pub(in crate::ui) demo_feed: Option<DemoFeed>,
    /// The live controller's states over the picture: when each began.
    pub(super) overlay: crate::ui::video_player::controls::LiveOverlayClock,
}

impl LiveStoryUi {
    pub(in crate::ui) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        // Enter sends; comments are one line (tdesktop `NoNewlines`).
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary, shift } => {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if marked.is_none() && !*shift && !*secondary {
                        this.send_live_comment(window, cx);
                    }
                }
                // The placeholder follows the text.
                InputEvent::Change => cx.notify(),
                InputEvent::Focus | InputEvent::Blur => {}
            },
        )
        .detach();
        Self {
            input,
            player: None,
            watching: None,
            seen_call: false,
            chosen_stars: 0,
            stars_box: None,
            delete: None,
            stars_before_top_up: None,
            flying: Vec::new(),
            scroll: ScrollHandle::new(),
            last_comment: None,
            demo_feed: None,
            overlay: Default::default(),
        }
    }

    /// Everything but the field and the picture goes back to the start.
    pub(super) fn reset(&mut self) {
        self.watching = None;
        self.seen_call = false;
        self.chosen_stars = 0;
        self.stars_box = None;
        self.delete = None;
        self.stars_before_top_up = None;
        self.flying.clear();
        self.last_comment = None;
        self.overlay = Default::default();
    }

    /// Queue effects, keeping the newest four.
    pub(super) fn fly(&mut self, sender: MessageSender, stars: i64, now: Instant) {
        self.flying.push(FlyingStars {
            sender,
            stars,
            started: now,
        });
        let over = self.flying.len().saturating_sub(MAX_EFFECTS);
        self.flying.drain(..over);
    }

    /// Drop finished effects; whether any is still running.
    pub(super) fn prune_flying(&mut self, now: Instant) -> bool {
        self.flying
            .retain(|effect| effect_progress(effect.started, now).is_some());
        !self.flying.is_empty()
    }
}

/// How far an effect is, 0 to 1; `None` once it is over.
pub(super) fn effect_progress(started: Instant, now: Instant) -> Option<f32> {
    let elapsed = now.saturating_duration_since(started);
    (elapsed < EFFECT_DURATION).then(|| elapsed.as_secs_f32() / EFFECT_DURATION.as_secs_f32())
}

/// What the composer's send button does (tdesktop `SendButton::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendAction {
    /// Empty field, no Stars: opens the Stars box.
    EditPrice,
    /// Sends the comment with the chosen Stars.
    Send,
    /// The comment is too long or has too many emoji for the chosen
    /// Stars: the box opens at the Stars it needs.
    NeedsStars(i64),
}

/// The send button's action for the field and the chosen Stars. Free
/// comments on your own live story skip the level check.
pub(super) fn send_action(
    levels: &[GroupCallMessageLevel],
    text: &str,
    chosen: i64,
    price: i64,
    admin: bool,
) -> SendAction {
    if text.trim().is_empty() && chosen == 0 {
        return if admin {
            SendAction::Send
        } else {
            SendAction::EditPrice
        };
    }
    if admin {
        return SendAction::Send;
    }
    let required = quill::live_story::stars_required(levels, text).max(price);
    if chosen < required {
        SendAction::NeedsStars(required)
    } else {
        SendAction::Send
    }
}

/// A pinned comment in the strip under the header.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pinned {
    pub(super) message_id: i32,
    pub(super) sender: MessageSender,
    pub(super) stars: i64,
    pub(super) text: String,
    /// The part of the pin time already gone, 0 to 1.
    pub(super) gone: f32,
    pub(super) color: u32,
}

/// The paid comments still pinned at `now` (unix seconds), most Stars
/// first (tdesktop `Messages::pinned`).
pub(super) fn pinned_comments(
    messages: &[ParsedGroupCallMessage],
    levels: &[GroupCallMessageLevel],
    now: i64,
) -> Vec<Pinned> {
    let mut pinned: Vec<Pinned> = messages
        .iter()
        .filter(|m| m.paid_message_star_count > 0 && m.date > 0)
        .filter_map(|m| {
            let finish = pin_finish(levels, m.date, m.paid_message_star_count);
            let start = i64::from(m.date);
            (finish > now).then(|| {
                let span = (finish - start).max(1) as f32;
                Pinned {
                    message_id: m.message_id,
                    sender: m.sender_id,
                    stars: m.paid_message_star_count,
                    text: m.text.clone(),
                    gone: ((now - start) as f32 / span).clamp(0.0, 1.0),
                    color: level_for_stars(levels, m.paid_message_star_count).first_color,
                }
            })
        })
        .collect();
    pinned.sort_by(|a, b| b.stars.cmp(&a.stars).then(b.message_id.cmp(&a.message_id)));
    pinned
}

/// The pin time of a level as tdesktop shortens it: "5m", "1h", "30s".
pub(super) fn pin_label(seconds: i32) -> String {
    use quill::lang::{keys, trn};
    if seconds >= 3600 {
        trn(keys::HOURS_TINY, seconds / 3600)
    } else if seconds >= 60 {
        trn(keys::MINUTES_TINY, seconds / 60)
    } else {
        trn(keys::SECONDS_TINY, seconds)
    }
}

/// An `0xRRGGBB` level color.
pub(super) fn level_color(color: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(color).into();
    color.a = alpha;
    color
}
