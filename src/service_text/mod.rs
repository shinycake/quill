//! Service-message wording, following Telegram Desktop's
//! `HistoryItem::setServiceMessageByAction` / `prepare*Text` strings
//! (`lng_action_*` in `lang.strings`). Pure: the caller supplies the
//! names it knows through [`ServiceNames`], and gets back a
//! [`ServiceText`] — plain segments, some carrying a link (a user or chat
//! to open, a message to jump to) — that both the history pill and the
//! chat-list preview use.

use crate::local_time::{civil_local, day_label, hhmm};
use crate::telegram::envelope::{
    BotAccessReason, ExpiredKind, GiftOrigin, MessageContent, MessageSender, Money, PassportKind,
    ServiceAction, format_payment_price,
};
use std::ops::Range;

mod actions;
pub use actions::*;

/// What a clickable segment opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceLink {
    /// The profile of a user or chat.
    Sender(MessageSender),
    /// Scroll to a message in this chat.
    Message(i64),
    /// An external address.
    Url(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seg {
    pub text: String,
    pub link: Option<ServiceLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServiceText {
    pub segs: Vec<Seg>,
}

impl ServiceText {
    /// The text without links (chat-list preview, search, accessibility).
    pub fn plain(&self) -> String {
        self.segs.iter().map(|seg| seg.text.as_str()).collect()
    }

    /// The text and the byte ranges of its links, for a styled paragraph.
    pub fn spans(&self) -> (String, Vec<(Range<usize>, ServiceLink)>) {
        let mut text = String::new();
        let mut links = Vec::new();
        for seg in &self.segs {
            let start = text.len();
            text.push_str(&seg.text);
            if let Some(link) = &seg.link
                && !seg.text.is_empty()
            {
                links.push((start..text.len(), link.clone()));
            }
        }
        (text, links)
    }
}

/// What a pinned message shows in "X pinned …".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinTarget {
    /// Text excerpt of the pinned message.
    Text(String),
    /// A media phrase ("a photo", "a voice message"…).
    Media(String),
    /// Not loaded (yet).
    Loading,
}

/// Lookups the wording needs; implemented by the session.
pub trait ServiceNames {
    /// Full display name of a user.
    fn user_name(&self, user_id: i64) -> Option<String>;
    /// Short name (first name) of a user; defaults to the full name.
    fn user_short_name(&self, user_id: i64) -> Option<String> {
        self.user_name(user_id)
    }
    /// Title of a chat.
    fn chat_name(&self, chat_id: i64) -> Option<String>;
    /// The message a pin points at.
    fn pinned(&self, message_id: i64) -> PinTarget;
    /// Title of the game a game-score message answers.
    fn game_title(&self, message_id: i64) -> Option<String>;
}

pub struct ServiceCtx<'a> {
    /// `message.sender_id`.
    pub actor: Option<MessageSender>,
    pub is_outgoing: bool,
    pub my_id: Option<i64>,
    /// A broadcast channel: events are anonymous posts.
    pub is_channel: bool,
    pub is_secret: bool,
    /// The chat the message lives in (the bot, for shared-chat rows).
    pub chat_id: i64,
    /// Unix time, for "today at…" in scheduled-call rows.
    pub now: i64,
    pub names: &'a dyn ServiceNames,
}

const WHO: &str = "Someone";

#[derive(Default)]
struct B(Vec<Seg>);

impl B {
    fn t(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            return self;
        }
        match self.0.last_mut() {
            Some(Seg {
                link: None,
                text: last,
            }) => last.push_str(&text),
            _ => self.0.push(Seg { text, link: None }),
        }
        self
    }

    fn link(mut self, text: impl Into<String>, link: ServiceLink) -> Self {
        self.0.push(Seg {
            text: text.into(),
            link: Some(link),
        });
        self
    }

    /// Appends another builder's segments.
    fn extend(mut self, other: B) -> Self {
        for seg in other.0 {
            self = match seg.link {
                Some(link) => self.link(seg.text, link),
                None => self.t(seg.text),
            };
        }
        self
    }

    fn done(self) -> ServiceText {
        ServiceText { segs: self.0 }
    }
}

