//! A peer's share theme (`parity:profile-share-theme`): what TDLib keeps
//! in `userFullInfo.share_theme` and `supergroupFullInfo.share_theme`,
//! what the box shows for it and what `setProfileShareTheme` sends
//! (tdesktop `api_share_themes.cpp`, `info_profile_share_theme.cpp`).

use super::appearance::{ShareThemeBackdrop, SwatchKey};
use super::color::Rgb;
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

/// The gifts panel's list for one owner (tdesktop `CreateQrGiftList`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeGifts {
    pub owner: Option<ThemeOwner>,
    pub list: Vec<ThemeGift>,
    pub offset: String,
    /// The first page came back.
    pub loaded: bool,
    pub complete: bool,
    pub loading: bool,
    /// Tells a page for the current request from an older one. It keeps
    /// counting when the list moves to another owner, so a late page for
    /// the old owner never matches.
    pub serial: u64,
    /// When the request on its way was sent (unix seconds).
    pub requested_at: i64,
    /// The next page failed: the panel stops asking for more until
    /// `retry_at` (unix seconds).
    pub retry_at: Option<i64>,
}

/// How long the gifts panel waits after a failed page before it asks for
/// the next one again.
pub const GIFTS_RETRY_SECS: i64 = 5;

/// What a gifts request asks for: its serial and the offset to start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftsRequest {
    pub serial: u64,
    pub offset: String,
}

impl ThemeGifts {
    /// A request for `owner`'s list: the first page again, or the next
    /// one when there is one, none is on its way and none failed in the
    /// last few seconds. A list for another owner starts empty.
    pub fn start(&mut self, owner: ThemeOwner, more: bool, now: i64) -> Option<GiftsRequest> {
        if self.owner != Some(owner) {
            *self = ThemeGifts {
                owner: Some(owner),
                serial: self.serial,
                ..ThemeGifts::default()
            };
        }
        if more && !self.wants_more(now) {
            return None;
        }
        self.serial += 1;
        self.loading = true;
        self.requested_at = now;
        self.retry_at = None;
        Some(GiftsRequest {
            serial: self.serial,
            offset: if more {
                self.offset.clone()
            } else {
                String::new()
            },
        })
    }

    /// The panel may ask for the next page.
    pub fn wants_more(&self, now: i64) -> bool {
        self.loaded && !self.complete && !self.loading && self.retry_at.is_none_or(|at| now >= at)
    }

    /// A page or failure belongs to the request now on its way.
    pub fn is_current(&self, owner: ThemeOwner, serial: u64) -> bool {
        self.owner == Some(owner) && self.serial == serial
    }

    /// The request failed. The list stays as it was; the empty state shows
    /// only when there is nothing to list. A failed next page is retried
    /// [`GIFTS_RETRY_SECS`] after it was asked for.
    pub fn fail(&mut self) {
        self.loading = false;
        if self.loaded {
            self.retry_at = Some(self.requested_at.saturating_add(GIFTS_RETRY_SECS));
        } else {
            // No first page ever came: show what there is (nothing) as
            // a whole list, as tdesktop does.
            self.loaded = true;
            self.complete = true;
        }
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
        self.complete = next_offset.is_empty();
        self.offset = next_offset;
        self.loaded = true;
        self.loading = false;
        self.retry_at = None;
    }
}

/// How the gifts panel lays its tiles out (tdesktop `ChooseLayout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftsLayout {
    Loading,
    Empty,
    /// One, two or three wide tiles in a row.
    Wide(usize),
    Grid,
}

/// Tiles per grid row.
pub const GIFT_COLUMNS: usize = 3;

pub fn gifts_layout(gifts: &ThemeGifts) -> GiftsLayout {
    let count = gifts.list.len();
    if !gifts.loaded {
        GiftsLayout::Loading
    } else if count == 0 {
        GiftsLayout::Empty
    } else if !gifts.complete || count > GIFT_COLUMNS {
        GiftsLayout::Grid
    } else {
        GiftsLayout::Wide(count)
    }
}

/// tdesktop `kRefreshEach`: the backdrop list is asked for again an hour
/// after the last request.
pub const BACKDROPS_REFRESH_SECS: i64 = 3600;

/// The server's backdrops and when they were last asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Backdrops {
    pub list: Vec<ShareThemeBackdrop>,
    pub loading: bool,
    /// Unix time of the last request; cleared by a failure so the next
    /// box asks again.
    pub requested_at: Option<i64>,
}

impl Backdrops {
    /// tdesktop `backdropsValue`: ask when never asked, or an hour after
    /// the last request, and never twice at once.
    pub fn due(&self, now: i64) -> bool {
        !self.loading
            && self
                .requested_at
                .is_none_or(|at| now.saturating_sub(at) >= BACKDROPS_REFRESH_SECS)
    }

    pub fn start(&mut self, now: i64) {
        self.loading = true;
        self.requested_at = Some(now);
    }

    /// A refresh keeps the old list until the new one is here.
    pub fn accept(&mut self, list: Vec<ShareThemeBackdrop>) {
        self.list = list;
        self.loading = false;
    }

    pub fn fail(&mut self) {
        self.loading = false;
        self.requested_at = None;
    }
}
