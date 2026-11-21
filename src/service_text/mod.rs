//! Service-message wording, following Telegram Desktop's
//! `HistoryItem::setServiceMessageByAction` / `prepare*Text` strings
//! (`lng_action_*` in `lang.strings`). Pure: the caller supplies the
//! names it knows through [`ServiceNames`], and gets back a
//! [`ServiceText`] — plain segments, some carrying a link (a user or chat
//! to open, a message to jump to) — that both the history pill and the
//! chat-list preview use.
#![deny(clippy::indexing_slicing)]

use crate::lang::format::Piece;
use crate::lang::{Key, PluralKey, keys, tr, tr_pieces, tr_with, trn, trn_pieces, trn_with};
use crate::local_time::{civil_local, day_label, hhmm};
use crate::telegram::envelope::{
    BotAccessReason, ExpiredKind, GiftOrigin, MessageContent, MessageSender, Money, PassportKind,
    ServiceAction, format_payment_price,
};
use std::ops::Range;

mod actions;
mod gifts;
mod wallet;
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
    /// The message is not in the cache: a lookup result, nothing is asked
    /// for here.
    NotLoaded,
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

#[derive(Default, Clone)]
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

impl From<&str> for B {
    fn from(text: &str) -> Self {
        B::default().t(text)
    }
}

impl From<String> for B {
    fn from(text: String) -> Self {
        B::default().t(text)
    }
}

impl From<&String> for B {
    fn from(text: &String) -> Self {
        B::default().t(text.as_str())
    }
}

/// Builds a sentence from a template's pieces: each `{tag}` becomes the
/// segments of its part (links kept), wherever the translation puts it. A
/// tag with no part stays as written; a part the translation drops is gone.
fn fill(pieces: Vec<Piece<'_>>, parts: &[(&str, B)]) -> B {
    pieces
        .into_iter()
        .fold(B::default(), |out, piece| match piece {
            Piece::Text(text) => out.t(text),
            Piece::Tag(name) => match parts.iter().find(|(tag, _)| *tag == name) {
                Some((_, part)) => out.extend(part.clone()),
                None => out.t(format!("{{{name}}}")),
            },
        })
}

/// `key` with its `{tag}` placeholders filled by `parts`.
fn phrase(key: Key, parts: &[(&str, B)]) -> B {
    fill(tr_pieces(key), parts)
}

/// [`phrase`] with a plural form chosen by `count` (`{count}` filled in).
fn phrase_plural(key: PluralKey, count: i64, parts: &[(&str, B)]) -> B {
    phrase_plural_shown(key, count, count.to_string(), parts)
}

/// [`phrase_plural`] with `{count}` written as `shown` (a fractional Gram
/// amount, say) while the form still follows `count`.
fn phrase_plural_shown(key: PluralKey, count: i64, shown: impl Into<B>, parts: &[(&str, B)]) -> B {
    let mut all = parts.to_vec();
    all.push(("count", shown.into()));
    fill(trn_pieces(key, count), &all)
}

/// A text-only `key`, its `{tag}` placeholders filled.
fn line(key: Key, tags: &[(&str, &str)]) -> ServiceText {
    B::from(tr_with(key, tags)).done()
}

/// A text-only `key` with no placeholders.
fn say(key: Key) -> ServiceText {
    B::from(tr(key)).done()
}

/// `parts` with the actor's `{from}` in front.
fn with_from<'a>(from: B, parts: &[(&'a str, B)]) -> Vec<(&'a str, B)> {
    let mut all = vec![("from", from)];
    all.extend_from_slice(parts);
    all
}

/// The keys and tag names of a list "A, B and C"
/// (`lng_action_*_and_one` / `_and_last`).
struct ListKeys {
    and_one: Key,
    and_last: Key,
    /// The tag of the list so far.
    accumulated: &'static str,
    /// The tag of the item being added.
    item: &'static str,
}

/// tdesktop's user lists (`lng_action_add_users_and_*`).
const USERS: ListKeys = ListKeys {
    and_one: keys::ACTION_ADD_USERS_AND_ONE,
    and_last: keys::ACTION_ADD_USERS_AND_LAST,
    accumulated: "accumulated",
    item: "user",
};

