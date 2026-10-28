//! What a Files, Music, Voice or Links row says about its message
//! (tdesktop `Overview::Layout::{Document, Voice, Link}`): captured when the
//! page is parsed, turned into display lines by [`SharedMediaMeta::lines`].
#![deny(clippy::indexing_slicing)]

use crate::ids::FileId;
use crate::lang::{Key, keys, tr, tr_with};
use crate::local_time::{CivilTime, civil_local, hhmm, needs_year};
use crate::message_a11y::format_duration_text;
use crate::telegram::envelope::{
    MessageContent, MessageOrigin, MessageSender, MiniThumbnail, ParsedMessage, TextContent,
    format_size_text,
};
use crate::text::{TextEntity, TextEntityKind, schemeless_url};

/// Most links one row lists (tdesktop lists every entity; a message with a
/// wall of URLs would otherwise make one row taller than the panel).
const MAX_LINKS: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SharedMediaMeta {
    #[default]
    None,
    File {
        name: String,
        size: i64,
    },
    Music {
        performer: String,
        title: String,
        file_name: String,
        duration: i32,
        size: i64,
    },
    Voice {
        sender: Option<MessageSender>,
        /// Who wrote it first, for a forward.
        forwarded: Option<MessageOrigin>,
        duration: i32,
    },
    Link {
        title: String,
        description: String,
        links: Vec<LinkLine>,
        /// What a click on the row opens: the page, else the first link.
        main_url: String,
    },
    /// A photo, video or GIF: the picture of a grid tile.
    Tile(SharedMediaTile),
}

/// What a grid tile of the Media, Photos, Videos and GIFs tabs draws
/// (tdesktop `Overview::Layout::{Photo, Video, Gif}`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedMediaTile {
    /// Sizes to try, preferred first.
    pub thumbs: Vec<FileId>,
    pub width: i32,
    pub height: i32,
    /// The ~40 px blur shown until the thumbnail arrives.
    pub mini: Option<MiniThumbnail>,
    pub badge: TileBadge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TileBadge {
    #[default]
    None,
    /// A video's length in seconds.
    Duration(i32),
}

/// One address listed in a Links row: the text the message shows for it and
/// where it leads (`None` for an e-mail address, which only displays).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLine {
    pub display: String,
    pub url: Option<String>,
}

/// The text of one row, ready to draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedMediaLines {
    pub title: String,
    pub status: String,
    pub date: String,
    pub description: String,
    pub links: Vec<LinkLine>,
    /// Extension (files) or first letter (links) for the leading tile.
    pub badge: String,
    /// The file tile's colour slot (tdesktop `ColorIndexForName`: 0 blue,
    /// 1 green, 2 red, 3 yellow).
    pub tone: u8,
}

impl SharedMediaMeta {
    pub fn from_parsed(message: &ParsedMessage) -> Self {
        match &message.content {
            MessageContent::Document(doc) => SharedMediaMeta::File {
                name: doc.file_name.clone(),
                size: doc.size,
            },
            MessageContent::Photo(photo) => {
                let mut thumbs: Vec<FileId> = Vec::new();
                for size in photo.thumb_size().into_iter().chain(photo.largest_size()) {
                    if !thumbs.contains(&size.file_id) {
                        thumbs.push(size.file_id);
                    }
                }
                let size = photo.thumb_size().or_else(|| photo.largest_size());
                let hidden = photo.is_secret || photo.has_spoiler;
                SharedMediaMeta::Tile(SharedMediaTile {
                    thumbs: if hidden { Vec::new() } else { thumbs },
                    width: size.map_or(0, |size| size.width),
                    height: size.map_or(0, |size| size.height),
                    mini: photo.minithumbnail.clone().filter(|_| !hidden),
                    badge: TileBadge::None,
                })
            }
            MessageContent::Video(video) => {
                let hidden = video.is_secret || video.has_spoiler;
                SharedMediaMeta::Tile(SharedMediaTile {
                    thumbs: video
                        .thumb_file_id()
                        .filter(|_| !hidden && video.thumb_still)
                        .into_iter()
                        .collect(),
                    width: video.thumb_width,
                    height: video.thumb_height,
                    mini: video.minithumbnail.clone().filter(|_| !hidden),
                    badge: TileBadge::Duration(video.duration),
                })
            }
            MessageContent::Animation(gif) => {
                let hidden = gif.is_secret || gif.has_spoiler;
                SharedMediaMeta::Tile(SharedMediaTile {
                    thumbs: gif
                        .thumb_file_id()
                        .filter(|_| !hidden && gif.thumb_still)
                        .into_iter()
                        .collect(),
                    width: gif.thumb_width,
                    height: gif.thumb_height,
                    mini: gif.minithumbnail.clone().filter(|_| !hidden),
                    badge: TileBadge::None,
                })
            }
            MessageContent::Audio(audio) => SharedMediaMeta::Music {
                performer: audio.performer.clone(),
                title: audio.title.clone(),
                file_name: audio.file_name.clone(),
                duration: audio.duration,
                size: audio.size,
            },
            MessageContent::VoiceNote(voice) => SharedMediaMeta::Voice {
                sender: message.sender,
                forwarded: message
                    .forward_info
                    .as_ref()
                    .map(|info| info.origin.clone()),
                duration: voice.duration,
            },
            MessageContent::Text(text) => link_meta(text),
            _ => SharedMediaMeta::None,
        }
    }

