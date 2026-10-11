//! HTML pages for a chat: `messages.html`, `messages2.html`, ... with at
//! most [`MESSAGES_PER_FILE`] messages each (tdesktop's `kMessagesInFile`).
//! Each page links `css/style.css` and `js/script.js` from the export root
//! (see `assets.rs`); in a full account export the chat lives in
//! `chats/chat_NN/` and its header leads back to the chats list.
use super::html_media::body_html;
use super::model::{Body, ExportMessage, Origin};
use super::settings::Skip;
use super::text::escape;
use super::userpic::{self, Userpic};
use crate::local_time::{civil_at, month_name};

pub use super::html_media::{human_size, noun_skip_note, skip_note, url_path};

/// Messages per HTML file, as in tdesktop.
pub const MESSAGES_PER_FILE: usize = 1000;
/// Messages from one sender this close together share one name header.
const JOIN_WITHIN_SECS: i32 = 900;

/// What happened to a message's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Copied to `path`, relative to the chat's folder, with its preview
    /// image next to it when there is one (`<path>_thumb.jpg`).
    Saved {
        path: String,
        thumb: Option<String>,
    },
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

/// The start of any export page, through the opening of `<main>`. `base`
/// leads from the page to the export root (`""` or `"../"`, `"../../"`);
/// `back` is the header's back link, relative to the root.
pub fn page_start(title: &str, base: &str, back: Option<&str>) -> String {
    let title = escape(title);
    let back = back.map_or(String::new(), |target| {
        format!(
            "<a class=\"back\" href=\"{}\" onclick=\"return GoBack(this)\" aria-label=\"Back\">&#8249;</a>\n",
            escape(&format!("{base}{target}"))
        )
    });
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<link rel=\"stylesheet\" href=\"{base}css/style.css\">\n\
         <script src=\"{base}js/script.js\"></script>\n</head>\n\
         <body onload=\"CheckLocation();\">\n<main class=\"page\">\n<header>{back}<h1>{title}</h1>\n"
    )
}

/// The end of any export page.
pub const PAGE_END: &str = "</main>\n</body>\n</html>\n";

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