/// "A, B and C": the items joined with the pack's list words. `None` for
/// an empty list.
fn join_segments(items: Vec<B>, list: &ListKeys) -> Option<B> {
    let mut items = items.into_iter();
    let first = items.next()?;
    let rest: Vec<B> = items.collect();
    let last = rest.len();
    Some(
        rest.into_iter()
            .enumerate()
            .fold(first, |out, (index, item)| {
                let key = if index + 1 == last {
                    list.and_last
                } else {
                    list.and_one
                };
                phrase(key, &[(list.accumulated, out), (list.item, item)])
            }),
    )
}

/// `Ui::FormatTTL`.
fn format_ttl(secs: i32) -> String {
    let secs = i64::from(secs);
    if secs < 86_400 {
        trn(keys::HOURS, secs / 3600)
    } else if secs < 86_400 * 7 {
        trn(keys::DAYS, secs / 86_400)
    } else if secs < 86_400 * 31 {
        let days = secs / 86_400;
        if secs % 7 == 0 {
            trn(keys::WEEKS, days / 7)
        } else {
            format!(
                "{} {}",
                trn(keys::WEEKS, days / 7),
                trn(keys::DAYS, days % 7)
            )
        }
    } else if secs <= 86_400 * 31 * 11 {
        trn(keys::MONTHS, secs / (86_400 * 31))
    } else {
        trn(
            keys::YEARS,
            (secs as f64 / (86_400.0 * 365.0)).round() as i64,
        )
    }
}

/// The duration of a finished video chat (`prepareGroupCall`).
fn call_duration(secs: i32) -> String {
    let secs = i64::from(secs);
    let (days, hours, minutes) = (secs / 86_400, secs / 3600, secs / 60);
    if days > 1 {
        trn(keys::DAYS, days)
    } else if hours > 1 {
        trn(keys::HOURS, hours)
    } else if minutes > 1 {
        trn(keys::MINUTES, minutes)
    } else {
        trn(keys::SECONDS, secs)
    }
}

/// A number as shown, with the count whose plural form it takes.
struct Shown {
    text: String,
    form: i64,
}

impl Shown {
    /// `whole` and, when the shown text has one, a fractional part.
    /// A fractional count takes the form of 2: `few` in Russian (the
    /// genitive singular, "2,5 грамма") and `other` in English.
    fn new(text: String, whole: i64, fractional: bool) -> Self {
        let form = if fractional { 2 } else { whole };
        Shown { text, form }
    }

    /// `{count}` of `key` written as this number, in its form.
    fn count_of(&self, key: PluralKey) -> String {
        trn_with(key, self.form, &[("count", &self.text)])
    }
}

/// A Gram amount in hundredths: "5", "2.5".
fn grams(cents: i64) -> Shown {
    let (whole, frac) = (cents / 100, cents % 100);
    let text = if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac:02}")
            .trim_end_matches('0')
            .to_string()
    };
    Shown::new(text, whole, frac != 0)
}

/// "1 Gram" / "2.5 Grams".
fn grams_cost(cents: i64) -> String {
    grams(cents).count_of(keys::ACTION_GIFT_FOR_TON)
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
        "TON" => format!("{} TON", grams(money.amount).text),
        currency => format_payment_price(currency, money.amount),
    }
}

/// `lng_action_gift_for_stars` and friends: "N Stars" / "N Grams".
fn cost(money: &Money) -> String {
    match money.currency.as_str() {
        "XTR" => stars_cost(money.amount),
        "TON" => grams_cost(money.amount),
        _ => amount(money),
    }
}

fn stars_cost(stars: i64) -> String {
    trn(keys::ACTION_GIFT_FOR_STARS, stars)
}

