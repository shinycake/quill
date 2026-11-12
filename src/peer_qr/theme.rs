//! A peer's share theme (`parity:profile-share-theme`): what TDLib keeps
//! in `userFullInfo.share_theme` and `supergroupFullInfo.share_theme`,
//! what the box shows for it and what `setProfileShareTheme` sends
//! (tdesktop `api_share_themes.cpp`, `info_profile_share_theme.cpp`).

use super::appearance::{ShareThemeBackdrop, SwatchKey};
use super::color::Rgb;
use crate::ids::RequestId;
use crate::premium_hub::upgraded::{Collectible, parse_collectible};
use crate::telegram::envelope::{ParsedFile, StickerContent};
use serde_json::{Value, json};

/// tdesktop `kMaxEmojiKeys`.
const MAX_THEME_EMOJI: usize = 6;

/// One emoji of a theme (`EmojiType`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeEmoji {
    Plain(String),
    /// A custom emoji, by id. Kept so a save sends it back; the pattern
    /// draws only plain emoji.
    Custom(i64),
}

impl ThemeEmoji {
    pub fn plain(&self) -> Option<&str> {
        match self {
            ThemeEmoji::Plain(emoji) => Some(emoji),
            ThemeEmoji::Custom(_) => None,
        }
    }

    fn parse(value: &Value) -> Option<Self> {
        match value.get("@type")?.as_str()? {
            "emojiTypeRegular" => {
                let emoji = value.get("emoji")?.as_str()?;
                (!emoji.is_empty()).then(|| ThemeEmoji::Plain(emoji.to_string()))
            }
            "emojiTypeCustomEmoji" => {
                let id = value.get("custom_emoji_id").and_then(|id| {
                    id.as_i64()
                        .or_else(|| id.as_str().and_then(|text| text.parse().ok()))
                })?;
                (id != 0).then_some(ThemeEmoji::Custom(id))
            }
            _ => None,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            ThemeEmoji::Plain(emoji) => json!({"@type": "emojiTypeRegular", "emoji": emoji}),
            // TDLib takes int64 as a string.
            ThemeEmoji::Custom(id) => {
                json!({"@type": "emojiTypeCustomEmoji", "custom_emoji_id": id.to_string()})
            }
        }
    }
}

impl PartialEq<&str> for ThemeEmoji {
    fn eq(&self, other: &&str) -> bool {
        self.plain() == Some(*other)
    }
}

/// An upgraded gift a theme is made from (tdesktop `QrGiftSwatch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeGift {
    /// `upgradedGift.id`: two looks of the same gift are the same theme.
    pub id: i64,
    /// `receivedGift.received_gift_id`, what a save sends. Empty for a
    /// theme read from full info, which names only the gift.
    pub received_id: String,
    /// "Plush Pepe #1,234".
    pub name: String,
    pub center: Rgb,
    pub edge: Rgb,
    /// The color the symbol pattern is drawn in.
    pub symbol_color: Rgb,
    pub model: Option<StickerContent>,
    pub symbol: Option<StickerContent>,
    /// The stickers' files, for the downloader.
    pub files: Vec<ParsedFile>,
}

impl ThemeGift {
    pub fn from_collectible(gift: &Collectible, received_id: String) -> Self {
        Self {
            id: gift.id,
            received_id,
            name: gift.full_name(),
            center: Rgb::hex(gift.backdrop.center),
            edge: Rgb::hex(gift.backdrop.edge),
            symbol_color: Rgb::hex(gift.backdrop.symbol),
            model: gift.model.sticker.clone(),
            symbol: gift.symbol.sticker.clone(),
            files: gift.files().cloned().collect(),
        }
    }
}

/// A peer's saved theme (tdesktop `Data::ShareTheme`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ShareTheme {
    #[default]
    Default,
    Emoji {
        /// `None`: the default background.
        backdrop: Option<i32>,
        emoji: Vec<ThemeEmoji>,
    },
    Gift(Box<ThemeGift>),
}

/// Up to six distinct emoji in order (tdesktop `EmojiKeys`).
fn emoji_keys(list: impl IntoIterator<Item = ThemeEmoji>) -> Vec<ThemeEmoji> {
    let mut out: Vec<ThemeEmoji> = Vec::new();
    for emoji in list {
        if out.len() == MAX_THEME_EMOJI {
            break;
        }
        if !out.contains(&emoji) {
            out.push(emoji);
        }
    }
    out
}

