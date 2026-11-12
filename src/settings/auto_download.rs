//! Automatic media download, as Telegram Desktop keeps it (Settings ›
//! Advanced › Automatic media download, `data/data_auto_download.cpp`):
//! a byte limit per chat source and media kind, where 0 means "never", plus
//! per-chat exceptions that always or never download.
//!
//! The settings live on this device only, like tdesktop's. TDLib's
//! `autoDownloadSettings` are per network type (mobile, Wi-Fi, roaming) for
//! phones; TDLib downloads nothing by itself either way.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::telegram::envelope::ChatKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MIB: i64 = 1024 * 1024;
/// tdesktop `kDefaultMaxSize`: photos, voice messages, files and music.
pub const DEFAULT_DOWNLOAD_LIMIT: i64 = 8 * MIB;
/// tdesktop `kDefaultAutoPlaySize`: videos, round video messages and GIFs.
pub const DEFAULT_AUTOPLAY_LIMIT: i64 = 50 * MIB;

/// Where a chat's media comes from (tdesktop `AutoDownload::Source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoDownloadSource {
    Private,
    Groups,
    Channels,
}

impl AutoDownloadSource {
    pub const ALL: [Self; 3] = [Self::Private, Self::Groups, Self::Channels];

    /// tdesktop `SourceFromPeer`: users and secret chats are private, basic
    /// groups and supergroups are groups, broadcast channels are channels.
    pub fn of_chat(kind: &ChatKind) -> Self {
        match kind {
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Self::Channels,
            ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. } => Self::Groups,
            _ => Self::Private,
        }
    }
}

/// One media kind with its own limit (tdesktop `AutoDownload::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoDownloadKind {
    Photo,
    File,
    /// Hidden in the box, like tdesktop's streamed types: it follows the
    /// download limit, and the row switch turns it on and off.
    Voice,
    /// Hidden like [`Self::Voice`].
    Music,
    VideoMessage,
    Video,
    Gif,
}

impl AutoDownloadKind {
    /// tdesktop `kAutoPlayTypes`: these share the "Autoplay" slider.
    pub fn is_autoplay(self) -> bool {
        matches!(self, Self::VideoMessage | Self::Video | Self::Gif)
    }

    /// tdesktop `kStreamedTypes`: not shown in the box.
    pub fn is_hidden(self) -> bool {
        matches!(self, Self::Voice | Self::Music)
    }
}

/// The byte limit of each kind for one source; 0 is "never".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLimits {
    pub photo: i64,
    pub file: i64,
    pub voice: i64,
    pub music: i64,
    pub video_message: i64,
    pub video: i64,
    pub gif: i64,
}

impl SourceLimits {
    pub const DISABLED: Self = Self {
        photo: 0,
        file: 0,
        voice: 0,
        music: 0,
        video_message: 0,
        video: 0,
        gif: 0,
    };

    /// tdesktop `SetDefaultsForSource`: channels download no files or music.
    pub fn defaults(source: AutoDownloadSource) -> Self {
        let files = match source {
            AutoDownloadSource::Channels => 0,
            AutoDownloadSource::Private | AutoDownloadSource::Groups => DEFAULT_DOWNLOAD_LIMIT,
        };
        Self {
            photo: DEFAULT_DOWNLOAD_LIMIT,
            file: files,
            voice: DEFAULT_DOWNLOAD_LIMIT,
            music: files,
            video_message: DEFAULT_AUTOPLAY_LIMIT,
            video: DEFAULT_AUTOPLAY_LIMIT,
            gif: DEFAULT_AUTOPLAY_LIMIT,
        }
    }

    pub fn limit(&self, kind: AutoDownloadKind) -> i64 {
        match kind {
            AutoDownloadKind::Photo => self.photo,
            AutoDownloadKind::File => self.file,
            AutoDownloadKind::Voice => self.voice,
            AutoDownloadKind::Music => self.music,
            AutoDownloadKind::VideoMessage => self.video_message,
            AutoDownloadKind::Video => self.video,
            AutoDownloadKind::Gif => self.gif,
        }
    }

    pub fn set_limit(&mut self, kind: AutoDownloadKind, bytes: i64) {
        let slot = match kind {
            AutoDownloadKind::Photo => &mut self.photo,
            AutoDownloadKind::File => &mut self.file,
            AutoDownloadKind::Voice => &mut self.voice,
            AutoDownloadKind::Music => &mut self.music,
            AutoDownloadKind::VideoMessage => &mut self.video_message,
            AutoDownloadKind::Video => &mut self.video,
            AutoDownloadKind::Gif => &mut self.gif,
        };
        *slot = bytes.max(0);
    }

