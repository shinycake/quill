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

#[allow(clippy::too_many_lines)]
pub fn render_action(action: &ServiceAction, ctx: &ServiceCtx<'_>) -> ServiceText {
    use ServiceAction as A;
    let text = |s: &str| B::default().t(s).done();
    let from = || ctx.from();
    match action {
        A::ChatCreate { title, supergroup } => {
            if *supergroup && ctx.is_channel {
                text("Channel created")
            } else {
                from()
                    .t(format!(" created the group \u{AB}{title}\u{BB}"))
                    .done()
            }
        }
        A::ChatTitle { title } => {
            if ctx.is_channel {
                text(&format!("Channel name was changed to \u{AB}{title}\u{BB}"))
            } else {
                from()
                    .t(format!(" changed group name to \u{AB}{title}\u{BB}"))
                    .done()
            }
        }
        A::ChatPhoto { .. } => {
            if ctx.is_channel {
                text("Channel photo updated")
            } else {
                from().t(" updated group photo").done()
            }
        }
        A::ChatPhotoDeleted => {
            if ctx.is_channel {
                text("Channel photo removed")
            } else {
                from().t(" removed group photo").done()
            }
        }
        A::OwnerLeft { new_owner } => ctx
            .user(*new_owner)
            .t(" will become the new main admin in 7 days if ")
            .extend(from())
            .t(" does not return.")
            .done(),
        A::OwnerChanged { new_owner } => from()
            .t(" made ")
            .extend(ctx.user(*new_owner))
            .t(" the new main admin of the group.")
            .done(),
        A::ProtectedToggled { enabled } => match (ctx.actor_is_me(), enabled) {
            (true, true) => text("You disabled sharing in this chat"),
            (true, false) => text("You enabled sharing in this chat"),
            (false, true) => from().t(" disabled sharing in this chat").done(),
            (false, false) => from().t(" enabled sharing in this chat").done(),
        },
        A::ProtectedDisableRequested { expired } => {
            if *expired {
                text("Sharing enable request has expired")
            } else {
                let features = "\nForwarding messages\nSaving photos and videos\nCopying messages";
                if ctx.actor_is_me() {
                    text(&format!(
                        "You requested to enable sharing in this chat, which includes:{features}"
                    ))
                } else {
                    from()
                        .t(format!(
                            " would like to enable sharing in this chat, which includes:{features}"
                        ))
                        .done()
                }
            }
        }
        A::AddMembers { users } => match users.as_slice() {
            [only] if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *only) => {
                if ctx.is_channel {
                    text("You joined this channel")
                } else {
                    from().t(" joined the group").done()
                }
            }
            [only] if ctx.is_me_user(*only) && !ctx.actor_is_me() => from()
                .t(if ctx.is_channel {
                    " added you to this channel"
                } else {
                    " added you to this group"
                })
                .done(),
            users => from().t(" added ").extend(ctx.users(users)).done(),
        },
        A::DeleteMember { user } => {
            if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *user) {
                from().t(" left the group").done()
            } else {
                from().t(" removed ").extend(ctx.user(*user)).done()
            }
        }
        A::JoinByLink => from().t(" joined the group via invite link").done(),
        A::JoinByRequest => {
            if ctx.actor_is_me() {
                text(if ctx.is_channel {
                    "Your request to join the channel was approved"
                } else {
                    "Your request to join the group was approved"
                })
            } else {
                from().t(" was accepted to the group").done()
            }
        }
        A::UpgradedTo => text("This group was upgraded to a supergroup"),
        A::UpgradedFrom { title } => text(&format!(
            "The group \u{AB}{title}\u{BB} was upgraded to a supergroup"
        )),
        A::Pin { message_id } => {
            let (excerpt, is_media) = pin_text(ctx.names.pinned(*message_id));
            let link = ServiceLink::Message(*message_id);
            if is_media {
                from().t(" pinned ").link(excerpt, link).done()
            } else {
                from().t(" pinned \"").link(excerpt, link).t("\"").done()
            }
        }
        A::Background { only_for_self } => {
            if ctx.actor_is_me() {
                if *only_for_self {
                    text("You set a new wallpaper for this chat")
                } else {
                    B::default()
                        .t("You set a new wallpaper for ")
                        .extend(ctx.chat(ctx.chat_id))
                        .t(" and you.")
                        .done()
                }
            } else {
                from().t(" set a new wallpaper for this chat").done()
            }
        }
        A::Theme { emoji, gift_name } => match (gift_name, emoji.is_empty()) {
            (Some(name), _) => {
                if ctx.actor_is_me() {
                    text(&format!("You set {name} as a new theme for this chat."))
                } else {
                    from()
                        .t(format!(" set {name} as a new theme for this chat."))
                        .done()
                }
            }
            (None, false) => {
                if ctx.actor_is_me() {
                    text(&format!("You changed the chat theme to {emoji}"))
                } else {
                    from()
                        .t(format!(" changed the chat theme to {emoji}"))
                        .done()
                }
            }
            (None, true) => {
                if ctx.actor_is_me() {
                    text("You disabled the chat theme")
                } else {
                    from().t(" disabled the chat theme").done()
                }
            }
        },
        A::Boost { count } => {
            if ctx.actor_is_me() {
                text("You boosted the group")
            } else if *count > 1 {
                from().t(format!(" boosted the group {count} times")).done()
            } else {
                from().t(" boosted the group").done()
            }
        }
        A::TopicCreated { name } => text(&format!("The topic {} was created", quote(name))),
        A::TopicEdited {
            name, icon_edited, ..
        } => {
            if name.is_empty() && *icon_edited {
                from().t(" changed the topic icon").done()
            } else if name.is_empty() {
                text("Topic edited")
            } else {
                from()
                    .t(format!(" renamed the topic to {}", quote(name)))
                    .done()
            }
        }
        A::TopicClosed { closed } => {
            if *closed {
                from().t(" closed the topic").done()
            } else {
                from().t(" reopened the topic").done()
            }
        }
        A::TopicHidden { hidden } => text(if *hidden {
            "Topic hidden"
        } else {
            "Topic unhidden"
        }),
        A::SuggestProfilePhoto { .. } => {
            if ctx.actor_is_me() {
                B::default()
                    .t("You suggested this photo for ")
                    .extend(ctx.chat(ctx.chat_id))
                    .t("'s Telegram profile.")
                    .done()
            } else {
                from()
                    .t(" suggests this photo for your Telegram profile.")
                    .done()
            }
        }
        A::SuggestBirthdate { day, month, year } => {
            let date = if *year > 0 {
                format!(
                    "{day} {} {year}",
                    crate::local_time::month_name(*month as u8)
                )
            } else {
                format!("{day} {}", crate::local_time::month_name(*month as u8))
            };
            if ctx.actor_is_me() {
                B::default()
                    .t("You suggest ")
                    .extend(ctx.chat(ctx.chat_id))
                    .t(format!(" add a date of birth: {date}"))
                    .done()
            } else {
                from()
                    .t(format!(" suggests you add your date of birth: {date}"))
                    .done()
            }
        }
        A::Custom { text: custom } => text(custom),
        A::GameScore {
            game_message_id,
            score,
        } => {
            let scored = if ctx.actor_is_me() {
                B::default().t(format!("You scored {score}"))
            } else {
                from().t(format!(" scored {score}"))
            };
            match ctx.names.game_title(*game_message_id) {
                Some(title) => scored
                    .t(" in ")
                    .link(title, ServiceLink::Message(*game_message_id))
                    .done(),
                None => scored.done(),
            }
        }
        A::ManagedBotCreated { bot_user_id } => from()
            .t(" created a bot ")
            .extend(ctx.user(*bot_user_id))
            .t(".")
            .done(),
        A::PaymentRefunded { owner, money } => {
            let who = match owner {
                Some(owner) => ctx.peer(*owner),
                None => B::default().t(WHO),
            };
            who.t(format!(" refunded {}", amount(money))).done()
        }
        A::GiftedPremium {
            gifter,
            receiver: _,
            money,
        } => ctx.gift_line_between(*gifter, &amount(money)),
        A::GiftedStars {
            gifter,
            receiver: _,
            money,
        } => ctx.gift_line_between(*gifter, &amount(money)),
        A::GiftedGrams {
            gifter, grams: g, ..
        } => ctx.gift_line_between(*gifter, &grams_cost(*g)),
        A::PremiumGiftCode {
            creator,
            from_giveaway,
            unclaimed,
            money,
        } => match creator {
            Some(MessageSender::Chat { chat_id }) => {
                if *unclaimed {
                    ctx.channel_prize(
                        "You have an unclaimed prize from a giveaway by ",
                        *chat_id,
                        ".",
                    )
                } else if *from_giveaway {
                    ctx.channel_prize("You won a prize in a giveaway organized by ", *chat_id, ".")
                } else {
                    ctx.channel_prize("You've received a gift from ", *chat_id, ".")
                }
            }
            Some(MessageSender::User { user_id }) => {
                ctx.gift_line_between(*user_id, &amount(money))
            }
            None => ctx.gift_line(&amount(money)),
        },
        A::GiveawayCreated { stars } => {
            let target = if ctx.is_channel {
                "followers"
            } else {
                "members"
            };
            if *stars > 0 {
                from()
                    .t(format!(
                        " just started a giveaway of {} to its {target}.",
                        stars_cost(*stars)
                    ))
                    .done()
            } else {
                from()
                    .t(format!(
                        " just started a giveaway of Telegram Premium subscriptions to its {target}."
                    ))
                    .done()
            }
        }
        A::Giveaway {
            winners,
            stars,
            months,
        } => {
            if *stars > 0 {
                text(&format!(
                    "Giveaway: {} for {}",
                    stars_cost(*stars),
                    plural(i64::from(*winners), "winner", "winners")
                ))
            } else {
                text(&format!(
                    "Giveaway: {winners} Telegram Premium {} for {}",
                    if *winners == 1 {
                        "subscription"
                    } else {
                        "subscriptions"
                    },
                    plural(i64::from(*months), "month", "months")
                ))
            }
        }
        A::GiveawayCompleted {
            winners,
            stars,
            unclaimed,
        }
        | A::GiveawayWinners {
            winners,
            unclaimed,
            stars,
        } => giveaway_results(*winners, *stars, *unclaimed),
        A::GiveawayPrizeStars {
            boosted_chat_id,
            unclaimed,
        } => {
            if *unclaimed {
                ctx.channel_prize(
                    "You have an unclaimed prize from a giveaway by ",
                    *boosted_chat_id,
                    ".",
                )
            } else {
                ctx.channel_prize(
                    "You won a prize in a giveaway organized by ",
                    *boosted_chat_id,
                    ".",
                )
            }
        }
        A::Gift {
            sender,
            receiver,
            stars,
            prepaid_upgrade,
            from_auction,
        } => render_gift(
            ctx,
            *sender,
            *receiver,
            *stars,
            *prepaid_upgrade,
            *from_auction,
        ),
        A::UpgradedGift {
            name,
            sender,
            receiver,
            origin,
            price,
        } => render_upgraded_gift(ctx, name, *sender, *receiver, *origin, price.as_ref()),
        A::RefundedUpgradedGift => text(
            "This gift was downgraded because a request to refund the payment related to this gift was made, and the money was returned.",
        ),
        A::GiftOffer { name, price, state } => match state {
            1 => text("This offer was accepted."),
            2 => text("This offer was rejected."),
            _ => {
                if ctx.actor_is_me() {
                    text(&format!("You offered {} for {name}.", cost(price)))
                } else {
                    text(&format!("An offer to buy this gift for {}.", cost(price)))
                }
            }
        },
        A::GiftOfferRejected {
            name,
            price,
            expired,
        } => {
            let price = cost(price);
            match (*expired, ctx.actor_is_me()) {
                (true, true) => text(&format!(
                    "Your offer to buy {name} for {price} has expired."
                )),
                (true, false) => from()
                    .t(format!(
                        "'s offer to buy your {name} for {price} has expired."
                    ))
                    .done(),
                (false, false) => from()
                    .t(format!(" rejected your offer to buy {name} for {price}."))
                    .done(),
                (false, true) => text(&format!(
                    "You rejected the offer to buy your {name} for {price}."
                )),
            }
        }
        A::PaidMessagesRefunded { count: _, stars } => {
            if ctx.actor_is_me() {
                B::default()
                    .t(format!("You refunded {} to ", stars_cost(*stars)))
                    .extend(ctx.chat(ctx.chat_id))
                    .done()
            } else {
                from()
                    .t(format!(" refunded {} to you", stars_cost(*stars)))
                    .done()
            }
        }
        A::PaidMessagePriceChanged { stars } => {
            if *stars == 0 {
                text("Messages are now free in this group.")
            } else {
                text(&format!(
                    "Messages now cost {} each in this group.",
                    stars_cost(*stars)
                ))
            }
        }
        A::DirectMessagePriceChanged { enabled, stars } => {
            if !*enabled {
                text("Channel disabled Direct Messages.")
            } else if *stars == 0 {
                text("Channel enabled Direct Messages.")
            } else {
                text(&format!(
                    "Channel allows Direct Messages for {} each.",
                    stars_cost(*stars)
                ))
            }
        }
        A::ChecklistDone { done, not_done, .. } => {
            let tasks = |n: usize| {
                if n == 1 {
                    "task".to_string()
                } else {
                    format!("{n} tasks")
                }
            };
            let who = |tail: String| {
                if ctx.actor_is_me() {
                    B::default().t(format!("You {tail}"))
                } else {
                    from().t(format!(" {tail}"))
                }
            };
            if *done > 0 && *not_done == 0 {
                who(format!("marked {} as done.", tasks(*done))).done()
            } else if *not_done > 0 && *done == 0 {
                who(format!("marked {} as not done.", tasks(*not_done))).done()
            } else {
                who(format!(
                    "marked {} as done and {} as not done.",
                    tasks(*done),
                    tasks(*not_done)
                ))
                .done()
            }
        }
        A::ChecklistAdded { titles } => {
            let list = join_and(titles);
            if ctx.actor_is_me() {
                text(&format!("You added {list} to the list."))
            } else {
                from().t(format!(" added {list} to the list.")).done()
            }
        }
        A::PollOptionAdded { text: option } => {
            if ctx.actor_is_me() {
                text(&format!("You added {} to the poll.", quote(option)))
            } else {
                from()
                    .t(format!(" added {} to the poll.", quote(option)))
                    .done()
            }
        }
        A::PollOptionDeleted { text: option } => {
            if ctx.actor_is_me() {
                text(&format!("You removed {} from the poll.", quote(option)))
            } else {
                from()
                    .t(format!(" removed {} from the poll.", quote(option)))
                    .done()
            }
        }
        A::SuggestedPostApprovalFailed => text("Transaction failed."),
        A::SuggestedPostApproved => text("Agreement reached!"),
        A::SuggestedPostDeclined => text("The post was rejected."),
        A::SuggestedPostPaid { stars, grams: g } => {
            let received = if *stars > 0 {
                stars_cost(*stars)
            } else {
                grams_cost(*g)
            };
            from()
                .t(format!(" has received {received} for publishing post."))
                .done()
        }
        A::SuggestedPostRefunded { post_deleted } => text(if *post_deleted {
            "Admin deleted the post early so that the price was refunded to the user."
        } else {
            "User refunded the Stars so that post was deleted."
        }),
        A::ContactRegistered => from().t(" joined Telegram").done(),
        A::UsersShared { users } => {
            let mut shared = B::default();
            for (index, (id, name)) in users.iter().enumerate() {
                if index > 0 {
                    shared = shared.t(if index + 1 == users.len() {
                        " and "
                    } else {
                        ", "
                    });
                }
                let display = ctx
                    .names
                    .user_name(*id)
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| name.clone());
                shared = shared.link(
                    display,
                    ServiceLink::Sender(MessageSender::User { user_id: *id }),
                );
            }
            B::default()
                .t("You shared ")
                .extend(shared)
                .t(" with ")
                .extend(ctx.chat(ctx.chat_id))
                .done()
        }
        A::ChatShared { chat_id, title } => {
            let shared = B::default().link(
                ctx.names
                    .chat_name(*chat_id)
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| title.clone()),
                ServiceLink::Sender(MessageSender::Chat { chat_id: *chat_id }),
            );
            B::default()
                .t("You shared ")
                .extend(shared)
                .t(" with ")
                .extend(ctx.chat(ctx.chat_id))
                .done()
        }
        A::BotWriteAccess { reason } => match reason {
            BotAccessReason::ConnectedWebsite { domain } => B::default()
                .t("You allowed this bot to message you when you logged in on ")
                .link(domain.clone(), ServiceLink::Url(format!("http://{domain}")))
                .t(".")
                .done(),
            BotAccessReason::AttachMenu => text(
                "You allowed this bot to message you when you added it to your attachment menu.",
            ),
            BotAccessReason::WebApp { title } => {
                let app = if title.is_empty() {
                    "App"
                } else {
                    title.as_str()
                };
                text(&format!(
                    "You allowed this bot to message you when you opened {app}."
                ))
            }
            BotAccessReason::AcceptedRequest => {
                text("You allowed this bot to message you in its web-app.")
            }
        },
        A::WebAppDataSent { button } => text(&format!(
            "Data from the {} button was transferred to the bot.",
            quote(button)
        )),
        A::WebAppDataReceived { button } => text(&format!(
            "Data from the {} button was received.",
            quote(button)
        )),
        A::PassportSent { kinds } => {
            let mut names: Vec<&str> = Vec::new();
            for kind in kinds {
                let name = match kind {
                    PassportKind::PersonalDetails => "personal details",
                    PassportKind::ProofOfIdentity => "proof of identity",
                    PassportKind::Address => "address",
                    PassportKind::ProofOfAddress => "proof of address",
                    PassportKind::Phone => "phone number",
                    PassportKind::Email => "email address",
                };
                names.push(name);
            }
            ctx.chat(ctx.chat_id)
                .t(format!(
                    " received the following documents: {}",
                    names.join(", ")
                ))
                .done()
        }
        A::PassportReceived => text("Telegram Passport data received"),
        A::Proximity {
            traveler,
            watcher,
            distance: meters,
        } => {
            let near = distance(*meters);
            let me = |s: &MessageSender| matches!(s, MessageSender::User { user_id } if ctx.is_me_user(*user_id));
            if me(traveler) {
                B::default()
                    .t(format!("You are now within {near} from "))
                    .extend(ctx.peer(*watcher))
                    .done()
            } else if me(watcher) {
                ctx.peer(*traveler)
                    .t(format!(" is now within {near} from you"))
                    .done()
            } else {
                ctx.peer(*traveler)
                    .t(format!(" is now within {near} from "))
                    .extend(ctx.peer(*watcher))
                    .done()
            }
        }
        A::VideoChatScheduled { start_date } => {
            let when = ctx.format_schedule(*start_date);
            if ctx.is_channel {
                text(&format!("Live stream scheduled for {when}"))
            } else {
                from()
                    .t(format!(" scheduled a video chat for {when}"))
                    .done()
            }
        }
        A::VideoChatStarted => {
            if ctx.is_channel {
                text("Live stream started")
            } else {
                from().t(" started a video chat").done()
            }
        }
        A::VideoChatEnded { duration } => {
            let length = call_duration(*duration);
            if ctx.is_channel {
                text(&format!("Live stream finished ({length})"))
            } else {
                from().t(format!(" ended the video chat ({length})")).done()
            }
        }
        A::VideoChatInvite { users } => from()
            .t(" invited ")
            .extend(ctx.users(users))
            .t(" to the video chat")
            .done(),
        A::Expired(kind) => text(match kind {
            ExpiredKind::Photo => "Expired photo",
            ExpiredKind::Video => "Expired video",
            ExpiredKind::VideoNote => "Round message expired",
            ExpiredKind::VoiceNote => "Voice message expired",
        }),
        A::StakeDice { stake, prize } => {
            let amount = grams_cost;
            let (verb_me, verb_other) = if *prize > *stake {
                (
                    format!("You won {}", amount(*prize - *stake)),
                    format!(" won {}", amount(*prize - *stake)),
                )
            } else if *prize == 0 && *stake > 0 {
                (
                    format!("You lost {}", amount(*stake)),
                    format!(" lost {}", amount(*stake)),
                )
            } else {
                (
                    "You didn't win anything".to_string(),
                    " didn't win anything".to_string(),
                )
            };
            if ctx.actor_is_me() {
                text(&verb_me)
            } else {
                from().t(verb_other).done()
            }
        }
        A::Story { via_mention } => text(if *via_mention {
            "Mentioned in a story"
        } else {
            "Story"
        }),
        A::PaidMedia { stars } => text(&format!("Paid media \u{B7} {}", stars_cost(*stars))),
        A::Checklist { title } => text(&if title.is_empty() {
            "Checklist".to_string()
        } else {
            format!("Checklist: {title}")
        }),
    }
}