/// `14.11.2023 22:13:20`, tdesktop's `FormatDateTime`.
pub fn full_stamp(date: i32, offset: i32) -> String {
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

/// The link to a replied-to message: `#go_to_message<id>` on the same page
/// (the script scrolls to it and highlights it) or
/// `messagesN.html#go_to_message<id>` on another page of the same chat.
fn reply_link(id: i64, href: &str) -> String {
    let onclick = if href.starts_with('#') {
        format!(" onclick=\"return GoToMessage({id})\"")
    } else {
        String::new()
    };
    format!("<a href=\"{}\"{onclick}>this message</a>", escape(href))
}

/// The round picture next to a message: the sender's initials in their
/// color (tdesktop's export never shows the real photo here).
fn sender_userpic(msg: &ExportMessage) -> String {
    let color = msg
        .sender
        .map_or(0, |who| userpic::color_index(userpic::bare_id(who)));
    let picture = Userpic {
        color,
        size: userpic::HISTORY_SIZE,
        name: msg.from.as_deref().unwrap_or_default(),
        one_letter: matches!(msg.sender, Some(super::model::Who::Chat(_))),
        ..Default::default()
    }
    .render();
    format!("<div class=\"pull_left userpic_wrap\">{picture}</div>\n")
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
    let mut out = format!("<div class=\"{class}\" id=\"message{}\">\n", msg.id);
    if !joined {
        out.push_str(&sender_userpic(msg));
    }
    out.push_str(&format!(
        "<div class=\"body\">\n<div class=\"pull_right date details\" title=\"{}\">{:02}:{:02}</div>\n",
        full_stamp(msg.date, offset),
        time.hour,
        time.minute
    ));
    if !joined && let Some(from) = &msg.from {
        out.push_str(&format!(
            "<div class=\"from_name\">{}</div>\n",
            escape(from)
        ));
    }
    if let Some(reply) = msg.reply_to {
        let link = match reply_href(reply) {
            Some(href) => reply_link(reply, &href),
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

/// One HTML file of a chat and how it links to the rest of the export.
#[derive(Debug, Clone, Copy)]
pub struct PageSpec<'a> {
    pub title: &'a str,
    /// 0-based index of this file, and the number of files.
    pub page: usize,
    pub pages: usize,
    /// From the chat's folder to the export root: `""` for a chat export,
    /// `"../../"` for `chats/chat_NN/` in an account export.
    pub base: &'a str,
    /// The header's back link, relative to the export root.
    pub back: Option<&'a str>,
}

/// A whole HTML file.
pub fn render_page(
    spec: &PageSpec<'_>,
    items: &[Item<'_>],
    offset_at: &dyn Fn(i64) -> i32,
    reply_href: &dyn Fn(i64) -> Option<String>,
) -> String {
    let nav = |label: &str, target: Option<usize>| {
        target.map_or(String::new(), |index| {
            format!(
                "<a class=\"pagination\" href=\"{}\">{label}</a>\n",
                messages_file(index)
            )
        })
    };
    let previous = nav("Previous messages", spec.page.checked_sub(1));
    let next = nav(
        "Next messages",
        (spec.page + 1 < spec.pages).then_some(spec.page + 1),
    );
    let mut out = page_start(spec.title, spec.base, spec.back);
    out.push_str(&format!("{previous}</header>\n<div class=\"history\">\n"));
    if items.is_empty() {
        out.push_str(
            "<div class=\"message service\"><div class=\"body details\">No exported messages</div></div>\n",
        );
    }
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
        let joined = !new_day && last.is_some_and(|prev| joins(prev, msg, offset_at));
        out.push_str(&message_html(item, joined, offset, reply_href));
        last = Some(msg);
    }
    out.push_str(&format!("</div>\n<footer>{next}</footer>\n{PAGE_END}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::model::{Entity, EntityKind, MediaRef, Who};
    use crate::export::settings::MediaKind;

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
            width: 0,
            height: 0,
            thumb: None,
        });
        m
    }

    fn spec(title: &str, page: usize, pages: usize) -> PageSpec<'_> {
        PageSpec {
            title,
            page,
            pages,
            base: "",
            back: None,
        }
    }

    fn page(items: &[Item<'_>]) -> String {
        render_page(&spec("Chat", 0, 1), items, &|_| 0, &|_| None)
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

    fn saved(path: &str) -> Outcome {
        Outcome::Saved {
            path: path.into(),
            thumb: None,
        }
    }

    #[test]
    fn page_escapes_title_names_and_text() {
        let mut m = msg(1, 1_700_000_000, 1, "<script>alert(1)</script> & \"q\"");
        m.from = Some("Ann <b>".into());
        let html = render_page(
            &spec("Team & <Co>", 0, 1),
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
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("<link rel=\"stylesheet\" href=\"css/style.css\">"));
        assert!(html.contains("<script src=\"js/script.js\"></script>"));
        assert!(html.contains("<body onload=\"CheckLocation();\">"));
    }

    #[test]
    fn pages_inside_an_account_export_reach_the_root_and_go_back() {
        let m = msg(1, 1_700_000_000, 1, "x");
        let items = [Item {
            message: &m,
            outcome: None,
        }];
        let html = render_page(
            &PageSpec {
                title: "Ann",
                page: 0,
                pages: 1,
                base: "../../",
                back: Some("lists/chats.html"),
            },
            &items,
            &|_| 0,
            &|_| None,
        );
        assert!(html.contains("href=\"../../css/style.css\""));
        assert!(html.contains("src=\"../../js/script.js\""));
        assert!(html.contains(
            "<a class=\"back\" href=\"../../lists/chats.html\" onclick=\"return GoBack(this)\""
        ));
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
    fn senders_get_a_round_picture_with_initials_once_per_group() {
        let mut a = msg(1, 1_700_000_000, 7, "one");
        a.from = Some("Ann Lee".into());
        let mut b = msg(2, 1_700_000_060, 7, "two");
        b.from = Some("Ann Lee".into());
        let html = plain(&[a, b]);
        // User 7: bare id 7 -> color 0 -> class userpic1.
        assert_eq!(html.matches("class=\"userpic userpic1\"").count(), 1);
        assert!(html.contains(">AL</div>"));
        assert!(html.contains(
            "id=\"message1\">\n<div class=\"pull_left userpic_wrap\"><div class=\"userpic"
        ));
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
        let saved_photo = saved("photos/photo_1.jpg");
        let items = [
            Item {
                message: &photo,
                outcome: Some(&saved_photo),
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
        let outcomes = [
            saved("video_files/video_1.mp4"),
            saved("voice_messages/audio_1.ogg"),
            saved("files/my report #1.pdf"),
            saved("animations/a.mp4"),
            saved("stickers/s.webp"),
        ];
        let messages = [&video, &voice, &doc, &gif, &sticker];
        let items: Vec<Item<'_>> = messages
            .iter()
            .zip(&outcomes)
            .map(|(message, outcome)| Item {
                message,
                outcome: Some(outcome),
            })
            .collect();
        let html = page(&items);
        assert!(html.contains("<video class=\"media\" src=\"video_files/video_1.mp4\" controls"));
        assert!(html.contains("<audio class=\"media\" src=\"voice_messages/audio_1.ogg\""));
        assert!(html.contains("href=\"files/my%20report%20%231.pdf\""));
        assert!(html.contains("src=\"animations/a.mp4\" autoplay loop muted"));
        assert!(html.contains("<img src=\"stickers/s.webp\""));
    }

    #[test]
    fn replies_jump_on_the_page_or_open_the_right_file() {
        let mut m = msg(10, 1_700_000_000, 1, "answer");
        m.reply_to = Some(9);
        m.edited = 1_700_000_100;
        let mut near = msg(13, 1_700_000_200, 1, "near");
        near.reply_to = Some(10);
        let mut f = msg(11, 1_700_000_500, 1, "fwd");
        f.forward = Some(Origin::Who(Who::Chat(-5)));
        f.forwarded_from = Some("News <Daily>".into());
        let mut orphan = msg(12, 1_700_000_900, 1, "x");
        orphan.reply_to = Some(1);
        let items: Vec<Item<'_>> = [&m, &near, &f, &orphan]
            .iter()
            .map(|message| Item {
                message,
                outcome: None,
            })
            .collect();
        let html = render_page(&spec("C", 0, 1), &items, &|_| 0, &|id| match id {
            9 => Some("messages2.html#go_to_message9".to_string()),
            10 => Some("#go_to_message10".to_string()),
            _ => None,
        });
        assert!(
            html.contains("In reply to <a href=\"messages2.html#go_to_message9\">this message</a>")
        );
        assert!(html.contains(
            "In reply to <a href=\"#go_to_message10\" onclick=\"return GoToMessage(10)\">this message</a>"
        ));
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
        assert!(!html.contains("javascript:alert"));
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
    fn an_empty_page_says_so() {
        assert!(plain(&[]).contains("No exported messages"));
    }

    #[test]
    fn pagination_links_connect_the_files() {
        let m = msg(1, 1_700_000_000, 1, "x");
        let items = [Item {
            message: &m,
            outcome: None,
        }];
        let first = render_page(&spec("C", 0, 3), &items, &|_| 0, &|_| None);
        assert!(first.contains("href=\"messages2.html\">Next messages"));
        assert!(!first.contains("Previous messages"));
        let middle = render_page(&spec("C", 1, 3), &items, &|_| 0, &|_| None);
        assert!(middle.contains("href=\"messages.html\">Previous messages"));
        assert!(middle.contains("href=\"messages3.html\">Next messages"));
        let last = render_page(&spec("C", 2, 3), &items, &|_| 0, &|_| None);
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
