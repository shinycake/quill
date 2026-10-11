//! HTML pages for a chat export: `messages.html`, `messages2.html`, ... with
//! at most [`MESSAGES_PER_FILE`] messages each (tdesktop's `kMessagesInFile`),
//! plus Quill's own `css/style.css`. The pages use plain anchors only, no
//! script.
use super::model::{Body, ExportMessage, MediaRef, Origin};
use super::settings::{MediaKind, Skip};
use super::text::{escape, render};
use crate::local_time::{civil_at, month_name};

/// Messages per HTML file, as in tdesktop.
pub const MESSAGES_PER_FILE: usize = 1000;
/// Messages from one sender this close together share one name header.
const JOIN_WITHIN_SECS: i32 = 900;

/// What happened to a message's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Copied to this path, relative to the export folder.
    Saved(String),
    Skipped(Skip),
}

/// One message with the result of copying its file.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    pub message: &'a ExportMessage,
    pub outcome: Option<&'a Outcome>,
}

/// `messages.html`, `messages2.html`, ... for the page at `index` (0-based).
pub fn messages_file(index: usize) -> String {
    if index == 0 {
        "messages.html".into()
    } else {
        format!("messages{}.html", index + 1)
    }
}

/// Percent-encode a relative path for use in `href` / `src`.
pub fn url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn kind_noun(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Photo => "Photo",
        MediaKind::Video => "Video",
        MediaKind::VoiceMessage => "Voice message",
        MediaKind::VideoMessage => "Video message",
        MediaKind::Sticker => "Sticker",
        MediaKind::Gif => "GIF",
        MediaKind::File => "File",
    }
}

/// tdesktop's wording for a file left out of the export.
pub fn skip_note(kind: MediaKind, skip: Skip) -> String {
    let noun = kind_noun(kind);
    match skip {
        Skip::NotIncluded => {
            format!("({noun} not included. Change data exporting settings to download.)")
        }
        Skip::TooLarge => {
            format!("({noun} exceeds maximum size. Change data exporting settings to download.)")
        }
        Skip::Unavailable => format!("({noun} unavailable, please try again later)"),
    }
}

fn human_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes.max(0) as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.2} GB", b / KB / KB / KB)
    }
}

