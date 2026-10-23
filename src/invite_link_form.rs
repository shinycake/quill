//! The New Link / Edit Link box (tdesktop `EditInviteLinkBox`,
//! ui/boxes/edit_invite_link.cpp, and `EditLinkBox` in
//! boxes/peers/edit_peer_invite_link.cpp): the time and member-count
//! presets with their custom values, admin approval, the monthly Stars fee
//! of a channel link, and the request Save sends.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ParsedChatInviteLink;

/// tdesktop `kHour`.
pub const HOUR: i32 = 3600;
/// tdesktop `kDay`.
pub const DAY: i32 = 86_400;
/// tdesktop `kMaxLabelLength`; also TDLib's "0-32 characters".
pub const NAME_MAX: usize = 32;
/// TDLib `createChatInviteLink` `member_limit`: "0-99999".
pub const MEMBER_LIMIT_MAX: i32 = 99_999;
/// The `subscription_star_count_max` option before it arrives (tdesktop's
/// `stars_subscription_amount_max` default).
pub const STARS_MAX_DEFAULT: i64 = 2_500;
/// tdesktop `starsWithdrawRate` default in US cents per Star (`stars_usd_withdraw_rate_x1000`
/// 1300).
pub const STARS_USD_RATE_DEFAULT: f64 = 1.3;
/// tdesktop `ChooseDateTimeBox` `kMinimalSchedule`.
pub const CUSTOM_DATE_MIN_AHEAD: i64 = 10;
/// tdesktop `ChooseDateTimeBox` default `max`: a year from now.
pub const CUSTOM_DATE_MAX_AHEAD: i64 = 365 * 86_400;

/// When the link stops working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expire {
    Never,
    /// This long after Save (the 1 hour / 1 day / 1 week presets).
    After(i32),
    /// At this Unix time (a custom date, or the date of a link being edited).
    At(i32),
}

/// How many people may join through the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    Any,
    Limit(i32),
}

/// One radio of "Limit by time period".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpireOption {
    Value(Expire),
    /// "Custom": opens the date and time box.
    Custom,
}

/// One radio of "Limit by number of users".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageOption {
    Value(Usage),
    /// "Custom": opens the number box.
    Custom,
}

const EXPIRE_PRESETS: [i32; 3] = [HOUR, DAY, 7 * DAY];
const USAGE_PRESETS: [i32; 3] = [1, 10, 100];

impl Expire {
    /// The Unix time sent for this choice (0: never).
    pub fn date(self, now: i64) -> i32 {
        match self {
            Self::Never => 0,
            Self::After(secs) => clamp_i32(now.saturating_add(i64::from(secs))),
            Self::At(date) => date,
        }
    }

    /// Where a link edited with this expiry opens (tdesktop: a date
    /// becomes its own radio, a missing one is "No limit").
    pub fn from_date(date: i32) -> Self {
        if date > 0 {
            Self::At(date)
        } else {
            Self::Never
        }
    }
}

impl Usage {
    pub fn limit(self) -> i32 {
        match self {
            Self::Any => 0,
            Self::Limit(count) => count,
        }
    }