    /// The grid tile of a photo, video or GIF.
    pub fn tile(&self) -> Option<&SharedMediaTile> {
        match self {
            SharedMediaMeta::Tile(tile) => Some(tile),
            _ => None,
        }
    }

    /// The address a click on a Links row opens.
    pub fn link_url(&self) -> Option<&str> {
        match self {
            SharedMediaMeta::Link { main_url, .. } => Some(main_url.as_str()),
            _ => None,
        }
    }

    /// The row's text. `now` is the current unix time (a date this year
    /// leaves the year out); `sender_name` and `origin_name` resolve a
    /// voice message's sender and the origin of a forward.
    pub fn lines(
        &self,
        date: i32,
        now: i64,
        sender_name: impl Fn(Option<MessageSender>) -> String,
        origin_name: impl Fn(&MessageOrigin) -> String,
    ) -> Option<SharedMediaLines> {
        let when = || date_time(date, now);
        Some(match self {
            SharedMediaMeta::None | SharedMediaMeta::Tile(_) => return None,
            SharedMediaMeta::File { name, size } => {
                let (badge, tone) = file_tile(name);
                SharedMediaLines {
                    title: if name.is_empty() {
                        tr(keys::IN_DLG_FILE).to_string()
                    } else {
                        name.clone()
                    },
                    status: format_size_text(*size),
                    date: when(),
                    badge,
                    tone,
                    ..SharedMediaLines::default()
                }
            }
            SharedMediaMeta::Music {
                performer,
                title,
                file_name,
                duration,
                size,
            } => SharedMediaLines {
                title: song_name(performer, title, file_name),
                status: tr_with(
                    keys::DURATION_AND_SIZE,
                    &[
                        ("duration", &format_duration_text(i64::from(*duration))),
                        ("size", &format_size_text(*size)),
                    ],
                ),
                ..SharedMediaLines::default()
            },
            SharedMediaMeta::Voice {
                sender,
                forwarded,
                duration,
            } => {
                let length = format_duration_text(i64::from(*duration));
                SharedMediaLines {
                    title: match forwarded {
                        Some(origin) => tr_with(
                            keys::Q_MISC_FORWARDED_FROM,
                            &[("name", &origin_name(origin))],
                        ),
                        None => sender_name(*sender),
                    },
                    status: if date > 0 {
                        tr_with(
                            keys::DATE_AND_DURATION,
                            &[("date", &when()), ("duration", &length)],
                        )
                    } else {
                        length
                    },
                    ..SharedMediaLines::default()
                }
            }
            SharedMediaMeta::Link {
                title,
                description,
                links,
                main_url,
            } => {
                let guessed = domain_title(main_url);
                SharedMediaLines {
                    title: if title.is_empty() {
                        guessed.clone()
                    } else {
                        title.clone()
                    },
                    description: description.clone(),
                    links: links.iter().take(MAX_LINKS).cloned().collect(),
                    badge: guessed.chars().take(1).collect(),
                    ..SharedMediaLines::default()
                }
            }
        })
    }
}