    /// tdesktop `HasEnabledTypes`: the row switch is on while a kind the
    /// box shows downloads.
    pub fn any_enabled(&self) -> bool {
        AutoDownloadKind::SHOWN
            .iter()
            .any(|kind| self.limit(*kind) > 0)
    }

    /// tdesktop `Single::shouldDownload`. A size of 0 is not known yet and
    /// does not block.
    pub fn allows(&self, kind: AutoDownloadKind, size: i64) -> bool {
        let limit = self.limit(kind);
        limit > 0 && size <= limit
    }
}

impl AutoDownloadKind {
    /// The kinds the box has a switch for, in its order.
    pub const SHOWN: [Self; 5] = [
        Self::Photo,
        Self::File,
        Self::VideoMessage,
        Self::Video,
        Self::Gif,
    ];
    pub const ALL: [Self; 7] = [
        Self::Photo,
        Self::File,
        Self::Voice,
        Self::Music,
        Self::VideoMessage,
        Self::Video,
        Self::Gif,
    ];
}

/// A chat's exception (tdesktop `AutoDownload::Override`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoDownloadOverride {
    Always,
    Never,
}

/// Every source's limits and the chat exceptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoDownloadSettings {
    pub private: SourceLimits,
    pub groups: SourceLimits,
    pub channels: SourceLimits,
    /// Keyed by chat id.
    #[serde(default)]
    pub exceptions: BTreeMap<i64, AutoDownloadOverride>,
}

impl Default for AutoDownloadSettings {
    fn default() -> Self {
        Self {
            private: SourceLimits::defaults(AutoDownloadSource::Private),
            groups: SourceLimits::defaults(AutoDownloadSource::Groups),
            channels: SourceLimits::defaults(AutoDownloadSource::Channels),
            exceptions: BTreeMap::new(),
        }
    }
}

impl AutoDownloadSettings {
    pub fn source(&self, source: AutoDownloadSource) -> &SourceLimits {
        match source {
            AutoDownloadSource::Private => &self.private,
            AutoDownloadSource::Groups => &self.groups,
            AutoDownloadSource::Channels => &self.channels,
        }
    }

    pub fn source_mut(&mut self, source: AutoDownloadSource) -> &mut SourceLimits {
        match source {
            AutoDownloadSource::Private => &mut self.private,
            AutoDownloadSource::Groups => &mut self.groups,
            AutoDownloadSource::Channels => &mut self.channels,
        }
    }

    /// The row switch (tdesktop `SetDefaultsForSource` /
    /// `SetDisabledForSource`).
    pub fn set_source_enabled(&mut self, source: AutoDownloadSource, on: bool) {
        *self.source_mut(source) = if on {
            SourceLimits::defaults(source)
        } else {
            SourceLimits::DISABLED
        };
    }

    pub fn exception(&self, chat: ChatId) -> Option<AutoDownloadOverride> {
        self.exceptions.get(&chat.0).copied()
    }

    /// The chats with exception `which` among those `source_of` places in
    /// `source`. A chat this device does not know is kept but not listed.
    pub fn exception_chats(
        &self,
        which: AutoDownloadOverride,
        source: AutoDownloadSource,
        source_of: impl Fn(ChatId) -> Option<AutoDownloadSource>,
    ) -> Vec<ChatId> {
        self.exceptions
            .iter()
            .filter(|(_, value)| **value == which)
            .map(|(id, _)| ChatId(*id))
            .filter(|chat| source_of(*chat) == Some(source))
            .collect()
    }

    /// Whether a `size`-byte file of `kind` in `chat` (from `source`)
    /// downloads by itself (tdesktop `Should` / `ShouldAutoPlay`). A chat
    /// that always downloads uses the source's limit, or its default limit
    /// when the user switched the kind off (`ForceAllowLimit`).
    pub fn allows(
        &self,
        chat: ChatId,
        source: AutoDownloadSource,
        kind: AutoDownloadKind,
        size: i64,
    ) -> bool {
        let limits = self.source(source);
        match self.exception(chat) {
            Some(AutoDownloadOverride::Never) => false,
            Some(AutoDownloadOverride::Always) => {
                let own = limits.limit(kind);
                let limit = if own > 0 {
                    own
                } else {
                    SourceLimits::defaults(source).limit(kind)
                };
                limit > 0 && size <= limit
            }
            None => limits.allows(kind, size),
        }
    }
}