    pub fn from_limit(limit: i32) -> Self {
        if limit > 0 {
            Self::Limit(limit)
        } else {
            Self::Any
        }
    }
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value.clamp(0, i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

/// tdesktop `regenerate`: No limit, 1 hour, 1 day, 1 week, Custom, with
/// a chosen date put in its place among the presets.
pub fn expire_options(current: Expire, now: i64) -> Vec<ExpireOption> {
    let mut options = vec![ExpireOption::Value(Expire::Never)];
    let date = match current {
        Expire::Never => None,
        Expire::After(secs) if EXPIRE_PRESETS.contains(&secs) => None,
        other => Some(other.date(now)),
    };
    let mut placed = date.is_none();
    for secs in EXPIRE_PRESETS {
        if let Some(date) = date
            && !placed
            && now.saturating_add(i64::from(secs)) >= i64::from(date)
        {
            options.push(ExpireOption::Value(current));
            placed = true;
        }
        options.push(ExpireOption::Value(Expire::After(secs)));
    }
    if !placed {
        options.push(ExpireOption::Value(current));
    }
    options.push(ExpireOption::Custom);
    options
}

/// tdesktop `regenerate`: No limit, 1, 10, 100, Custom, with a chosen
/// count put in order among the presets.
pub fn usage_options(current: Usage) -> Vec<UsageOption> {
    let mut options = vec![UsageOption::Value(Usage::Any)];
    let custom = match current {
        Usage::Limit(count) if !USAGE_PRESETS.contains(&count) => Some(count),
        _ => None,
    };
    let mut placed = custom.is_none();
    for preset in USAGE_PRESETS {
        if let Some(count) = custom
            && !placed
            && preset > count
        {
            options.push(UsageOption::Value(Usage::Limit(count)));
            placed = true;
        }
        options.push(UsageOption::Value(Usage::Limit(preset)));
    }
    if let (Some(count), false) = (custom, placed) {
        options.push(UsageOption::Value(Usage::Limit(count)));
    }
    options.push(UsageOption::Custom);
    options
}

/// What the box knows about the chat when it opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatFacts {
    /// A group, not a channel: the approval wording and no Stars fee.
    pub group: bool,
    /// A public chat (it has a t.me username).
    pub public: bool,
    /// The chat already asks admins to approve everyone who joins.
    pub global_request_approval: bool,
}

/// The box's choices; the name and Stars fields hold their own text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteLinkForm {
    /// The link being edited; `None` for a new link.
    pub link: Option<String>,
    pub facts: ChatFacts,
    pub expire: Expire,
    pub usage: Usage,
    pub request_approval: bool,
    /// "Require Monthly Fee" is on.
    pub subscription: bool,
    /// The fee of the subscription link being edited: it can't change.
    pub locked_stars: Option<i64>,
}

/// What a click on "Require Monthly Fee" did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionToggle {
    Toggled,
    /// An existing subscription link: tdesktop's
    /// `lng_group_invite_subscription_toast`.
    Locked,
}

/// One request the box sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InviteLinkRequest {
    /// `createChatInviteLink`.
    Create {
        name: String,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    },
    /// `editChatInviteLink`.
    Edit {
        link: String,
        name: String,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    },
    /// `createChatSubscriptionInviteLink`.
    CreateSubscription { name: String, star_count: i64 },
    /// `editChatSubscriptionInviteLink`: only the name changes.
    EditSubscription { link: String, name: String },
}

impl InviteLinkForm {
    /// tdesktop `CreateInviteLinkBox`.
    pub fn new_link(facts: ChatFacts) -> Self {
        Self {
            link: None,
            facts,
            expire: Expire::Never,
            usage: Usage::Any,
            request_approval: false,
            subscription: false,
            locked_stars: None,
        }
    }

    /// tdesktop `EditLinkBox` for an existing link.
    pub fn edit_link(facts: ChatFacts, link: &ParsedChatInviteLink) -> Self {
        let locked_stars = link
            .subscription_pricing
            .as_ref()
            .map(|pricing| pricing.star_count)
            .filter(|stars| *stars > 0);
        Self {
            link: Some(link.invite_link.clone()),
            facts,
            expire: Expire::from_date(link.expiration_date),
            usage: Usage::from_limit(link.member_limit),
            request_approval: link.creates_join_request && locked_stars.is_none(),
            subscription: locked_stars.is_some(),
            locked_stars,
        }
    }

    pub fn editing(&self) -> bool {
        self.link.is_some()
    }

    /// The "Require Monthly Fee" switch: channels with a private link only
    /// (tdesktop `!isPublic && fillSubscription`, `fill` is null in groups).
    pub fn offers_subscription(&self) -> bool {
        !self.facts.group && !self.facts.public
    }

    /// tdesktop `requestApprovalLocked`: a public chat that lets anyone in.
    pub fn approval_locked(&self) -> bool {
        self.facts.public && !self.facts.global_request_approval
    }

    /// The time and member-count limits. An existing subscription link
    /// only renames (tdesktop `subscriptionLocked`); a new paid link can't
    /// carry them either, as `createChatSubscriptionInviteLink` takes no
    /// expiry or limit.
    pub fn shows_limits(&self) -> bool {
        !self.subscription
    }