/// `share_theme`: null, `profileShareThemeEmoji` or
/// `profileShareThemeUpgradedGift`. An empty emoji theme on the default
/// background and a gift that is not upgraded read as the default
/// (tdesktop `Parse`).
pub fn parse_share_theme(value: Option<&Value>) -> ShareTheme {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return ShareTheme::Default;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("profileShareThemeEmoji") => {
            let emoji = emoji_keys(
                value
                    .get("emojis")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(ThemeEmoji::parse),
            );
            let default = value
                .get("use_default_background")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let backdrop = value
                .get("background_backdrop_id")
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .filter(|_| !default);
            if backdrop.is_none() && emoji.is_empty() {
                ShareTheme::Default
            } else {
                ShareTheme::Emoji { backdrop, emoji }
            }
        }
        Some("profileShareThemeUpgradedGift") => value
            .get("gift")
            .and_then(parse_collectible)
            .map(|gift| {
                ShareTheme::Gift(Box::new(ThemeGift::from_collectible(&gift, String::new())))
            })
            .unwrap_or_default(),
        _ => ShareTheme::Default,
    }
}

/// Whose theme it is: users by user id, groups and channels by
/// supergroup id (the full infos' keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeOwner {
    User(i64),
    Supergroup(i64),
}

/// What the box shows and saves (tdesktop `PeerQrThemeKey` with its
/// swatch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    Emoji {
        color: SwatchKey,
        emoji: Vec<ThemeEmoji>,
    },
    Gift(Box<ThemeGift>),
}

impl Default for ThemeChoice {
    fn default() -> Self {
        ThemeChoice::Emoji {
            color: SwatchKey::Default,
            emoji: Vec::new(),
        }
    }
}

impl ThemeChoice {
    /// The same theme as far as saving goes: the same circle and emoji, or
    /// the same gift.
    pub fn same_key(&self, other: &ThemeChoice) -> bool {
        match (self, other) {
            (ThemeChoice::Gift(a), ThemeChoice::Gift(b)) => a.id == b.id,
            (a, b) => a == b,
        }
    }

    /// The theme as the session keeps it once saved.
    pub fn to_theme(&self) -> ShareTheme {
        match self {
            ThemeChoice::Gift(gift) => ShareTheme::Gift(gift.clone()),
            ThemeChoice::Emoji { color, emoji } => {
                let backdrop = match color {
                    SwatchKey::Default => None,
                    SwatchKey::Backdrop(id) => Some(*id),
                };
                if backdrop.is_none() && emoji.is_empty() {
                    ShareTheme::Default
                } else {
                    ShareTheme::Emoji {
                        backdrop,
                        emoji: emoji.clone(),
                    }
                }
            }
        }
    }

    /// `InputProfileShareTheme`, or null for the default theme (tdesktop
    /// `Serialize`). `None` for a gift with no received id, which cannot be
    /// sent.
    pub fn input_json(&self) -> Option<Value> {
        match self {
            ThemeChoice::Gift(gift) if gift.received_id.is_empty() => None,
            ThemeChoice::Gift(gift) => Some(json!({
                "@type": "inputProfileShareThemeUpgradedGift",
                "received_gift_id": gift.received_id,
            })),
            ThemeChoice::Emoji { color, emoji } => Some(match color {
                SwatchKey::Default if emoji.is_empty() => Value::Null,
                SwatchKey::Default => json!({
                    "@type": "inputProfileShareThemeEmoji",
                    "emojis": emoji.iter().map(ThemeEmoji::to_json).collect::<Vec<_>>(),
                    "use_default_background": true,
                    "background_backdrop_id": 0,
                }),
                SwatchKey::Backdrop(id) => json!({
                    "@type": "inputProfileShareThemeEmoji",
                    "emojis": emoji.iter().map(ThemeEmoji::to_json).collect::<Vec<_>>(),
                    "use_default_background": false,
                    "background_backdrop_id": id,
                }),
            }),
        }
    }
}

fn is_default_backdrop(backdrop: &ShareThemeBackdrop) -> bool {
    backdrop.center == super::appearance::DEFAULT_CENTER
        && backdrop.edge == super::appearance::DEFAULT_EDGE
}

impl ShareTheme {
    /// What the box shows for this theme. A backdrop with the default
    /// colors reads as the default circle, so choosing that circle again
    /// saves nothing (tdesktop `BackdropSwatch`).
    pub fn choice(&self, backdrops: &[ShareThemeBackdrop]) -> ThemeChoice {
        match self {
            ShareTheme::Default => ThemeChoice::default(),
            ShareTheme::Emoji { backdrop, emoji } => {
                let default_id = backdrops
                    .iter()
                    .find(|b| is_default_backdrop(b))
                    .map(|b| b.id);
                let color = match backdrop {
                    Some(id) if Some(*id) != default_id => SwatchKey::Backdrop(*id),
                    _ => SwatchKey::Default,
                };
                ThemeChoice::Emoji {
                    color,
                    emoji: emoji.clone(),
                }
            }
            ShareTheme::Gift(gift) => ThemeChoice::Gift(gift.clone()),
        }
    }