fn plural(count: i64, one: &str, other: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { other })
}

fn quote(text: &str) -> String {
    format!("\"{text}\"")
}

/// `Ui::FormatTTL`.
fn format_ttl(secs: i32) -> String {
    let secs = i64::from(secs);
    if secs < 86_400 {
        plural(secs / 3600, "hour", "hours")
    } else if secs < 86_400 * 7 {
        plural(secs / 86_400, "day", "days")
    } else if secs < 86_400 * 31 {
        let days = secs / 86_400;
        if secs % 7 == 0 {
            plural(days / 7, "week", "weeks")
        } else {
            format!(
                "{} {}",
                plural(days / 7, "week", "weeks"),
                plural(days % 7, "day", "days")
            )
        }
    } else if secs <= 86_400 * 31 * 11 {
        plural(secs / (86_400 * 31), "month", "months")
    } else {
        plural(
            (secs as f64 / (86_400.0 * 365.0)).round() as i64,
            "year",
            "years",
        )
    }
}

/// The duration of a finished video chat (`prepareGroupCall`).
fn call_duration(secs: i32) -> String {
    let secs = i64::from(secs);
    let (days, hours, minutes) = (secs / 86_400, secs / 3600, secs / 60);
    if days > 1 {
        plural(days, "day", "days")
    } else if hours > 1 {
        plural(hours, "hour", "hours")
    } else if minutes > 1 {
        plural(minutes, "minute", "minutes")
    } else {
        plural(secs, "second", "seconds")
    }
}

fn grams(cents: i64) -> String {
    let (whole, frac) = (cents / 100, cents % 100);
    if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac:02}")
            .trim_end_matches('0')
            .to_string()
    }
}

/// "1 Gram" / "2.5 Grams".
fn grams_cost(cents: i64) -> String {
    if cents == 100 {
        "1 Gram".into()
    } else {
        format!("{} Grams", grams(cents))
    }
}