    /// The approval switch: every box but an existing subscription link's.
    pub fn shows_approval(&self) -> bool {
        self.locked_stars.is_none()
    }

    /// tdesktop `usagesSlide`: hidden while admins approve each joiner.
    pub fn shows_usage(&self) -> bool {
        self.shows_limits() && !self.request_approval
    }

    /// tdesktop `requestApproval` click: approval and the fee exclude
    /// each other.
    pub fn toggle_approval(&mut self) {
        if self.approval_locked() || self.locked_stars.is_some() {
            return;
        }
        self.request_approval = !self.request_approval;
        self.subscription = false;
    }

    /// tdesktop `subscription` click.
    pub fn toggle_subscription(&mut self) -> SubscriptionToggle {
        if self.locked_stars.is_some() {
            return SubscriptionToggle::Locked;
        }
        self.subscription = !self.subscription;
        self.request_approval = false;
        SubscriptionToggle::Toggled
    }

    /// tdesktop `addSaveButtons` and `EditLinkBox::done`: the request for
    /// the typed name and Stars amount. A fee of 0 makes a free link, as
    /// in tdesktop.
    pub fn request(&self, name: &str, stars: i64, now: i64) -> InviteLinkRequest {
        let name: String = name.trim().chars().take(NAME_MAX).collect();
        if let Some(link) = &self.link
            && self.locked_stars.is_some()
        {
            return InviteLinkRequest::EditSubscription {
                link: link.clone(),
                name,
            };
        }
        if self.subscription && self.link.is_none() && stars > 0 {
            return InviteLinkRequest::CreateSubscription {
                name,
                star_count: stars,
            };
        }
        let (expiration_date, member_limit, creates_join_request) = if self.subscription {
            (0, 0, false)
        } else {
            let approval = self.request_approval;
            let limit = if approval { 0 } else { self.usage.limit() };
            (self.expire.date(now), limit, approval)
        };
        match &self.link {
            Some(link) => InviteLinkRequest::Edit {
                link: link.clone(),
                name,
                expiration_date,
                member_limit,
                creates_join_request,
            },
            None => InviteLinkRequest::Create {
                name,
                expiration_date,
                member_limit,
                creates_join_request,
            },
        }
    }
}

/// tdesktop's custom date check (`ChooseDateTimeBox` `collect`): at
/// least a few seconds ahead and no more than a year.
pub fn custom_date_ok(date: i64, now: i64) -> bool {
    date >= now.saturating_add(CUSTOM_DATE_MIN_AHEAD)
        && date <= now.saturating_add(CUSTOM_DATE_MAX_AHEAD)
}

/// tdesktop's custom limit box: a positive number of people.
pub fn custom_limit(text: &str) -> Option<i32> {
    text.trim()
        .parse::<i32>()
        .ok()
        .filter(|count| (1..=MEMBER_LIMIT_MAX).contains(count))
}

/// The Stars field (tdesktop: digits, capped at the maximum as typed).
pub fn stars_amount(text: &str, max: i64) -> i64 {
    text.trim().parse::<i64>().unwrap_or(0).clamp(0, max.max(1))
}

/// App config `stars_usd_withdraw_rate_x1000` / 1000: US cents per Star
/// (tdesktop `AppConfig::starsWithdrawRate`).
pub fn stars_usd_rate_from_config(config: &serde_json::Value) -> Option<f64> {
    config
        .get("members")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|member| {
            member.get("key").and_then(serde_json::Value::as_str)
                == Some("stars_usd_withdraw_rate_x1000")
        })
        .and_then(|member| member.get("value")?.get("value")?.as_f64())
        .map(|rate| rate / 1000.0)
        .filter(|rate| rate.is_finite() && *rate > 0.0)
}

/// The `{cost}` of tdesktop's `lng_group_invite_subscription_price`:
/// `FillAmountAndCurrency(amount * rate, "USD")`, the rate in cents.
pub fn stars_usd(stars: i64, rate: f64) -> Option<String> {
    let cents = (stars as f64 * rate).round();
    if stars <= 0 || !cents.is_finite() || cents <= 0.0 {
        return None;
    }
    let cents = cents as i64;
    Some(format!("${}.{:02}", cents / 100, cents % 100))
}

#[cfg(test)]
mod tests;