fn join_and(items: &[String]) -> String {
    match items {
        [] => "task".into(),
        [only] => only.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

fn giveaway_results(winners: i32, stars: bool, unclaimed: i32) -> ServiceText {
    let text = if winners == 0 {
        "No winners of the giveaway could be selected.".to_string()
    } else {
        match (stars, unclaimed > 0) {
            (true, true) => "Some winners of the giveaway were randomly selected by Telegram and received their prize.".into(),
            (false, true) => "Some winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes.".into(),
            (true, false) => {
                if winners == 1 {
                    "1 winner of the giveaway was randomly selected by Telegram and received their prize.".into()
                } else {
                    format!("{winners} winners of the giveaway were randomly selected by Telegram and received their prize.")
                }
            }
            (false, false) => {
                if winners == 1 {
                    "1 winner of the giveaway was randomly selected by Telegram and received private messages with giftcodes.".into()
                } else {
                    format!("{winners} winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes.")
                }
            }
        }
    };
    B::default().t(text).done()
}

fn render_gift(
    ctx: &ServiceCtx<'_>,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    stars: i64,
    prepaid_upgrade: bool,
    from_auction: bool,
) -> ServiceText {
    let cost = stars_cost(stars);
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let sent_by_me = me(&sender) || (sender.is_none() && ctx.actor_is_me());
    if stars == 0 {
        return if sent_by_me {
            B::default().t("You sent a unique collectible item").done()
        } else {
            ctx.peer_of_gift()
                .t(" sent you a unique collectible item")
                .done()
        };
    }
    if prepaid_upgrade {
        return if sent_by_me {
            B::default()
                .t(format!("You sent an upgrade worth {cost} for "))
                .extend(match receiver {
                    Some(r) if !me(&receiver) => ctx.peer(r),
                    _ => B::default().t("this gift"),
                })
                .t(if me(&receiver) { "." } else { "'s gift." })
                .done()
        } else {
            ctx.peer_of_gift()
                .t(format!(" sent an upgrade worth {cost} for your gift."))
                .done()
        };
    }
    if sender.is_none() && !ctx.actor_is_me() {
        return B::default()
            .t(format!("Unknown user sent you a gift for {cost}"))
            .done();
    }
    if from_auction {
        return B::default()
            .t(format!(
                "You've successfully bought a gift in the auction for {cost}."
            ))
            .done();
    }
    if sent_by_me && me(&receiver) {
        return B::default()
            .t(format!("You bought a gift for {cost}"))
            .done();
    }
    ctx.gift_line(&cost)
}

fn render_upgraded_gift(
    ctx: &ServiceCtx<'_>,
    name: &str,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    origin: GiftOrigin,
    price: Option<&Money>,
) -> ServiceText {
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let actor_me = ctx.actor_is_me();
    match origin {
        GiftOrigin::Upgrade | GiftOrigin::PrepaidUpgrade => {
            if actor_me && (me(&sender) || sender.is_none()) {
                B::default()
                    .t("You turned this gift into a unique collectible")
                    .done()
            } else if actor_me {
                B::default()
                    .t("You turned the gift from ")
                    .extend(match sender {
                        Some(s) => ctx.peer(s),
                        None => B::default().t(WHO),
                    })
                    .t(" into a unique collectible")
                    .done()
            } else {
                ctx.from()
                    .t(" turned the gift from you into a unique collectible")
                    .done()
            }
        }
        GiftOrigin::Transfer | GiftOrigin::Blockchain => {
            if actor_me {
                match receiver {
                    Some(r) if !me(&receiver) => B::default()
                        .t("You transferred a gift to ")
                        .extend(ctx.peer(r))
                        .done(),
                    _ => B::default()
                        .t("You transferred a unique collectible")
                        .done(),
                }
            } else if ctx.actor.is_some() {
                ctx.from().t(" transferred you a gift").done()
            } else {
                B::default().t("Someone transferred you a gift").done()
            }
        }
        GiftOrigin::Resale => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me || me(&sender) {
                B::default().t(format!("You sold a gift for {cost}")).done()
            } else {
                ctx.from().t(format!(" sold you a gift for {cost}")).done()
            }
        }
        GiftOrigin::Offer => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me {
                B::default()
                    .t(format!("You sold {name} for {cost}."))
                    .done()
            } else {
                ctx.from().t(format!(" sold {name} for {cost}.")).done()
            }
        }
        GiftOrigin::Craft => B::default().t("You crafted a new gift").done(),
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