    pub fn files(&self) -> &[ParsedFile] {
        match self {
            ShareTheme::Gift(gift) => &gift.files,
            _ => &[],
        }
    }
}

/// What tdesktop `ShareThemes::CanSave` reads about a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRights {
    /// The current user.
    Me,
    /// Any other user.
    OtherUser,
    /// A supergroup or channel.
    Chat {
        has_username: bool,
        /// Creator, or an administrator with `can_change_info`.
        can_change_info: bool,
    },
    /// A basic group or a secret chat: no share theme.
    Other,
}

/// tdesktop `ShareThemes::CanSave`: your own box, or a group or channel
/// with a username whose info you may change (TDLib wants the
/// `can_change_info` administrator right in both).
pub fn can_save(rights: SaveRights) -> bool {
    match rights {
        SaveRights::Me => true,
        SaveRights::Chat {
            has_username,
            can_change_info,
        } => has_username && can_change_info,
        SaveRights::OtherUser | SaveRights::Other => false,
    }
}

/// A gifts request on its way: which one (its request) and when it was sent
/// (unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GiftsAsk {
    pub request: RequestId,
    pub at: i64,
}

/// Where the gifts panel's list is. The first page and what follows it are
/// different stages, so a list can never be "loaded" and "complete" at the
/// same time as "failed". A request on its way and the offset of the next
/// page live in the stage they belong to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftsStage {
    /// No first page yet and no request out (the panel has not asked).
    #[default]
    First,
    /// The first page's request is out as `ask`.
    FirstAsking(GiftsAsk),
    /// The first page could not be asked for (the driver refused): the
    /// panel says so and offers Retry.
    FirstFailed,
    /// The first page is in; `rest` is about the pages after it.
    Listed(GiftsRest),
}

/// The pages after the first one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftsRest {
    /// The list is whole.
    Complete,
    /// There is more from `offset`, and no request is on its way.
    More { offset: String },
    /// A request is on its way as `ask` (the next page from `offset`, or a
    /// refresh that shows the list meanwhile); `failures` asks in a row
    /// failed before it.
    Loading {
        ask: GiftsAsk,
        offset: String,
        failures: u8,
    },
    /// The next page failed `failures` times in a row: the panel stops asking
    /// for more until `retry_at_secs` (unix seconds), which grows with each
    /// failure up to [`GIFTS_RETRY_MAX_SECS`].
    Waiting {
        retry_at_secs: i64,
        offset: String,
        failures: u8,
    },
}

impl GiftsRest {
    /// Where the next page starts; empty when the list is whole.
    fn offset(&self) -> &str {
        match self {
            Self::Complete => "",
            Self::More { offset } | Self::Loading { offset, .. } | Self::Waiting { offset, .. } => {
                offset
            }
        }
    }

    /// Asks in a row that failed before now.
    fn failures(&self) -> u8 {
        match self {
            Self::Complete | Self::More { .. } => 0,
            Self::Loading { failures, .. } | Self::Waiting { failures, .. } => *failures,
        }
    }

    /// The rest after a request that began at `since` failed: a whole list
    /// stays whole (no offset is left), else the next page waits
    /// [`GIFTS_RETRY_SECS`], doubled for each failure in a row.
    fn failed_at(&self, since: i64) -> Self {
        let failures = self.failures();
        let wait = (GIFTS_RETRY_SECS << failures.min(8)).min(GIFTS_RETRY_MAX_SECS);
        match self.offset() {
            "" => Self::Complete,
            offset => Self::Waiting {
                retry_at_secs: since.saturating_add(wait),
                offset: offset.to_string(),
                failures: failures.saturating_add(1),
            },
        }
    }
}

/// The gifts panel's list for one owner (tdesktop `CreateQrGiftList`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeGifts {
    pub owner: Option<ThemeOwner>,
    pub list: Vec<ThemeGift>,
    pub stage: GiftsStage,
}

/// How long the gifts panel waits after a failed page before it asks for
/// the next one again.
pub const GIFTS_RETRY_SECS: i64 = 5;

/// The longest the wait grows to when pages keep failing.
pub const GIFTS_RETRY_MAX_SECS: i64 = 300;