fn distance(meters: i32) -> String {
    if meters >= 1000 {
        let tens = meters / 10;
        let km = f64::from(10 * tens) / 1000.0;
        let text = format!("{km:.2}");
        let text = text.trim_end_matches('0').trim_end_matches('.').to_string();
        Shown::new(text, i64::from(meters / 1000), tens % 100 != 0)
            .count_of(keys::ACTION_PROXIMITY_DISTANCE_KM)
    } else {
        trn(keys::ACTION_PROXIMITY_DISTANCE_M, i64::from(meters))
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
        let name = self.names.user_name(user_id).unwrap_or_else(someone_string);
        let name = crate::text::isolate(&name);
        B::default().link(name, ServiceLink::Sender(MessageSender::User { user_id }))
    }

    fn user_short(&self, user_id: i64) -> B {
        let name = self
            .names
            .user_short_name(user_id)
            .unwrap_or_else(someone_string);
        let name = crate::text::isolate(&name);
        B::default().link(name, ServiceLink::Sender(MessageSender::User { user_id }))
    }

    fn chat(&self, chat_id: i64) -> B {
        let name = self
            .names
            .chat_name(chat_id)
            .unwrap_or_else(|| tr(keys::Q_SERVICE_A_CHAT).to_string());
        let name = crate::text::isolate(&name);
        B::default().link(name, ServiceLink::Sender(MessageSender::Chat { chat_id }))
    }

    fn peer(&self, sender: MessageSender) -> B {
        match sender {
            MessageSender::User { user_id } => self.user(user_id),
            MessageSender::Chat { chat_id } => self.chat(chat_id),
        }
    }

    /// Whether the session knows `sender`'s name (else the wording falls
    /// back to "Someone" / "a chat").
    fn knows(&self, sender: MessageSender) -> bool {
        match sender {
            MessageSender::User { user_id } => self.names.user_name(user_id).is_some(),
            MessageSender::Chat { chat_id } => self.names.chat_name(chat_id).is_some(),
        }
    }

    /// `{from}`: the acting member. My own unnamed actor reads "You".
    fn from(&self) -> B {
        match self.actor {
            Some(actor) if self.actor_is_me() && !self.knows(actor) => {
                B::default().link(tr(keys::FROM_YOU), ServiceLink::Sender(actor))
            }
            Some(actor) => self.peer(actor),
            None if self.is_outgoing => B::from(tr(keys::FROM_YOU)),
            None => B::from(someone()),
        }
    }

    /// `you` when I acted, else `other` with the actor as `{from}`.
    fn by_actor(&self, you: Key, other: Key, parts: &[(&str, B)]) -> ServiceText {
        if self.actor_is_me() {
            phrase(you, parts).done()
        } else {
            phrase(other, &with_from(self.from(), parts)).done()
        }
    }

    /// [`Self::by_actor`] with plural forms chosen by `count`.
    fn by_actor_plural(
        &self,
        you: PluralKey,
        other: PluralKey,
        count: i64,
        parts: &[(&str, B)],
    ) -> ServiceText {
        if self.actor_is_me() {
            phrase_plural(you, count, parts).done()
        } else {
            phrase_plural(other, count, &with_from(self.from(), parts)).done()
        }
    }

    /// A list "A, B and C" of user links (`lng_action_add_users_and_*`).
    fn users(&self, ids: &[i64]) -> B {
        self.join_users(ids, &USERS)
    }

    fn join_users(&self, ids: &[i64], list: &ListKeys) -> B {
        join_segments(ids.iter().map(|id| self.user(*id)).collect(), list)
            .unwrap_or_else(|| B::from(someone()))
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
        let cost = B::from(cost);
        if self.actor_is_me() {
            phrase(keys::ACTION_GIFT_SENT, &[("cost", cost)]).done()
        } else {
            phrase(
                keys::ACTION_GIFT_RECEIVED,
                &[("user", self.peer_of_gift()), ("cost", cost)],
            )
            .done()
        }
    }

    fn gift_line_between(&self, gifter: i64, cost: &str) -> ServiceText {
        let cost = B::from(cost);
        if self.is_me_user(gifter) || self.actor_is_me() {
            phrase(keys::ACTION_GIFT_SENT, &[("cost", cost)]).done()
        } else {
            phrase(
                keys::ACTION_GIFT_RECEIVED,
                &[("user", self.user_short(gifter)), ("cost", cost)],
            )
            .done()
        }
    }

    /// "{channel} also included …": the prize wording naming the giveaway's
    /// chat.
    fn channel_prize(&self, key: Key, chat_id: i64) -> ServiceText {
        phrase(key, &[("channel", self.chat(chat_id))]).done()
    }

    fn format_schedule(&self, start: i32) -> String {
        let scheduled = civil_local(i64::from(start));
        let now = civil_local(self.now);
        let time = hhmm(&scheduled);
        let ahead = scheduled.day_number() - now.day_number();
        match ahead {
            0 => tr_with(keys::GROUP_CALL_STARTS_TODAY, &[("time", &time)]),
            1 => tr_with(keys::GROUP_CALL_STARTS_TOMORROW, &[("time", &time)]),
            _ => tr_with(
                keys::GROUP_CALL_STARTS_DATE,
                &[("date", &day_label(&scheduled, &now)), ("time", &time)],
            ),
        }
    }
}

