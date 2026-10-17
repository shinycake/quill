//! What a Files, Music, Voice or Links row says about its message
//! (tdesktop `Overview::Layout::{Document, Voice, Link}`): captured when the
//! page is parsed, turned into display lines by [`SharedMediaMeta::lines`].
#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, tr, tr_with};
use crate::local_time::{civil_local, full_stamp};
use crate::message_a11y::format_duration_text;
use crate::telegram::envelope::{MessageContent, MessageSender, ParsedMessage, format_size_text};

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
        outgoing: bool,
        duration: i32,
    },
    Link {
        title: String,
        description: String,
        urls: Vec<String>,
    },
}

/// The text of one row, ready to draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedMediaLines {
    pub title: String,
    pub status: String,
    pub date: String,
    pub description: String,
    pub links: Vec<String>,
    /// Extension (files) or first letter (links) for the leading tile.
    pub badge: String,
}

impl SharedMediaMeta {
    pub fn from_parsed(message: &ParsedMessage) -> Self {
        match &message.content {
            MessageContent::Document(doc) => SharedMediaMeta::File {
                name: doc.file_name.clone(),
                size: doc.size,
            },
            MessageContent::Audio(audio) => SharedMediaMeta::Music {
                performer: audio.performer.clone(),
                title: audio.title.clone(),
                file_name: audio.file_name.clone(),
                duration: audio.duration,
                size: audio.size,
            },
            MessageContent::VoiceNote(voice) => SharedMediaMeta::Voice {
                sender: message.sender,
                outgoing: message.is_outgoing,
                duration: voice.duration,
            },
            MessageContent::Text(text) => {
                let mut urls: Vec<String> = Vec::new();
                for entity in &text.entities {
                    if let Some(url) = entity.open_href(&text.text)
                        && !urls.iter().any(|known| known == url)
                    {
                        urls.push(url.to_string());
                    }
                }
                let preview = text.link_preview.as_ref();
                if urls.is_empty()
                    && let Some(preview) = preview.filter(|p| !p.url.is_empty())
                {
                    urls.push(preview.url.clone());
                }
                if urls.is_empty() {
                    return SharedMediaMeta::None;
                }
                urls.truncate(MAX_LINKS);
                SharedMediaMeta::Link {
                    title: preview.map(|p| p.title.clone()).unwrap_or_default(),
                    description: preview.map(|p| p.description.clone()).unwrap_or_default(),
                    urls,
                }
            }
            _ => SharedMediaMeta::None,
        }
    }

    /// The address a click on a Links row opens.
    pub fn link_url(&self) -> Option<&str> {
        match self {
            SharedMediaMeta::Link { urls, .. } => urls.first().map(String::as_str),
            _ => None,
        }
    }

    /// The row's text. `sender_name` resolves a voice message's sender.
    pub fn lines(
        &self,
        date: i32,
        sender_name: impl Fn(Option<MessageSender>) -> String,
    ) -> Option<SharedMediaLines> {
        let when = || {
            if date > 0 {
                full_stamp(&civil_local(i64::from(date)))
            } else {
                String::new()
            }
        };
        Some(match self {
            SharedMediaMeta::None => return None,
            SharedMediaMeta::File { name, size } => SharedMediaLines {
                title: if name.is_empty() {
                    tr(keys::IN_DLG_FILE).to_string()
                } else {
                    name.clone()
                },
                status: format_size_text(*size),
                date: when(),
                badge: extension_of(name),
                ..SharedMediaLines::default()
            },
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
                outgoing,
                duration,
            } => SharedMediaLines {
                title: if *outgoing {
                    tr(keys::FROM_YOU).to_string()
                } else {
                    sender_name(*sender)
                },
                status: tr_with(
                    keys::DATE_AND_DURATION,
                    &[
                        ("date", &when()),
                        ("duration", &format_duration_text(i64::from(*duration))),
                    ],
                ),
                ..SharedMediaLines::default()
            },
            SharedMediaMeta::Link {
                title,
                description,
                urls,
            } => {
                let first = urls.first().map(String::as_str).unwrap_or_default();
                let guessed = domain_title(first);
                SharedMediaLines {
                    title: if title.is_empty() {
                        guessed.clone()
                    } else {
                        title.clone()
                    },
                    description: description.clone(),
                    links: urls.iter().map(|url| display_url(url)).collect(),
                    badge: guessed.chars().take(1).collect(),
                    ..SharedMediaLines::default()
                }
            }
        })
    }
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

fn extension_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| !ext.is_empty() && ext.chars().count() <= 5)
        .map(|ext| ext.to_uppercase())
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_names_follow_tdesktop() {
        assert_eq!(song_name("Ada", "Song", "x.mp3"), "Ada \u{2013} Song");
        assert_eq!(song_name("", "Song", "x.mp3"), "Song");
        assert_eq!(song_name("", "", "x.mp3"), "x.mp3");
        assert_eq!(song_name("", "", ""), "Unknown Track");
        assert_eq!(song_name("Ada", "", ""), "Ada \u{2013} Unknown Track");
    }

    #[test]
    fn link_without_a_page_title_is_named_after_its_domain() {
        let meta = SharedMediaMeta::Link {
            title: String::new(),
            description: "About".into(),
            urls: vec![
                "https://news.example.com/a/b".into(),
                "http://x.org/".into(),
            ],
        };
        let lines = meta.lines(0, |_| String::new()).expect("link lines");
        assert_eq!(lines.title, "Example");
        assert_eq!(lines.badge, "E");
        assert_eq!(lines.description, "About");
        assert_eq!(lines.links, ["news.example.com/a/b", "x.org"]);
        assert_eq!(meta.link_url(), Some("https://news.example.com/a/b"));
    }

    #[test]
    fn file_shows_name_size_and_extension() {
        let meta = SharedMediaMeta::File {
            name: "report.final.pdf".into(),
            size: 4 * 1024 * 1024 + 300_000,
        };
        let lines = meta.lines(0, |_| String::new()).expect("file lines");
        assert_eq!(lines.title, "report.final.pdf");
        assert_eq!(lines.status, "4.2 MB");
        assert_eq!(lines.badge, "PDF");
        assert_eq!(lines.date, "");
    }

    #[test]
    fn voice_names_the_sender_or_you() {
        let sender = Some(MessageSender::User { user_id: 7 });
        let voice = |outgoing| SharedMediaMeta::Voice {
            sender,
            outgoing,
            duration: 65,
        };
        let theirs = voice(false).lines(0, |_| "Ada".into()).expect("lines");
        assert_eq!(theirs.title, "Ada");
        assert_eq!(theirs.status, ", 01:05");
        let mine = voice(true).lines(0, |_| "Ada".into()).expect("lines");
        assert_eq!(mine.title, "You");
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
        let lines = meta.lines(0, |_| String::new()).expect("lines");
        assert_eq!(lines.title, "Ada \u{2013} Song");
        assert_eq!(lines.status, "03:21, 2.0 KB");
    }

    #[test]
    fn plain_text_has_no_meta() {
        assert!(SharedMediaMeta::None.lines(0, |_| String::new()).is_none());
    }
}