/// The two size sliders of the box (tdesktop `AddSizeLimitSlider`): one
/// for the kinds it downloads, one for the kinds it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoDownloadLimitGroup {
    Download,
    Autoplay,
}

impl AutoDownloadLimitGroup {
    pub fn of(kind: AutoDownloadKind) -> Self {
        if kind.is_autoplay() {
            Self::Autoplay
        } else {
            Self::Download
        }
    }

    /// Where the slider starts while every kind of the group is off
    /// (tdesktop `kDefaultDownloadLimit` / `kDefaultAutoPlayLimit`).
    fn fallback(self) -> i64 {
        match self {
            Self::Download => 10 * MIB,
            Self::Autoplay => DEFAULT_AUTOPLAY_LIMIT,
        }
    }
}

/// One source's box being edited (tdesktop `AutoDownloadBox`): nothing
/// applies until Save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoDownloadDraft {
    pub source: AutoDownloadSource,
    /// Which shown kinds are on: a limit above 0. The value is replaced by
    /// its group's slider on Save.
    limits: SourceLimits,
    download_limit: i64,
    autoplay_limit: i64,
    pub always: std::collections::BTreeSet<i64>,
    pub never: std::collections::BTreeSet<i64>,
}

impl AutoDownloadDraft {
    pub fn new(
        settings: &AutoDownloadSettings,
        source: AutoDownloadSource,
        source_of: impl Fn(ChatId) -> Option<AutoDownloadSource>,
    ) -> Self {
        let limits = *settings.source(source);
        let start = |group: AutoDownloadLimitGroup| {
            let top = AutoDownloadKind::SHOWN
                .iter()
                .filter(|kind| AutoDownloadLimitGroup::of(**kind) == group)
                .map(|kind| limits.limit(*kind))
                .max()
                .unwrap_or(0);
            if top > 0 { top } else { group.fallback() }
        };
        let ids = |which| {
            settings
                .exception_chats(which, source, &source_of)
                .into_iter()
                .map(|chat| chat.0)
                .collect()
        };
        Self {
            source,
            limits,
            download_limit: start(AutoDownloadLimitGroup::Download),
            autoplay_limit: start(AutoDownloadLimitGroup::Autoplay),
            always: ids(AutoDownloadOverride::Always),
            never: ids(AutoDownloadOverride::Never),
        }
    }

    pub fn is_on(&self, kind: AutoDownloadKind) -> bool {
        self.limits.limit(kind) > 0
    }

    pub fn set_on(&mut self, kind: AutoDownloadKind, on: bool) {
        let limit = if on {
            self.group_limit(AutoDownloadLimitGroup::of(kind))
        } else {
            0
        };
        self.limits.set_limit(kind, limit);
    }

    pub fn group_limit(&self, group: AutoDownloadLimitGroup) -> i64 {
        match group {
            AutoDownloadLimitGroup::Download => self.download_limit,
            AutoDownloadLimitGroup::Autoplay => self.autoplay_limit,
        }
    }

    pub fn set_group_limit(&mut self, group: AutoDownloadLimitGroup, bytes: i64) {
        let bytes = bytes.max(1);
        match group {
            AutoDownloadLimitGroup::Download => self.download_limit = bytes,
            AutoDownloadLimitGroup::Autoplay => self.autoplay_limit = bytes,
        }
    }

    /// The exception list `which` (staged).
    pub fn exceptions(&self, which: AutoDownloadOverride) -> &std::collections::BTreeSet<i64> {
        match which {
            AutoDownloadOverride::Always => &self.always,
            AutoDownloadOverride::Never => &self.never,
        }
    }

    /// The picker's Save: `chats` become list `which`, and leave the other.
    pub fn set_exceptions(
        &mut self,
        which: AutoDownloadOverride,
        chats: std::collections::BTreeSet<i64>,
    ) {
        let (dest, other) = match which {
            AutoDownloadOverride::Always => (&mut self.always, &mut self.never),
            AutoDownloadOverride::Never => (&mut self.never, &mut self.always),
        };
        other.retain(|chat| !chats.contains(chat));
        *dest = chats;
    }