fn extension(path: &str) -> String {
    path.rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn saved_media(media: &MediaRef, path: &str) -> String {
    let src = escape(&url_path(path));
    let ext = extension(path);
    let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif");
    let is_video = matches!(ext.as_str(), "mp4" | "mov" | "webm" | "m4v");
    let is_audio = matches!(ext.as_str(), "ogg" | "oga" | "mp3" | "m4a" | "opus" | "wav");
    let label = if media.label.is_empty() {
        String::new()
    } else {
        format!("<div class=\"details\">{}</div>\n", escape(&media.label))
    };
    let body = match media.kind {
        MediaKind::Photo | MediaKind::Sticker if is_image => format!(
            "<a class=\"media image\" href=\"{src}\"><img src=\"{src}\" loading=\"lazy\" alt=\"\"></a>"
        ),
        MediaKind::Gif if is_video => {
            format!("<video class=\"media\" src=\"{src}\" autoplay loop muted playsinline></video>")
        }
        MediaKind::Gif | MediaKind::Sticker if is_image => format!(
            "<a class=\"media image\" href=\"{src}\"><img src=\"{src}\" loading=\"lazy\" alt=\"\"></a>"
        ),
        _ if is_video => {
            format!("<video class=\"media\" src=\"{src}\" controls preload=\"none\"></video>")
        }
        _ if is_audio => {
            format!("<audio class=\"media\" src=\"{src}\" controls preload=\"none\"></audio>")
        }
        _ => format!(
            "<a class=\"media file\" href=\"{src}\">{}</a>",
            escape(path)
        ),
    };
    format!(
        "<div class=\"media_wrap\">\n{body}\n{label}<div class=\"details\">{}</div>\n</div>\n",
        human_size(media.size)
    )
}

fn body_html(item: &Item<'_>) -> String {
    let msg = item.message;
    let mut out = String::new();
    match &msg.body {
        Body::Media(media) => match item.outcome {
            Some(Outcome::Saved(path)) => out.push_str(&saved_media(media, path)),
            Some(Outcome::Skipped(skip)) => out.push_str(&format!(
                "<div class=\"media_wrap\"><div class=\"details\">{}</div></div>\n",
                escape(&skip_note(media.kind, *skip))
            )),
            None => {}
        },
        Body::Location {
            latitude,
            longitude,
        } => out.push_str(&map_link("Location", "", *latitude, *longitude)),
        Body::Venue {
            title,
            address,
            latitude,
            longitude,
        } => {
            let detail = if address.is_empty() {
                title.clone()
            } else {
                format!("{title}, {address}")
            };
            out.push_str(&map_link("Venue", &detail, *latitude, *longitude));
        }
        Body::Contact { name, phone } => out.push_str(&format!(
            "<div class=\"media_wrap\"><div class=\"title\">{}</div><div class=\"details\">{}</div></div>\n",
            escape(name),
            escape(phone)
        )),
        Body::Poll {
            question,
            options,
            total_voters,
        } => {
            out.push_str(&format!(
                "<div class=\"media_wrap\"><div class=\"title\">Poll: {}</div>\n",
                escape(question)
            ));
            for (text, votes) in options {
                out.push_str(&format!(
                    "<div class=\"details\">- {} ({votes})</div>\n",
                    escape(text)
                ));
            }
            out.push_str(&format!(
                "<div class=\"details\">Total votes: {total_voters}</div></div>\n"
            ));
        }
        Body::Dice { emoji, value } => out.push_str(&format!(
            "<div class=\"media_wrap\"><div class=\"title\">{} {value}</div></div>\n",
            escape(emoji)
        )),
        Body::Other => out.push_str(
            "<div class=\"media_wrap\"><div class=\"details\">(This message type is not exported)</div></div>\n",
        ),
        Body::Text | Body::Service { .. } => {}
    }
    if !msg.text.is_empty() {
        out.push_str(&format!(
            "<div class=\"text\">{}</div>\n",
            render(&msg.text, &msg.entities)
        ));
    }
    out
}

fn map_link(label: &str, detail: &str, lat: f64, lon: f64) -> String {
    let detail = if detail.is_empty() {
        String::new()
    } else {
        format!("<div class=\"details\">{}</div>", escape(detail))
    };
    format!(
        "<div class=\"media_wrap\"><div class=\"title\">{label}</div>{detail}\
         <a href=\"https://www.openstreetmap.org/?mlat={lat}&amp;mlon={lon}\">{lat:.5}, {lon:.5}</a></div>\n"
    )
}

/// The service text with `{from}` and `{users}` filled in.
pub fn service_text(msg: &ExportMessage) -> String {
    let Body::Service { text, names, .. } = &msg.body else {
        return String::new();
    };
    let from = msg.from.as_deref().unwrap_or("Someone");
    text.replace("{from}", from)
        .replace("{users}", &names.join(", "))
}

fn day_label(date: i32, offset: i32) -> String {
    let time = civil_at(i64::from(date), offset);
    format!("{} {} {}", time.day, month_name(time.month), time.year)
}

fn full_stamp(date: i32, offset: i32) -> String {
    let unix = i64::from(date);
    let time = civil_at(unix, offset);
    let second = (unix + i64::from(offset)).rem_euclid(60);
    format!(
        "{:02}.{:02}.{:04} {:02}:{:02}:{:02}",
        time.day, time.month, time.year, time.hour, time.minute, second
    )
}

fn joins(prev: &ExportMessage, cur: &ExportMessage, offset_at: &dyn Fn(i64) -> i32) -> bool {
    if prev.is_service() || cur.is_service() || cur.sender.is_none() || prev.sender != cur.sender {
        return false;
    }
    let day =
        |m: &ExportMessage| civil_at(i64::from(m.date), offset_at(i64::from(m.date))).day_number();
    if day(prev) != day(cur) || prev.forward != cur.forward {
        return false;
    }
    let window = if cur.forward.is_some() {
        1
    } else {
        JOIN_WITHIN_SECS
    };
    (cur.date - prev.date).abs() <= window
}

fn forward_name(msg: &ExportMessage) -> Option<String> {
    match (&msg.forward, &msg.forwarded_from) {
        (Some(Origin::Name(name)), _) => Some(name.clone()),
        (Some(Origin::Who(_)), Some(name)) => Some(name.clone()),
        (Some(Origin::Who(_)), None) => Some("Unknown".into()),
        (None, _) => None,
    }
}

/// One message block. `reply_href` maps a replied-to message id to the
/// link that reaches it, when that message is part of the export.
fn message_html(
    item: &Item<'_>,
    joined: bool,
    offset: i32,
    reply_href: &dyn Fn(i64) -> Option<String>,
) -> String {
    let msg = item.message;
    if msg.is_service() {
        return format!(
            "<div class=\"message service\" id=\"message{}\"><div class=\"body details\">{}</div></div>\n",
            msg.id,
            escape(&service_text(msg))
        );
    }
    let class = match (joined, msg.outgoing) {
        (true, true) => "message default clearfix joined out",
        (true, false) => "message default clearfix joined",
        (false, true) => "message default clearfix out",
        (false, false) => "message default clearfix",
    };
    let time = civil_at(i64::from(msg.date), offset);
    let mut out = format!(
        "<div class=\"{class}\" id=\"message{}\">\n<div class=\"body\">\n\
         <div class=\"pull_right date details\" title=\"{}\">{:02}:{:02}</div>\n",
        msg.id,
        full_stamp(msg.date, offset),
        time.hour,
        time.minute
    );
    if !joined && let Some(from) = &msg.from {
        out.push_str(&format!(
            "<div class=\"from_name\">{}</div>\n",
            escape(from)
        ));
    }
    if let Some(reply) = msg.reply_to {
        let link = match reply_href(reply) {
            Some(href) => format!("<a href=\"{}\">this message</a>", escape(&href)),
            None => "a message outside this export".into(),
        };
        out.push_str(&format!(
            "<div class=\"reply_to details\">In reply to {link}</div>\n"
        ));
    }
    let body = body_html(item);
    if let Some(name) = forward_name(msg) {
        out.push_str(&format!(
            "<div class=\"forwarded body\">\n<div class=\"from_name\">{} <span class=\"details\">forwarded</span></div>\n{body}</div>\n",
            escape(&name)
        ));
    } else {
        out.push_str(&body);
    }
    if msg.edited > 0 {
        out.push_str("<div class=\"details\">edited</div>\n");
    }
    out.push_str("</div>\n</div>\n");
    out
}

/// A whole HTML file. `page` is 0-based, `pages` the number of files.
pub fn render_page(
    title: &str,
    page: usize,
    pages: usize,
    items: &[Item<'_>],
    offset_at: &dyn Fn(i64) -> i32,
    reply_href: &dyn Fn(i64) -> Option<String>,
) -> String {
    let title = escape(title);
    let nav = |label: &str, target: Option<usize>| {
        target.map_or(String::new(), |index| {
            format!(
                "<a class=\"pagination\" href=\"{}\">{label}</a>\n",
                messages_file(index)
            )
        })
    };
    let previous = nav("Previous messages", page.checked_sub(1));
    let next = nav("Next messages", (page + 1 < pages).then_some(page + 1));
    let mut out = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<link rel=\"stylesheet\" href=\"css/style.css\">\n</head>\n\
         <body>\n<main class=\"page\">\n<header><h1>{title}</h1>\n{previous}</header>\n<div class=\"history\">\n"
    );
    let mut last: Option<&ExportMessage> = None;
    for item in items {
        let msg = item.message;
        let offset = offset_at(i64::from(msg.date));
        let new_day = last.is_none_or(|prev| {
            let prev_offset = offset_at(i64::from(prev.date));
            civil_at(i64::from(prev.date), prev_offset).day_number()
                != civil_at(i64::from(msg.date), offset).day_number()
        });
        if new_day {
            out.push_str(&format!(
                "<div class=\"message service\"><div class=\"body details\">{}</div></div>\n",
                day_label(msg.date, offset)
            ));
        }
        let joined = last.is_some_and(|prev| joins(prev, msg, offset_at));
        out.push_str(&message_html(item, joined, offset, reply_href));
        last = Some(msg);
    }
    out.push_str(&format!(
        "</div>\n<footer>{next}</footer>\n</main>\n</body>\n</html>\n"
    ));
    out
}

/// Quill's own stylesheet, written to `css/style.css`.
pub const STYLE: &str = "\
:root{color-scheme:light dark;--bg:#f4f5f7;--card:#fff;--out:#e3f0ff;--text:#1c1e21;--muted:#65676b;--accent:#2f6fed}\n\
@media (prefers-color-scheme:dark){:root{--bg:#17212b;--card:#182533;--out:#2b5278;--text:#e8eaed;--muted:#8fa0b3;--accent:#6ab3f3}}\n\
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.45 system-ui,sans-serif}\n\
.page{max-width:760px;margin:0 auto;padding:24px 16px}\n\
h1{font-size:20px;margin:0 0 8px}\n\
a{color:var(--accent)}\n\
.pagination{display:block;text-align:center;padding:12px 0}\n\
.message{margin:6px 0}\n\
.message.service{text-align:center}\n\
.details{color:var(--muted);font-size:13px}\n\
.message.default>.body{background:var(--card);border-radius:10px;padding:8px 12px;max-width:85%;overflow-wrap:anywhere}\n\
.message.default.out>.body{background:var(--out);margin-left:auto}\n\
.message.joined{margin-top:2px}\n\
.pull_right{float:right;margin-left:12px}\n\
.from_name{font-weight:600;font-size:13px;color:var(--accent)}\n\
.forwarded{border-left:2px solid var(--accent);padding-left:8px;margin:4px 0}\n\
.reply_to{margin-bottom:4px}\n\
.media_wrap{margin:4px 0}\n\
.media_wrap .title{font-weight:600}\n\
.media_wrap img,.media_wrap video{max-width:100%;max-height:420px;border-radius:6px;display:block}\n\
.media_wrap audio{width:100%}\n\
.text blockquote{margin:4px 0;padding-left:8px;border-left:2px solid var(--muted)}\n\
.text pre,.text code{font-family:ui-monospace,monospace;background:rgba(127,127,127,.15);border-radius:4px}\n\
.text pre{padding:6px 8px;white-space:pre-wrap}\n\
.spoiler{background:rgba(127,127,127,.35);color:transparent;border-radius:3px}\n\
.spoiler:hover{color:inherit}\n\
.clearfix::after{content:'';display:block;clear:both}\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::model::{Entity, EntityKind, Who};

    fn msg(id: i64, date: i32, sender: i64, text: &str) -> ExportMessage {
        ExportMessage {
            id,
            date,
            edited: 0,
            outgoing: false,
            sender: Some(Who::User(sender)),
            from: Some(format!("User {sender}")),
            reply_to: None,
            forward: None,
            forwarded_from: None,
            text: text.into(),
            entities: Vec::new(),
            body: Body::Text,
        }
    }

    fn media_msg(id: i64, kind: MediaKind, name: &str) -> ExportMessage {
        let mut m = msg(id, 1_700_000_000, 1, "");
        m.body = Body::Media(MediaRef {
            kind,
            file_id: 1,
            size: 2048,
            file_name: name.into(),
            mime: String::new(),
            duration: 0,
            label: String::new(),
        });
        m
    }

    fn page(items: &[Item<'_>]) -> String {
        render_page("Chat", 0, 1, items, &|_| 0, &|_| None)
    }

    fn plain(messages: &[ExportMessage]) -> String {
        let items: Vec<Item<'_>> = messages
            .iter()
            .map(|message| Item {
                message,
                outcome: None,
            })
            .collect();
        page(&items)
    }

    #[test]
    fn page_escapes_title_names_and_text() {
        let mut m = msg(1, 1_700_000_000, 1, "<script>alert(1)</script> & \"q\"");
        m.from = Some("Ann <b>".into());
        let html = render_page(
            "Team & <Co>",
            0,
            1,
            &[Item {
                message: &m,
                outcome: None,
            }],
            &|_| 0,
            &|_| None,
        );
        assert!(html.contains("<title>Team &amp; &lt;Co&gt;</title>"));
        assert!(html.contains("Ann &lt;b&gt;"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;q&quot;"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("<link rel=\"stylesheet\" href=\"css/style.css\">"));
    }

    #[test]
    fn day_headings_and_joined_senders() {
        let a = msg(1, 1_700_000_000, 1, "one");
        let b = msg(2, 1_700_000_060, 1, "two");
        let c = msg(3, 1_700_000_100, 2, "three");
        let d = msg(4, 1_700_000_000 + 2 * 86_400, 2, "later");
        let html = plain(&[a, b, c, d]);
        assert_eq!(
            html.matches("<div class=\"from_name\">User 1</div>")
                .count(),
            1
        );
        assert_eq!(
            html.matches("<div class=\"from_name\">User 2</div>")
                .count(),
            2
        );
        assert!(html.contains("message default clearfix joined\" id=\"message2\""));
        assert!(html.contains("<div class=\"body details\">14 November 2023</div>"));
        assert!(html.contains("<div class=\"body details\">16 November 2023</div>"));
        assert!(html.contains("id=\"message3\""));
    }

    #[test]
    fn far_apart_messages_are_not_joined() {
        let a = msg(1, 1_700_000_000, 1, "one");
        let b = msg(2, 1_700_000_000 + 901, 1, "two");
        assert!(!joins(&a, &b, &|_| 0));
        let c = msg(3, 1_700_000_000 + 900, 1, "two");
        assert!(joins(&a, &c, &|_| 0));
    }

    #[test]
    fn media_outcomes_render_files_and_notes() {
        let photo = media_msg(1, MediaKind::Photo, "");
        let big = media_msg(2, MediaKind::Video, "");
        let off = media_msg(3, MediaKind::File, "a b.pdf");
        let items = [
            Item {
                message: &photo,
                outcome: Some(&Outcome::Saved("photos/photo_1.jpg".into())),
            },
            Item {
                message: &big,
                outcome: Some(&Outcome::Skipped(Skip::TooLarge)),
            },
            Item {
                message: &off,
                outcome: Some(&Outcome::Skipped(Skip::NotIncluded)),
            },
        ];
        let html = page(&items);
        assert!(html.contains("<img src=\"photos/photo_1.jpg\""));
        assert!(
            html.contains(
                "(Video exceeds maximum size. Change data exporting settings to download.)"
            )
        );
        assert!(html.contains("(File not included. Change data exporting settings to download.)"));
        assert!(html.contains("2.0 KB"));
    }

    #[test]
    fn saved_files_pick_a_player_by_extension() {
        let video = media_msg(1, MediaKind::Video, "");
        let voice = media_msg(2, MediaKind::VoiceMessage, "");
        let doc = media_msg(3, MediaKind::File, "");
        let gif = media_msg(4, MediaKind::Gif, "");
        let sticker = media_msg(5, MediaKind::Sticker, "");
        let items = [
            Item {
                message: &video,
                outcome: Some(&Outcome::Saved("video_files/video_1.mp4".into())),
            },
            Item {
                message: &voice,
                outcome: Some(&Outcome::Saved("voice_messages/audio_1.ogg".into())),
            },
            Item {
                message: &doc,
                outcome: Some(&Outcome::Saved("files/my report #1.pdf".into())),
            },
            Item {
                message: &gif,
                outcome: Some(&Outcome::Saved("animations/a.mp4".into())),
            },
            Item {
                message: &sticker,
                outcome: Some(&Outcome::Saved("stickers/s.webp".into())),
            },
        ];
        let html = page(&items);
        assert!(html.contains("<video class=\"media\" src=\"video_files/video_1.mp4\" controls"));
        assert!(html.contains("<audio class=\"media\" src=\"voice_messages/audio_1.ogg\""));
        assert!(html.contains("href=\"files/my%20report%20%231.pdf\""));
        assert!(html.contains("src=\"animations/a.mp4\" autoplay loop muted"));
        assert!(html.contains("<img src=\"stickers/s.webp\""));
    }

    #[test]
    fn replies_forwards_and_edits() {
        let mut m = msg(10, 1_700_000_000, 1, "answer");
        m.reply_to = Some(9);
        m.edited = 1_700_000_100;
        let mut f = msg(11, 1_700_000_500, 1, "fwd");
        f.forward = Some(Origin::Who(Who::Chat(-5)));
        f.forwarded_from = Some("News <Daily>".into());
        let mut orphan = msg(12, 1_700_000_900, 1, "x");
        orphan.reply_to = Some(1);
        let items: Vec<Item<'_>> = [&m, &f, &orphan]
            .iter()
            .map(|message| Item {
                message,
                outcome: None,
            })
            .collect();
        let html = render_page("C", 0, 1, &items, &|_| 0, &|id| {
            (id == 9).then(|| "messages2.html#message9".to_string())
        });
        assert!(html.contains("In reply to <a href=\"messages2.html#message9\">this message</a>"));
        assert!(html.contains("In reply to a message outside this export"));
        assert!(html.contains("News &lt;Daily&gt; <span class=\"details\">forwarded</span>"));
        assert!(html.contains("<div class=\"details\">edited</div>"));
    }

    #[test]
    fn service_messages_fill_in_names() {
        let mut m = msg(1, 1_700_000_000, 1, "");
        m.body = Body::Service {
            text: "{from} invited {users}".into(),
            users: vec![2, 3],
            names: vec!["Bo".into(), "Cy <x>".into()],
        };
        let html = plain(&[m]);
        assert!(html.contains("User 1 invited Bo, Cy &lt;x&gt;"));
        assert!(html.contains("class=\"message service\" id=\"message1\""));
    }

    #[test]
    fn formatted_text_is_rendered_inside_the_message() {
        let mut m = msg(1, 1_700_000_000, 1, "bold link");
        m.entities = vec![
            Entity {
                offset: 0,
                length: 4,
                kind: EntityKind::Bold,
            },
            Entity {
                offset: 5,
                length: 4,
                kind: EntityKind::Link("javascript:alert(1)".into()),
            },
        ];
        let html = plain(&[m]);
        assert!(html.contains("<strong>bold</strong> link"));
        assert!(!html.contains("javascript"));
    }

    #[test]
    fn other_kinds_get_a_description() {
        let mut loc = msg(1, 1_700_000_000, 1, "");
        loc.body = Body::Location {
            latitude: 1.5,
            longitude: -2.25,
        };
        let mut poll = msg(2, 1_700_000_001, 1, "");
        poll.body = Body::Poll {
            question: "Q <1>".into(),
            options: vec![("yes".into(), 3)],
            total_voters: 3,
        };
        let mut other = msg(3, 1_700_000_002, 1, "");
        other.body = Body::Other;
        let html = plain(&[loc, poll, other]);
        assert!(html.contains("mlat=1.5&amp;mlon=-2.25"));
        assert!(html.contains("Poll: Q &lt;1&gt;"));
        assert!(html.contains("- yes (3)"));
        assert!(html.contains("(This message type is not exported)"));
    }

    #[test]
    fn pagination_links_connect_the_files() {
        let m = msg(1, 1_700_000_000, 1, "x");
        let items = [Item {
            message: &m,
            outcome: None,
        }];
        let first = render_page("C", 0, 3, &items, &|_| 0, &|_| None);
        assert!(first.contains("href=\"messages2.html\">Next messages"));
        assert!(!first.contains("Previous messages"));
        let middle = render_page("C", 1, 3, &items, &|_| 0, &|_| None);
        assert!(middle.contains("href=\"messages.html\">Previous messages"));
        assert!(middle.contains("href=\"messages3.html\">Next messages"));
        let last = render_page("C", 2, 3, &items, &|_| 0, &|_| None);
        assert!(!last.contains("Next messages"));
        assert_eq!(messages_file(0), "messages.html");
        assert_eq!(messages_file(4), "messages5.html");
    }

    #[test]
    fn url_path_encodes_reserved_bytes() {
        assert_eq!(
            url_path("files/a b#c%é.txt"),
            "files/a%20b%23c%25%C3%A9.txt"
        );
    }
}