/// A Links row from a text message (tdesktop `Overview::Layout::Link`): the
/// listed addresses are the message's URL, text-link and e-mail entities
/// (the page's address when there are none); the description is the text
/// before the trailing run of them, else the page's own description.
fn link_meta(text: &TextContent) -> SharedMediaMeta {
    let mut links: Vec<LinkLine> = Vec::new();
    for entity in &text.entities {
        let Some(line) = link_line(entity, &text.text) else {
            continue;
        };
        let known = line.url.is_some() && links.iter().any(|known| known.url == line.url);
        if !known {
            links.push(line);
        }
    }
    let preview = text.link_preview.as_ref().filter(|p| !p.url.is_empty());
    if links.is_empty()
        && let Some(preview) = preview
    {
        links.push(LinkLine {
            display: display_url(&preview.url),
            url: Some(preview.url.clone()),
        });
    }
    let main_url = match preview {
        Some(preview) => preview.url.clone(),
        None => links
            .iter()
            .find_map(|line| line.url.clone())
            .unwrap_or_default(),
    };
    if links.is_empty() || main_url.is_empty() {
        return SharedMediaMeta::None;
    }
    let before = text_before_links(&text.text, &text.entities);
    let description = if before.is_empty() {
        preview.map(|p| p.description.clone()).unwrap_or_default()
    } else {
        before
    };
    SharedMediaMeta::Link {
        title: preview.map(|p| p.title.clone()).unwrap_or_default(),
        description,
        links,
        main_url,
    }
}

fn is_link_entity(entity: &TextEntity) -> bool {
    matches!(
        entity.kind,
        TextEntityKind::Url | TextEntityKind::TextUrl { .. } | TextEntityKind::EmailAddress
    )
}

fn link_line(entity: &TextEntity, text: &str) -> Option<LinkLine> {
    if !is_link_entity(entity) {
        return None;
    }
    let shown = text.get(entity.utf8_start..entity.utf8_end)?.to_string();
    let url = match &entity.kind {
        TextEntityKind::EmailAddress => None,
        // A bare address as written, `github.com/x` included.
        TextEntityKind::Url => Some(
            entity
                .open_href(text)
                .map(str::to_string)
                .or_else(|| schemeless_url(&shown))?,
        ),
        _ => Some(entity.open_href(text)?.to_string()),
    };
    Some(LinkLine {
        display: shown,
        url,
    })
}

/// tdesktop's `till` loop: drop the trailing link entities and the
/// separator characters between them.
fn text_before_links(text: &str, entities: &[TextEntity]) -> String {
    const SEPARATORS: &str = ",.-;:`'\"()[]{}<>*&^%$#@!\\/";
    let is_gap = |c: char, extra: bool| {
        c.is_whitespace() || SEPARATORS.contains(c) || (extra && matches!(c, '_' | '=' | '+'))
    };
    let mut till = text.len();
    let mut consumed_all = true;
    for entity in entities.iter().rev() {
        if till == 0 {
            break;
        }
        if !is_link_entity(entity) {
            consumed_all = false;
            break;
        }
        let gap = text.get(entity.utf8_end..till).unwrap_or_default();
        if !gap.chars().all(|c| is_gap(c, true)) {
            consumed_all = false;
            break;
        }
        till = entity.utf8_start;
    }
    let kept = text.get(..till).unwrap_or_default();
    if consumed_all && kept.chars().all(|c| is_gap(c, false)) {
        return String::new();
    }
    kept.trim().to_string()
}

/// The abbreviated month name (tdesktop `MonthSmall`): "Oct".
pub fn month_small(month: u8) -> &'static str {
    const MONTHS: [Key; 12] = [
        keys::MONTH1_SMALL,
        keys::MONTH2_SMALL,
        keys::MONTH3_SMALL,
        keys::MONTH4_SMALL,
        keys::MONTH5_SMALL,
        keys::MONTH6_SMALL,
        keys::MONTH7_SMALL,
        keys::MONTH8_SMALL,
        keys::MONTH9_SMALL,
        keys::MONTH10_SMALL,
        keys::MONTH11_SMALL,
        keys::MONTH12_SMALL,
    ];
    usize::from(month.clamp(1, 12))
        .checked_sub(1)
        .and_then(|ix| MONTHS.get(ix))
        .map(|key| tr(*key))
        .unwrap_or_default()
}