/// The "Someone" fallback for a name the session does not know.
fn someone() -> &'static str {
    tr(keys::Q_CHAT_SOMEONE)
}

fn someone_string() -> String {
    someone().to_string()
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
            // A Hebrew excerpt in an English line (or the reverse) keeps its
            // ellipsis and the quote marks around it on the right side.
            if rest > 5 {
                (crate::text::isolate(&format!("{head}\u{2026}")), false)
            } else {
                (crate::text::isolate(&text), false)
            }
        }
        PinTarget::Media(phrase) => (phrase, true),
        PinTarget::NotLoaded => (tr(keys::Q_SERVICE_PINNED_MESSAGE).into(), true),
    }
}

/// The media phrase a pinned (or replied-to) message gets in service
/// text (`pinnedTextSubstring`), or `None` for text messages.
pub fn pinned_media_phrase(content: &MessageContent) -> Option<String> {
    Some(match content {
        MessageContent::Photo(_) => tr(keys::ACTION_PINNED_MEDIA_PHOTO).into(),
        MessageContent::Video(_) => tr(keys::ACTION_PINNED_MEDIA_VIDEO).into(),
        MessageContent::Audio(_) => tr(keys::ACTION_PINNED_MEDIA_AUDIO).into(),
        MessageContent::VoiceNote(_) => tr(keys::ACTION_PINNED_MEDIA_VOICE).into(),
        MessageContent::VideoNote(_) => tr(keys::ACTION_PINNED_MEDIA_VIDEO_MESSAGE).into(),
        MessageContent::Document(_) => tr(keys::ACTION_PINNED_MEDIA_FILE).into(),
        MessageContent::Animation(_) => tr(keys::ACTION_PINNED_MEDIA_GIF).into(),
        MessageContent::Contact(_) => tr(keys::ACTION_PINNED_MEDIA_CONTACT).into(),
        MessageContent::Location(_) | MessageContent::Venue(_) => {
            tr(keys::ACTION_PINNED_MEDIA_LOCATION).into()
        }
        MessageContent::Sticker(sticker) => {
            if sticker.emoji.is_empty() {
                tr(keys::ACTION_PINNED_MEDIA_STICKER).into()
            } else {
                tr_with(
                    keys::ACTION_PINNED_MEDIA_EMOJI_STICKER,
                    &[("emoji", &sticker.emoji)],
                )
            }
        }
        MessageContent::Game(game) => {
            tr_with(keys::ACTION_PINNED_MEDIA_GAME, &[("game", &game.title)])
        }
        MessageContent::Action(action) if matches!(**action, ServiceAction::Story { .. }) => {
            tr(keys::ACTION_PINNED_MEDIA_STORY).into()
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
            say(keys::ACTION_YOU_TOOK_SCREENSHOT)
        } else {
            phrase(keys::ACTION_TOOK_SCREENSHOT, &[("from", ctx.from())]).done()
        }),
        _ => None,
    }
}

fn render_ttl(secs: i32, ctx: &ServiceCtx<'_>) -> ServiceText {
    if secs <= 0 {
        return if ctx.is_channel {
            say(keys::ACTION_TTL_REMOVED_CHANNEL)
        } else if ctx.actor_is_me() {
            say(keys::ACTION_TTL_REMOVED_YOU)
        } else {
            phrase(keys::ACTION_TTL_REMOVED, &[("from", ctx.from())]).done()
        };
    }
    let duration = if secs == 5 {
        trn(keys::SECONDS, 5)
    } else {
        format_ttl(secs)
    };
    if ctx.is_channel {
        line(keys::ACTION_TTL_CHANGED_CHANNEL, &[("duration", &duration)])
    } else if ctx.actor_is_me() {
        line(keys::ACTION_TTL_CHANGED_YOU, &[("duration", &duration)])
    } else {
        phrase(
            keys::ACTION_TTL_CHANGED,
            &[("from", ctx.from()), ("duration", B::from(duration))],
        )
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
        PinTarget::NotLoaded
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
        (tr(keys::Q_PEOPLE_DAY), day.to_string()),
        (
            tr(keys::Q_PEOPLE_MONTH),
            crate::local_time::month_name_nominative(month.clamp(1, 12) as u8).to_string(),
        ),
    ];
    if year > 0 {
        rows.push((tr(keys::Q_PEOPLE_YEAR), year.to_string()));
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