fn group_digits(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

/// `AmountAndStarCurrency`.
fn amount(money: &Money) -> String {
    match money.currency.as_str() {
        "XTR" => format!("\u{2B50}{}", group_digits(money.amount)),
        "TON" => format!("{} TON", grams(money.amount)),
        currency => format_payment_price(currency, money.amount),
    }
}

/// `lng_action_gift_for_stars` and friends: "N Stars" / "N Grams".
fn cost(money: &Money) -> String {
    match money.currency.as_str() {
        "XTR" => plural(money.amount, "Star", "Stars"),
        "TON" => grams_cost(money.amount),
        _ => amount(money),
    }
}

fn stars_cost(stars: i64) -> String {
    plural(stars, "Star", "Stars")
}

fn distance(meters: i32) -> String {
    if meters >= 1000 {
        let km = f64::from(10 * (meters / 10)) / 1000.0;
        let text = format!("{km:.2}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        format!("{text} km")
    } else {
        plural(i64::from(meters), "meter", "meters")
    }
}

impl ServiceCtx<'_> {
    fn is_me_user(&self, user_id: i64) -> bool {
        self.my_id == Some(user_id)
    }

    fn actor_is_me(&self) -> bool {
        self.is_outgoing
            || matches!(self.actor, Some(MessageSender::User { user_id }) if self.is_me_user(user_id))
    }

    fn user(&self, user_id: i64) -> B {
        let name = self.names.user_name(user_id).unwrap_or_else(|| WHO.into());
        B::default().link(name, ServiceLink::Sender(MessageSender::User { user_id }))
    }

    fn user_short(&self, user_id: i64) -> B {
        let name = self
            .names
            .user_short_name(user_id)
            .unwrap_or_else(|| WHO.into());
        B::default().link(name, ServiceLink::Sender(MessageSender::User { user_id }))
    }

    fn chat(&self, chat_id: i64) -> B {
        let name = self
            .names
            .chat_name(chat_id)
            .unwrap_or_else(|| "a chat".into());
        B::default().link(name, ServiceLink::Sender(MessageSender::Chat { chat_id }))
    }

    fn peer(&self, sender: MessageSender) -> B {
        match sender {
            MessageSender::User { user_id } => self.user(user_id),
            MessageSender::Chat { chat_id } => self.chat(chat_id),
        }
    }

    /// `{from}`: the acting member.
    fn from(&self) -> B {
        match self.actor {
            Some(actor) => {
                let mut out = self.peer(actor);
                if self.actor_is_me()
                    && out
                        .0
                        .first()
                        .is_some_and(|seg| seg.text == WHO || seg.text == "a chat")
                {
                    out.0[0].text = "You".into();
                }
                out
            }
            None if self.is_outgoing => B::default().t("You"),
            None => B::default().t(WHO),
        }
    }

    /// A list "A, B and C" of user links.
    fn users(&self, ids: &[i64]) -> B {
        if ids.is_empty() {
            return B::default().t("somebody");
        }
        let mut out = B::default();
        for (index, id) in ids.iter().enumerate() {
            if index > 0 {
                out = out.t(if index + 1 == ids.len() {
                    " and "
                } else {
                    ", "
                });
            }
            out = out.extend(self.user(*id));
        }
        out
    }

    /// The other side of a private chat when the actor is me (the row's
    /// chat), else the actor.
    fn peer_of_gift(&self) -> B {
        if self.actor_is_me() {
            self.chat(self.chat_id)
        } else if let Some(MessageSender::User { user_id }) = self.actor {
            self.user_short(user_id)
        } else {
            self.from()
        }
    }

    /// "{user} sent you a gift for {cost}" / "You sent a gift for {cost}".
    fn gift_line(&self, cost: &str) -> ServiceText {
        if self.actor_is_me() {
            B::default().t(format!("You sent a gift for {cost}")).done()
        } else {
            self.peer_of_gift()
                .t(format!(" sent you a gift for {cost}"))
                .done()
        }
    }

    fn gift_line_between(&self, gifter: i64, cost: &str) -> ServiceText {
        if self.is_me_user(gifter) || self.actor_is_me() {
            B::default().t(format!("You sent a gift for {cost}")).done()
        } else {
            self.user_short(gifter)
                .t(format!(" sent you a gift for {cost}"))
                .done()
        }
    }

    fn channel_prize(&self, text_before: &str, chat_id: i64, text_after: &str) -> ServiceText {
        B::default()
            .t(text_before)
            .extend(self.chat(chat_id))
            .t(text_after)
            .done()
    }

    fn format_schedule(&self, start: i32) -> String {
        let scheduled = civil_local(i64::from(start));
        let now = civil_local(self.now);
        let time = hhmm(&scheduled);
        let ahead = scheduled.day_number() - now.day_number();
        match ahead {
            0 => format!("today at {time}"),
            1 => format!("tomorrow at {time}"),
            _ => format!("{} at {time}", day_label(&scheduled, &now)),
        }
    }
}

fn pin_text(target: PinTarget) -> (String, bool) {
    match target {
        PinTarget::Text(text) => {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut chars = text.chars();
            let head: String = chars.by_ref().take(16).collect();
            // tdesktop cuts at 16 characters and only adds the ellipsis
            // when more than 5 remain.
            let rest = chars.count();
            if rest > 5 {
                (format!("{head}\u{2026}"), false)
            } else {
                (text, false)
            }
        }
        PinTarget::Media(phrase) => (phrase, true),
        PinTarget::Loading => ("a message".into(), true),
    }
}

/// The media phrase a pinned (or replied-to) message gets in service
/// text (`pinnedTextSubstring`), or `None` for text messages.
pub fn pinned_media_phrase(content: &MessageContent) -> Option<String> {
    Some(match content {
        MessageContent::Photo(_) => "a photo".into(),
        MessageContent::Video(_) => "a video".into(),
        MessageContent::Audio(_) => "an audio file".into(),
        MessageContent::VoiceNote(_) => "a voice message".into(),
        MessageContent::VideoNote(_) => "a video message".into(),
        MessageContent::Document(_) => "a file".into(),
        MessageContent::Animation(_) => "a GIF".into(),
        MessageContent::Contact(_) => "a contact information".into(),
        MessageContent::Location(_) | MessageContent::Venue(_) => "a location mark".into(),
        MessageContent::Sticker(sticker) => {
            if sticker.emoji.is_empty() {
                "a sticker".into()
            } else {
                format!("a {} sticker", sticker.emoji)
            }
        }
        MessageContent::Game(game) => format!("the game \u{AB}{}\u{BB}", game.title),
        MessageContent::Action(action) if matches!(**action, ServiceAction::Story { .. }) => {
            "a story".into()
        }
        _ => return None,
    })
}

/// The wording of any service-like content, or `None` for content that
/// is an ordinary message.
pub fn render(content: &MessageContent, ctx: &ServiceCtx<'_>) -> Option<ServiceText> {
    match content {
        MessageContent::Action(action) => Some(render_action(action, ctx)),
        MessageContent::ChatTtlChanged { secs } => Some(render_ttl(*secs, ctx)),
        MessageContent::ScreenshotTaken => Some(if ctx.actor_is_me() {
            B::default().t("You took a screenshot!").done()
        } else {
            ctx.from().t(" took a screenshot!").done()
        }),
        _ => None,
    }
}

fn render_ttl(secs: i32, ctx: &ServiceCtx<'_>) -> ServiceText {
    if secs <= 0 {
        return if ctx.is_channel {
            B::default()
                .t("Messages in this channel will no longer be automatically deleted")
                .done()
        } else if ctx.actor_is_me() {
            B::default().t("You disabled the auto-delete timer").done()
        } else {
            ctx.from().t(" disabled the auto-delete timer").done()
        };
    }
    let duration = if secs == 5 {
        "5 seconds".to_string()
    } else {
        format_ttl(secs)
    };
    if ctx.is_channel {
        B::default()
            .t(format!(
                "Messages in this channel will be automatically deleted after {duration}"
            ))
            .done()
    } else if ctx.actor_is_me() {
        B::default()
            .t(format!("You set messages to auto-delete in {duration}"))
            .done()
    } else {
        ctx.from()
            .t(format!(" set messages to auto-delete in {duration}"))
            .done()
    }
}

struct NoNames;

impl ServiceNames for NoNames {
    fn user_name(&self, _: i64) -> Option<String> {
        None
    }
    fn chat_name(&self, _: i64) -> Option<String> {
        None
    }
    fn pinned(&self, _: i64) -> PinTarget {
        PinTarget::Loading
    }
    fn game_title(&self, _: i64) -> Option<String> {
        None
    }
}

/// Wording without any known names: the fallback of
/// [`MessageContent::preview`] for callers that have no session. Chat-list
/// rows built from a session use [`render`] with real names instead.
/// The Day / Month / Year table of a suggested-birthday card
/// (tdesktop `BirthdayTable`): the year column only shows when it is set.
pub fn birthday_table(day: i32, month: i32, year: i32) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Day", day.to_string()),
        (
            "Month",
            crate::local_time::month_name(month.clamp(1, 12) as u8).to_string(),
        ),
    ];
    if year > 0 {
        rows.push(("Year", year.to_string()));
    }
    rows
}

/// The date a suggested birthday card hands to the birthday form; `None`
/// for a date the form would reject.
pub fn birthday_form_parts(day: i32, month: i32, year: i32) -> Option<(u8, u8, Option<i32>)> {
    let valid = (1..=31).contains(&day) && (1..=12).contains(&month);
    valid.then(|| (day as u8, month as u8, (year > 0).then_some(year)))
}

pub fn action_preview(action: &ServiceAction) -> String {
    let ctx = ServiceCtx {
        actor: None,
        is_outgoing: false,
        my_id: None,
        is_channel: false,
        is_secret: false,
        chat_id: 0,
        now: 0,
        names: &NoNames,
    };
    render_action(action, &ctx).plain()
}