/// tdesktop `langDateTime`: "Oct 10 at 07:00", with the year by
/// [`needs_year`].
fn date_time(date: i32, now: i64) -> String {
    if date <= 0 {
        return String::new();
    }
    short_date_time(&civil_local(i64::from(date)), &civil_local(now))
}

/// [`date_time`] over already-split times, so callers (and tests) choose
/// the time zone.
pub fn short_date_time(at: &CivilTime, current: &CivilTime) -> String {
    let month = month_small(at.month);
    let day = at.day.to_string();
    let day_part = if !needs_year(at, current) {
        tr_with(keys::MONTH_DAY, &[("month", month), ("day", &day)])
    } else {
        tr_with(
            keys::MONTH_DAY_YEAR,
            &[
                ("month", month),
                ("day", &day),
                ("year", &at.year.to_string()),
            ],
        )
    };
    tr_with(
        keys::MEDIAVIEW_DATE_TIME,
        &[("date", &day_part), ("time", &hhmm(at))],
    )
}

/// tdesktop `FormatSongName`: "Performer – Title", the title alone, or the
/// file name, never empty.
pub fn song_name(performer: &str, title: &str, file_name: &str) -> String {
    let unknown = || tr(keys::Q_OPS_UNKNOWN_TRACK).to_string();
    match (performer.is_empty(), title.is_empty()) {
        (true, true) => {
            if file_name.is_empty() {
                unknown()
            } else {
                file_name.to_string()
            }
        }
        (true, false) => title.to_string(),
        (false, true) => format!("{performer} \u{2013} {}", unknown()),
        (false, false) => format!("{performer} \u{2013} {title}"),
    }
}

/// The file tile's label and colour slot (tdesktop
/// `DocumentGenericPreview`): the lowercased extension (the whole name when
/// there is no dot) squeezed in the middle to fit, and a colour by type.
fn file_tile(name: &str) -> (String, u8) {
    const FIT: usize = 5;
    let lower = name.to_lowercase();
    let ext = match lower.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext,
        _ => lower.as_str(),
    };
    let count = ext.chars().count();
    let label = if count <= FIT {
        ext.to_string()
    } else {
        let head: String = ext.chars().take(2).collect();
        let tail: String = ext.chars().skip(count.saturating_sub(2)).collect();
        format!("{head}\u{2026}{tail}")
    };
    let ends = |list: &[&str]| list.iter().any(|e| lower.ends_with(e));
    let tone = if ends(&[".doc", ".docx", ".txt", ".psd"]) {
        0
    } else if ends(&[".xls", ".xlsx", ".csv"]) {
        1
    } else if ends(&[".pdf", ".ppt", ".pptx", ".key"]) {
        2
    } else if ends(&[".zip", ".rar", ".ai", ".mp3", ".mov", ".avi"]) {
        3
    } else {
        let pick = match lower.rsplit_once('.') {
            Some((_, ext)) => ext.chars().next(),
            None => lower.chars().next(),
        };
        (pick.map_or(0, |c| c as u32) % 4) as u8
    };
    (label, tone)
}

/// The host part of a link without scheme, credentials or port.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    host.split(':').next().unwrap_or_default()
}

