//! What a chat export includes: media kinds, the size cap, the output
//! format and the date range (tdesktop `export/export_settings.h` and the
//! size steps in `export/view/export_view_settings.cpp`).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MEGABYTE: i64 = 1024 * 1024;

/// tdesktop's `kSizeValueCount`: how many positions the size slider has.
pub const SIZE_STEPS: usize = 100;
/// tdesktop's default media size limit.
pub const DEFAULT_SIZE_LIMIT: i64 = 8 * MEGABYTE;

/// The media groups the export box lets the user switch on and off. These
/// are tdesktop's `MediaSettings::Type` values, minus the combined masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Photo,
    Video,
    VoiceMessage,
    VideoMessage,
    Sticker,
    Gif,
    File,
}

impl MediaKind {
    pub const ALL: [MediaKind; 7] = [
        Self::Photo,
        Self::Video,
        Self::VoiceMessage,
        Self::VideoMessage,
        Self::Sticker,
        Self::Gif,
        Self::File,
    ];

    fn flag(self) -> u8 {
        1 << (self as u8)
    }

    /// The checkbox label in the export box.
    pub fn label(self) -> &'static str {
        match self {
            Self::Photo => "Photos",
            Self::Video => "Videos",
            Self::VoiceMessage => "Voice messages",
            Self::VideoMessage => "Video messages",
            Self::Sticker => "Stickers",
            Self::Gif => "GIFs",
            Self::File => "Files",
        }
    }

    /// Output folder, same names as tdesktop's `DocumentFolder`.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Photo => "photos",
            Self::Video => "video_files",
            Self::VoiceMessage => "voice_messages",
            Self::VideoMessage => "round_video_messages",
            Self::Sticker => "stickers",
            Self::Gif => "animations",
            Self::File => "files",
        }
    }
}

/// A set of [`MediaKind`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaTypes(u8);

impl Default for MediaTypes {
    /// tdesktop starts with photos only.
    fn default() -> Self {
        Self(MediaKind::Photo.flag())
    }
}

impl MediaTypes {
    pub fn none() -> Self {
        Self(0)
    }

    pub fn all() -> Self {
        MediaKind::ALL
            .iter()
            .fold(Self::none(), |set, kind| set.with(*kind, true))
    }

    pub fn contains(self, kind: MediaKind) -> bool {
        self.0 & kind.flag() != 0
    }

    pub fn with(self, kind: MediaKind, on: bool) -> Self {
        if on {
            Self(self.0 | kind.flag())
        } else {
            Self(self.0 & !kind.flag())
        }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// tdesktop's `SizeLimitByIndex`: slider position (0-99) to bytes. Steps
/// get coarser as the limit grows: 1 MB steps up to 10 MB, then 2, 5, 10,
/// 20, 50, 100 and 200 MB steps up to 4000 MB.
pub fn size_limit_by_index(index: usize) -> i64 {
    let index = index.min(SIZE_STEPS - 1) as i64 + 1;
    let megabytes = if index <= 10 {
        index
    } else if index <= 30 {
        10 + (index - 10) * 2
    } else if index <= 40 {
        50 + (index - 30) * 5
    } else if index <= 60 {
        100 + (index - 40) * 10
    } else if index <= 70 {
        300 + (index - 60) * 20
    } else if index <= 80 {
        500 + (index - 70) * 50
    } else if index <= 90 {
        1000 + (index - 80) * 100
    } else {
        2000 + (index - 90) * 200
    };
    megabytes * MEGABYTE
}

/// The slider position whose limit is closest to `limit` bytes.
pub fn size_index_for(limit: i64) -> usize {
    (0..SIZE_STEPS)
        .min_by_key(|i| (size_limit_by_index(*i) - limit).abs())
        .unwrap_or(0)
}

/// "14 November 2023" for a unix time, in local time.
pub fn date_label(unix: i64) -> String {
    let time = crate::local_time::civil_local(unix);
    format!(
        "{} {} {}",
        time.day,
        crate::local_time::month_name(time.month),
        time.year
    )
}

/// "8 MB" / "1.5 GB" style label for the slider.
pub fn size_limit_label(bytes: i64) -> String {
    let mb = bytes / MEGABYTE;
    if mb >= 1000 {
        format!("{:.1} GB", mb as f64 / 1000.0)
    } else {
        format!("{mb} MB")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Html,
    Json,
    HtmlAndJson,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 3] = [Self::Html, Self::Json, Self::HtmlAndJson];

    pub fn label(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Json => "JSON",
            Self::HtmlAndJson => "HTML and JSON",
        }
    }

    pub fn wants_html(self) -> bool {
        matches!(self, Self::Html | Self::HtmlAndJson)
    }

    pub fn wants_json(self) -> bool {
        matches!(self, Self::Json | Self::HtmlAndJson)
    }
}

/// Why a file was left out of the export; the HTML says so in place of the
/// file (tdesktop's `Data::File::SkipReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The kind is switched off in the settings.
    NotIncluded,
    /// Bigger than the size limit.
    TooLarge,
    /// TDLib could not provide the file.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSettings {
    pub chat_id: i64,
    pub title: String,
    /// Parent folder; the export creates its own subfolder in it.
    pub dir: PathBuf,
    pub format: ExportFormat,
    pub media: MediaTypes,
    pub size_limit: i64,
    /// Unix seconds, inclusive. 0 means from the beginning.
    pub from: i64,
    /// Unix seconds, exclusive. 0 means up to now.
    pub till: i64,
    /// Quill addition: keep only the viewer's own messages.
    pub only_mine: bool,
}