impl ThemeGifts {
    /// A list of `owner` that is shown as loaded: `list`, with the next
    /// page at `offset` (none when it is whole).
    pub fn listed(owner: ThemeOwner, list: Vec<ThemeGift>, offset: String) -> Self {
        let rest = if offset.is_empty() {
            GiftsRest::Complete
        } else {
            GiftsRest::More { offset }
        };
        Self {
            owner: Some(owner),
            list,
            stage: GiftsStage::Listed(rest),
        }
    }

    /// Where the next page starts; empty when the list is whole or has no
    /// page yet.
    pub fn offset(&self) -> &str {
        match &self.stage {
            GiftsStage::Listed(rest) => rest.offset(),
            GiftsStage::First | GiftsStage::FirstAsking(_) | GiftsStage::FirstFailed => "",
        }
    }

    /// Whether a request for `owner`'s list should go out, and the offset
    /// it starts from: the first page again, or the next one when there is
    /// one, none is on its way and none failed in the last few seconds. A
    /// list for another owner starts empty (and its late pages never match
    /// the new stage).
    pub fn plan(&mut self, owner: ThemeOwner, more: bool, now: i64) -> Option<String> {
        if self.owner != Some(owner) {
            *self = ThemeGifts {
                owner: Some(owner),
                ..ThemeGifts::default()
            };
        }
        if more && !self.wants_more(now) {
            return None;
        }
        Some(if more {
            self.offset().to_string()
        } else {
            String::new()
        })
    }

    /// The request planned for `owner` went out as `request` at `now`: the
    /// stage waits for it. A refresh keeps showing the list it has until
    /// its page lands.
    pub fn asked(&mut self, request: RequestId, now: i64) {
        let ask = GiftsAsk { request, at: now };
        self.stage = match std::mem::take(&mut self.stage) {
            GiftsStage::Listed(rest) => GiftsStage::Listed(GiftsRest::Loading {
                ask,
                offset: rest.offset().to_string(),
                failures: rest.failures(),
            }),
            GiftsStage::First | GiftsStage::FirstAsking(_) | GiftsStage::FirstFailed => {
                GiftsStage::FirstAsking(ask)
            }
        };
    }

    /// The panel may ask for the next page.
    pub fn wants_more(&self, now: i64) -> bool {
        match &self.stage {
            GiftsStage::Listed(GiftsRest::More { .. }) => true,
            GiftsStage::Listed(GiftsRest::Waiting { retry_at_secs, .. }) => now >= *retry_at_secs,
            _ => false,
        }
    }

    /// A page or failure belongs to the request now on its way.
    pub fn is_current(&self, request: RequestId) -> bool {
        match &self.stage {
            GiftsStage::FirstAsking(ask) | GiftsStage::Listed(GiftsRest::Loading { ask, .. }) => {
                ask.request == request
            }
            _ => false,
        }
    }

    /// A page is on its way.
    pub fn asking(&self) -> bool {
        matches!(
            self.stage,
            GiftsStage::FirstAsking(_) | GiftsStage::Listed(GiftsRest::Loading { .. })
        )
    }

    /// The request on its way is no longer pending and nothing answered it
    /// (a reconnect dropped it): it failed.
    pub fn forget_dropped(&mut self, pending: &std::collections::HashSet<RequestId>) -> bool {
        let out = match &self.stage {
            GiftsStage::FirstAsking(ask) | GiftsStage::Listed(GiftsRest::Loading { ask, .. }) => {
                Some(ask.request)
            }
            _ => None,
        };
        let dropped = out.is_some_and(|request| !pending.contains(&request));
        if dropped {
            self.fail();
        }
        dropped
    }

    /// The request failed. The list stays as it was; the empty state shows
    /// only when there is nothing to list. A failed next page is retried
    /// after a wait that grows with each failure in a row.
    pub fn fail(&mut self) {
        self.stage = match std::mem::take(&mut self.stage) {
            GiftsStage::Listed(GiftsRest::Loading {
                ask,
                offset,
                failures,
            }) => GiftsStage::Listed(
                GiftsRest::Loading {
                    ask,
                    offset,
                    failures,
                }
                .failed_at(ask.at),
            ),
            // No request is out: nothing failed.
            GiftsStage::Listed(rest) => GiftsStage::Listed(rest),
            // No first page came: a failure is not an empty list, the panel
            // says so and offers Retry.
            GiftsStage::First | GiftsStage::FirstAsking(_) | GiftsStage::FirstFailed => {
                GiftsStage::FirstFailed
            }
        };
    }