/// tdesktop names a link without a page title after its domain:
/// "https://news.example.com/x" is "Example".
fn domain_title(url: &str) -> String {
    let labels: Vec<&str> = host_of(url).split('.').filter(|l| !l.is_empty()).collect();
    let Some(name) = labels
        .len()
        .checked_sub(2)
        .and_then(|ix| labels.get(ix))
        .or_else(|| labels.first())
    else {
        return String::new();
    };
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn display_url(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.trim_end_matches('/').to_string()
}

impl crate::state::Session {
    /// Ask for the thumbnails of a page of grid tiles; the connect layer
    /// downloads them on the next ingest (`take_profile_thumbs`).
    pub(crate) fn want_shared_media_thumbs(&mut self, items: &[super::SharedMediaItem]) {
        let ids: Vec<FileId> = items
            .iter()
            .filter_map(|item| item.meta.tile())
            .filter_map(|tile| tile.thumbs.first().copied())
            .filter(|id| id.0 != 0)
            .collect();
        self.media.profile_thumbs_due.extend(ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{LinkPreview, LinkPreviewKind};

    // 2026-10-10 12:00 UTC: the same calendar day in every zone from
    // UTC-12 to UTC+11.
    const DATE: i32 = 1_791_633_600;
    const NOW: i64 = 1_791_633_600;

    fn no_name(_: Option<MessageSender>) -> String {
        String::new()
    }

    fn no_origin(_: &MessageOrigin) -> String {
        String::new()
    }

    fn entity(text: &str, part: &str, kind: TextEntityKind) -> TextEntity {
        let start = text.find(part).expect("part in text");
        TextEntity {
            utf8_start: start,
            utf8_end: start + part.len(),
            kind,
        }
    }

    fn preview(url: &str, title: &str, description: &str) -> LinkPreview {
        LinkPreview {
            url: url.into(),
            display_url: url.into(),
            site_name: String::new(),
            title: title.into(),
            description: description.into(),
            show_large_media: false,
            has_large_media: false,
            show_media_above_description: false,
            show_above_text: false,
            instant_view_version: 0,
            photo: None,
            kind: LinkPreviewKind::default(),
            view_button: None,
        }
    }

    fn content(text: &str, entities: Vec<TextEntity>, link: Option<LinkPreview>) -> TextContent {
        TextContent {
            text: text.into(),
            entities,
            link_preview: link,
            animated: None,
        }
    }

    fn link_lines(meta: &SharedMediaMeta) -> SharedMediaLines {
        meta.lines(0, NOW, no_name, no_origin).expect("link lines")
    }

    #[test]
    fn song_names_follow_tdesktop() {
        assert_eq!(song_name("Ada", "Song", "x.mp3"), "Ada \u{2013} Song");
        assert_eq!(song_name("", "Song", "x.mp3"), "Song");
        assert_eq!(song_name("", "", "x.mp3"), "x.mp3");
        assert_eq!(song_name("", "", ""), "Unknown Track");
        assert_eq!(song_name("Ada", "", ""), "Ada \u{2013} Unknown Track");
    }

    #[test]
    fn text_before_the_link_beats_the_page_description() {
        let body = "Worth a read: https://blog.example.com/a";
        let url = "https://blog.example.com/a";
        let meta = link_meta(&content(
            body,
            vec![entity(body, url, TextEntityKind::Url)],
            Some(preview(url, "Page", "Page description")),
        ));
        let lines = link_lines(&meta);
        assert_eq!(lines.description, "Worth a read:");
        assert_eq!(lines.title, "Page");
        assert_eq!(meta.link_url(), Some(url));
    }

    #[test]
    fn a_message_of_only_the_link_shows_the_page_description() {
        let url = "https://docs.example.org/";
        let meta = link_meta(&content(
            url,
            vec![entity(url, url, TextEntityKind::Url)],
            Some(preview(url, "", "About the docs")),
        ));
        let lines = link_lines(&meta);
        assert_eq!(lines.description, "About the docs");
        assert_eq!(lines.title, "Example");
        assert_eq!(lines.badge, "E");
    }

    #[test]
    fn without_a_preview_the_text_before_the_link_is_the_description() {
        let body = "Look at this https://x.com/foo";
        let meta = link_meta(&content(
            body,
            vec![entity(body, "https://x.com/foo", TextEntityKind::Url)],
            None,
        ));
        assert_eq!(link_lines(&meta).description, "Look at this");
    }

    #[test]
    fn a_text_link_lists_its_label_and_leads_to_its_target() {
        let body = "see release notes";
        let meta = link_meta(&content(
            body,
            vec![entity(
                body,
                "release notes",
                TextEntityKind::TextUrl {
                    url: "https://github.com/o/r/releases".into(),
                },
            )],
            None,
        ));
        let lines = link_lines(&meta);
        let line = lines.links.first().expect("one link");
        assert_eq!(lines.links.len(), 1);
        assert_eq!(line.display, "release notes");
        assert_eq!(line.url.as_deref(), Some("https://github.com/o/r/releases"));
    }

    #[test]
    fn a_bare_address_without_a_scheme_is_a_link() {
        let body = "see github.com/foo";
        let meta = link_meta(&content(
            body,
            vec![entity(body, "github.com/foo", TextEntityKind::Url)],
            None,
        ));
        assert_eq!(meta.link_url(), Some("https://github.com/foo"));
        let lines = link_lines(&meta);
        assert_eq!(
            lines.links.first().map(|l| l.display.as_str()),
            Some("github.com/foo")
        );
    }

    #[test]
    fn plain_text_without_links_has_no_meta() {
        assert_eq!(
            link_meta(&content("hello", Vec::new(), None)),
            SharedMediaMeta::None
        );
    }

    #[test]
    fn file_shows_name_size_and_a_lowercase_tile() {
        let meta = SharedMediaMeta::File {
            name: "Report.final.PDF".into(),
            size: 4 * 1024 * 1024 + 300_000,
        };
        let lines = meta.lines(0, NOW, no_name, no_origin).expect("lines");
        assert_eq!(lines.title, "Report.final.PDF");
        assert_eq!(lines.status, "4.2 MB");
        assert_eq!(lines.badge, "pdf");
        assert_eq!(lines.tone, 2);
        assert_eq!(lines.date, "");
    }

    #[test]
    fn file_tiles_colour_by_type_and_squeeze_long_extensions() {
        assert_eq!(file_tile("a.xlsx"), ("xlsx".to_string(), 1));
        assert_eq!(file_tile("a.zip"), ("zip".to_string(), 3));
        assert_eq!(file_tile("notes.txt"), ("txt".to_string(), 0));
        assert_eq!(file_tile("x.torrent"), ("to\u{2026}nt".to_string(), 0));
        // No dot: the whole name stands in for the extension.
        assert_eq!(file_tile("README"), ("re\u{2026}me".to_string(), 2));
    }

    #[test]
    fn a_dated_row_reads_month_day_and_time() {
        let meta = SharedMediaMeta::File {
            name: "a.txt".into(),
            size: 10,
        };
        let lines = meta.lines(DATE, NOW, no_name, no_origin).expect("lines");
        assert!(lines.date.starts_with("Oct 10 at "), "{}", lines.date);
        let older = meta
            .lines(DATE, NOW + 400 * 86_400, no_name, no_origin)
            .expect("lines");
        assert!(older.date.starts_with("Oct 10, 2026 at "), "{}", older.date);
    }

    #[test]
    fn a_december_row_seen_in_january_has_no_year() {
        let meta = SharedMediaMeta::File {
            name: "a.txt".into(),
            size: 10,
        };
        // DATE is 2026-10-10; 100 days later it is January 2027.
        let lines = meta
            .lines(DATE, NOW + 100 * 86_400, no_name, no_origin)
            .expect("lines");
        assert!(lines.date.starts_with("Oct 10 at "), "{}", lines.date);
    }

    #[test]
    fn voice_names_the_sender_or_the_forward() {
        let sender = Some(MessageSender::User { user_id: 7 });
        let voice = |forwarded| SharedMediaMeta::Voice {
            sender,
            forwarded,
            duration: 65,
        };
        let own = voice(None)
            .lines(DATE, NOW, |_| "Ada".into(), no_origin)
            .expect("lines");
        assert_eq!(own.title, "Ada");
        assert!(own.status.ends_with(", 01:05"), "{}", own.status);
        let origin = MessageOrigin::HiddenUser {
            sender_name: "Bob".into(),
        };
        let forwarded = voice(Some(origin))
            .lines(DATE, NOW, |_| "Ada".into(), |_| "Bob".into())
            .expect("lines");
        assert_eq!(forwarded.title, "Forwarded from Bob");
    }

    #[test]
    fn an_undated_voice_row_shows_only_its_length() {
        let meta = SharedMediaMeta::Voice {
            sender: None,
            forwarded: None,
            duration: 65,
        };
        let lines = meta.lines(0, NOW, no_name, no_origin).expect("lines");
        assert_eq!(lines.status, "01:05");
    }

    #[test]
    fn music_status_is_duration_and_size() {
        let meta = SharedMediaMeta::Music {
            performer: "Ada".into(),
            title: "Song".into(),
            file_name: String::new(),
            duration: 201,
            size: 2048,
        };
        let lines = meta.lines(0, NOW, no_name, no_origin).expect("lines");
        assert_eq!(lines.title, "Ada \u{2013} Song");
        assert_eq!(lines.status, "03:21, 2.0 KB");
    }

    #[test]
    fn plain_text_has_no_meta() {
        assert!(
            SharedMediaMeta::None
                .lines(0, NOW, no_name, no_origin)
                .is_none()
        );
    }
}