impl ExportSettings {
    pub fn new(chat_id: i64, title: String, dir: PathBuf) -> Self {
        Self {
            chat_id,
            title,
            dir,
            format: ExportFormat::default(),
            media: MediaTypes::default(),
            size_limit: DEFAULT_SIZE_LIMIT,
            from: 0,
            till: 0,
            only_mine: false,
        }
    }

    /// tdesktop's `SkipMessageByDate`: `from <= date < till`.
    pub fn skips_date(&self, date: i32) -> bool {
        let date = i64::from(date);
        (self.from > 0 && date < self.from) || (self.till > 0 && date >= self.till)
    }

    /// Whether paging newest to oldest has gone past the start of the range.
    pub fn before_range(&self, date: i32) -> bool {
        self.from > 0 && i64::from(date) < self.from
    }

    /// Choose the first day of the range (a local day number, days since
    /// 1970-01-01). An end that now falls before it is cleared.
    pub fn set_from_day(&mut self, day: i64) {
        self.from = crate::search_filters::local_midnight(day);
        if self.till > 0 && self.till <= self.from {
            self.till = 0;
        }
    }

    /// Choose the last day of the range, inclusive. A start that now falls
    /// after it is cleared.
    pub fn set_till_day(&mut self, day: i64) {
        self.till = crate::search_filters::local_midnight(day + 1);
        if self.from >= self.till {
            self.from = 0;
        }
    }

    /// The reason a file of `kind` and `size` bytes is not copied, if any.
    pub fn skip_reason(&self, kind: MediaKind, size: i64) -> Option<Skip> {
        if !self.media.contains(kind) {
            Some(Skip::NotIncluded)
        } else if size > self.size_limit {
            Some(Skip::TooLarge)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ExportSettings {
        ExportSettings::new(1, "Chat".into(), PathBuf::from("/tmp"))
    }

    #[test]
    fn size_steps_match_tdesktop() {
        assert_eq!(size_limit_by_index(0), MEGABYTE);
        assert_eq!(size_limit_by_index(9), 10 * MEGABYTE);
        assert_eq!(size_limit_by_index(29), 50 * MEGABYTE);
        assert_eq!(size_limit_by_index(39), 100 * MEGABYTE);
        assert_eq!(size_limit_by_index(59), 300 * MEGABYTE);
        assert_eq!(size_limit_by_index(89), 2000 * MEGABYTE);
        assert_eq!(size_limit_by_index(99), 4000 * MEGABYTE);
        // Out-of-range positions clamp.
        assert_eq!(size_limit_by_index(500), 4000 * MEGABYTE);
        assert_eq!(size_index_for(DEFAULT_SIZE_LIMIT), 7);
        assert_eq!(size_limit_label(8 * MEGABYTE), "8 MB");
        assert_eq!(size_limit_label(1500 * MEGABYTE), "1.5 GB");
    }

    #[test]
    fn media_types_default_to_photos_and_toggle() {
        let set = MediaTypes::default();
        assert!(set.contains(MediaKind::Photo));
        assert!(!set.contains(MediaKind::File));
        let set = set
            .with(MediaKind::File, true)
            .with(MediaKind::Photo, false);
        assert!(set.contains(MediaKind::File) && !set.contains(MediaKind::Photo));
        assert!(MediaTypes::none().is_empty());
        assert!(
            MediaKind::ALL
                .iter()
                .all(|k| MediaTypes::all().contains(*k))
        );
    }

    #[test]
    fn date_range_is_from_inclusive_till_exclusive() {
        let mut s = settings();
        assert!(!s.skips_date(5));
        s.from = 100;
        s.till = 200;
        assert!(s.skips_date(99));
        assert!(!s.skips_date(100));
        assert!(!s.skips_date(199));
        assert!(s.skips_date(200));
        assert!(s.before_range(99));
        assert!(!s.before_range(100));
        s.from = 0;
        assert!(!s.before_range(1));
    }

    #[test]
    fn picking_days_keeps_the_range_ordered() {
        let mut s = settings();
        s.set_from_day(19_700);
        s.set_till_day(19_710);
        assert!(s.from > 0 && s.from < s.till);
        // The end day itself is included: the range ends after that day.
        assert!(s.skips_date(i32::try_from(s.till).unwrap()));
        assert!(!s.skips_date(i32::try_from(s.till - 1).unwrap()));
        // A start after the end clears the end.
        s.set_from_day(19_720);
        assert_eq!(s.till, 0);
        // An end before the start clears the start.
        s.set_till_day(19_700);
        assert_eq!(s.from, 0);
        assert!(s.till > 0);
    }

    #[test]
    fn skip_reason_checks_kind_then_size() {
        let mut s = settings();
        s.media = MediaTypes::none().with(MediaKind::Video, true);
        s.size_limit = 1000;
        assert_eq!(s.skip_reason(MediaKind::Photo, 1), Some(Skip::NotIncluded));
        assert_eq!(s.skip_reason(MediaKind::Video, 1001), Some(Skip::TooLarge));
        assert_eq!(s.skip_reason(MediaKind::Video, 1000), None);
    }
}