    /// The box's Save (tdesktop `AutoDownloadBox` save button): each shown
    /// kind takes its slider or 0; a hidden kind that is on follows the
    /// download slider, and goes off with the last shown kind. The source's
    /// exceptions become the staged lists.
    pub fn apply(
        &self,
        settings: &mut AutoDownloadSettings,
        source_of: impl Fn(ChatId) -> Option<AutoDownloadSource>,
    ) {
        let mut limits = SourceLimits::DISABLED;
        for kind in AutoDownloadKind::SHOWN {
            if self.is_on(kind) {
                limits.set_limit(kind, self.group_limit(AutoDownloadLimitGroup::of(kind)));
            }
        }
        let any_shown = limits.any_enabled();
        let old = *settings.source(self.source);
        for kind in AutoDownloadKind::ALL
            .into_iter()
            .filter(|kind| kind.is_hidden())
        {
            if any_shown && old.limit(kind) > 0 {
                limits.set_limit(kind, self.download_limit);
            }
        }
        *settings.source_mut(self.source) = limits;
        let source = self.source;
        settings
            .exceptions
            .retain(|chat, _| source_of(ChatId(*chat)) != Some(source));
        for chat in &self.always {
            settings
                .exceptions
                .insert(*chat, AutoDownloadOverride::Always);
        }
        for chat in &self.never {
            settings
                .exceptions
                .insert(*chat, AutoDownloadOverride::Never);
        }
    }
}

/// What `media_prefs.json` holds: the settings, or the older per-source
/// bit grids (photo 0x01, voice 0x02, video 0x04, file 0x08, music 0x10,
/// GIF 0x20, round video 0x40) of files written before them.
#[derive(Serialize, Deserialize, Default)]
pub(super) struct AutoDownloadFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_download: Option<AutoDownloadSettings>,
    #[serde(default, skip_serializing)]
    auto_download_private: Option<u8>,
    #[serde(default, skip_serializing)]
    auto_download_groups: Option<u8>,
    #[serde(default, skip_serializing)]
    auto_download_channels: Option<u8>,
}

/// One old bit grid: each kind that was on gets tdesktop's default limit
/// for it, the rest are off.
fn limits_from_bits(bits: u8) -> SourceLimits {
    let mut limits = SourceLimits::DISABLED;
    for (bit, kind) in [
        (0x01, AutoDownloadKind::Photo),
        (0x02, AutoDownloadKind::Voice),
        (0x04, AutoDownloadKind::Video),
        (0x08, AutoDownloadKind::File),
        (0x10, AutoDownloadKind::Music),
        (0x20, AutoDownloadKind::Gif),
        (0x40, AutoDownloadKind::VideoMessage),
    ] {
        if bits & bit != 0 {
            let limit = if kind.is_autoplay() {
                DEFAULT_AUTOPLAY_LIMIT
            } else {
                DEFAULT_DOWNLOAD_LIMIT
            };
            limits.set_limit(kind, limit);
        }
    }
    limits
}

/// The old grid's default (photos, voice, GIFs, round videos).
const OLD_DEFAULT_BITS: u8 = 0x01 | 0x02 | 0x20 | 0x40;

impl From<AutoDownloadFile> for AutoDownloadSettings {
    fn from(file: AutoDownloadFile) -> Self {
        if let Some(settings) = file.auto_download {
            return settings;
        }
        let old = [
            file.auto_download_private,
            file.auto_download_groups,
            file.auto_download_channels,
        ];
        if old.iter().all(Option::is_none) {
            return Self::default();
        }
        let [private, groups, channels] =
            old.map(|bits| limits_from_bits(bits.unwrap_or(OLD_DEFAULT_BITS)));
        Self {
            private,
            groups,
            channels,
            exceptions: BTreeMap::new(),
        }
    }
}

impl From<AutoDownloadSettings> for AutoDownloadFile {
    fn from(settings: AutoDownloadSettings) -> Self {
        Self {
            auto_download: Some(settings),
            ..Self::default()
        }
    }
}

/// The settings as `media_prefs.json` stores them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "AutoDownloadFile", into = "AutoDownloadFile")]
pub struct AutoDownloadPrefs(pub AutoDownloadSettings);

impl From<AutoDownloadFile> for AutoDownloadPrefs {
    fn from(file: AutoDownloadFile) -> Self {
        Self(file.into())
    }
}

impl From<AutoDownloadPrefs> for AutoDownloadFile {
    fn from(prefs: AutoDownloadPrefs) -> Self {
        prefs.0.into()
    }
}

impl std::ops::Deref for AutoDownloadPrefs {
    type Target = AutoDownloadSettings;
    fn deref(&self) -> &AutoDownloadSettings {
        &self.0
    }
}

impl std::ops::DerefMut for AutoDownloadPrefs {
    fn deref_mut(&mut self) -> &mut AutoDownloadSettings {
        &mut self.0
    }
}

#[cfg(test)]
#[path = "auto_download_tests.rs"]
mod tests;