    /// The driver would not send the request at all (not ready, no right
    /// to save, the owner not known). A next page waits
    /// [`GIFTS_RETRY_SECS`] so the panel does not ask again on every
    /// frame; with no first page the panel says it could not load, with
    /// Retry (a refusal is not an empty list: reopening is not the only
    /// way out).
    pub fn refuse(&mut self, now: i64) {
        self.stage = match std::mem::take(&mut self.stage) {
            GiftsStage::Listed(rest) => GiftsStage::Listed(rest.failed_at(now)),
            GiftsStage::First | GiftsStage::FirstAsking(_) | GiftsStage::FirstFailed => {
                GiftsStage::FirstFailed
            }
        };
    }

    /// A page came back. The first page replaces the list; burned gifts
    /// and gifts already listed are skipped (tdesktop `AppendGifts`).
    pub fn accept(&mut self, gifts: Vec<ThemeGift>, next_offset: String, first: bool) {
        if first {
            self.list.clear();
        }
        for gift in gifts {
            if !self.list.iter().any(|known| known.id == gift.id) {
                self.list.push(gift);
            }
        }
        self.stage = GiftsStage::Listed(if next_offset.is_empty() {
            GiftsRest::Complete
        } else {
            GiftsRest::More {
                offset: next_offset,
            }
        });
    }
}

/// How the gifts panel lays its tiles out (tdesktop `ChooseLayout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftsLayoutView {
    Loading,
    /// The first page could not be asked for: a message and Retry.
    Failed,
    Empty,
    /// One, two or three wide tiles in a row.
    Wide(usize),
    Grid,
}

/// Tiles per grid row.
pub const GIFT_COLUMNS: usize = 3;

pub fn gifts_layout(gifts: &ThemeGifts) -> GiftsLayoutView {
    let count = gifts.list.len();
    let complete = match &gifts.stage {
        GiftsStage::First | GiftsStage::FirstAsking(_) => return GiftsLayoutView::Loading,
        GiftsStage::FirstFailed => return GiftsLayoutView::Failed,
        GiftsStage::Listed(rest) => *rest == GiftsRest::Complete,
    };
    if count == 0 {
        GiftsLayoutView::Empty
    } else if !complete || count > GIFT_COLUMNS {
        GiftsLayoutView::Grid
    } else {
        GiftsLayoutView::Wide(count)
    }
}

/// tdesktop `kRefreshEach`: the backdrop list is asked for again an hour
/// after the last request.
pub const BACKDROPS_REFRESH_SECS: i64 = 3600;

/// A request that has not answered this long is given up on, so a dropped
/// answer cannot keep the list from being asked for again.
pub const BACKDROPS_LOAD_SECS: i64 = 60;

/// Where the backdrop list is: never asked, on its way, or answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BackdropsStage {
    /// Never asked, or the last request failed (the next box asks again).
    #[default]
    Idle,
    /// The request `id`, sent at this unix time, is on its way. Only its
    /// own answer or error leaves the stage; if neither comes, it expires
    /// after [`BACKDROPS_LOAD_SECS`].
    Loading { id: RequestId, at: i64 },
    /// The list was asked for at this unix time and the answer is in.
    Ready { at: i64 },
}

/// The server's backdrops and when they were last asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Backdrops {
    pub list: Vec<ShareThemeBackdrop>,
    pub stage: BackdropsStage,
}

impl Backdrops {
    /// tdesktop `backdropsValue`: ask when never asked, or an hour after
    /// the last request, and never twice at once (until a request that
    /// never answered expires).
    pub fn due(&self, now: i64) -> bool {
        match self.stage {
            BackdropsStage::Idle => true,
            BackdropsStage::Loading { at, .. } => now.saturating_sub(at) >= BACKDROPS_LOAD_SECS,
            BackdropsStage::Ready { at } => now.saturating_sub(at) >= BACKDROPS_REFRESH_SECS,
        }
    }

    /// The request `id` goes out; an older one still out is forgotten.
    pub fn start(&mut self, id: RequestId, now: i64) {
        self.stage = BackdropsStage::Loading { id, at: now };
    }

    /// The answer of request `id`, kept with the old list replaced. An
    /// answer of any other request is dropped.
    pub fn accept(&mut self, id: RequestId, list: Vec<ShareThemeBackdrop>) {
        if let BackdropsStage::Loading { id: asked, at } = self.stage
            && asked == id
        {
            self.list = list;
            self.stage = BackdropsStage::Ready { at };
        }
    }

    /// Request `id` failed (or could not be sent): the next box asks again.
    pub fn fail(&mut self, id: RequestId) {
        if matches!(self.stage, BackdropsStage::Loading { id: asked, .. } if asked == id) {
            self.stage = BackdropsStage::Idle;
        }
    }
}
