use super::media::{Collector, MediaKind, MediaSource};
use super::*;
use crate::export::worker::{ExportError, Tdlib};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A scratch folder removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "quill-rich-html-test-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn plain(text: &str) -> Value {
    json!({"@type": "richTextPlain", "text": text})
}

fn styled(kind: &str, text: Value) -> Value {
    json!({"@type": kind, "text": text})
}

fn texts(parts: Vec<Value>) -> Value {
    json!({"@type": "richTexts", "texts": parts})
}

fn paragraph(text: Value) -> Value {
    json!({"@type": "pageBlockParagraph", "text": text})
}

fn rich(blocks: Vec<Value>) -> Value {
    json!({"@type": "richMessage", "blocks": blocks, "is_rtl": false, "is_full": true})
}

fn labels() -> PageLabels {
    PageLabels {
        media_missing: "Media unavailable".into(),
        embed: "Embedded content".into(),
    }
}

fn date(unix: i64) -> String {
    format!("day {unix}")
}

fn html_with(blocks: Vec<Value>, media: HashMap<i32, String>) -> String {
    page_html(&rich(blocks), "Title", &labels(), &date, media)
}

fn html(blocks: Vec<Value>) -> String {
    html_with(blocks, HashMap::new())
}

/// The `<article>` body only.
fn body(page: &str) -> &str {
    let start = page
        .find("<article>\n")
        .map(|i| i + "<article>\n".len())
        .unwrap();
    let end = page.find("</article>").unwrap();
    page.get(start..end).unwrap()
}

fn file(id: i32) -> Value {
    json!({"@type": "file", "id": id, "size": 3, "expected_size": 3,
        "local": {"@type": "localFile", "path": "", "is_downloading_completed": false}})
}

fn photo_block(file_id: i32) -> Value {
    json!({"@type": "pageBlockPhoto",
        "photo": {"@type": "photo", "has_stickers": false, "sizes": [
            {"@type": "photoSize", "type": "s", "photo": file(file_id + 100), "width": 90, "height": 60},
            {"@type": "photoSize", "type": "y", "photo": file(file_id), "width": 1200, "height": 800},
        ]},
        "caption": {"@type": "pageBlockCaption", "text": plain("Sunset"), "credit": plain("")},
        "url": "", "has_spoiler": false})
}

#[test]
fn message_text_is_escaped_and_cannot_inject_markup() {
    let page = page_html(
        &rich(vec![paragraph(plain(
            "<script>alert('x')</script> & \"quotes\"\nnext line",
        ))]),
        "<b>Title</b>",
        &labels(),
        &date,
        HashMap::new(),
    );
    assert!(page.contains("<title>&lt;b&gt;Title&lt;/b&gt;</title>"));
    assert_eq!(
        body(&page),
        "<p>&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;quotes&quot;<br>\nnext line</p>\n"
    );
    assert!(!page.contains("<script>alert"));
}

#[test]
fn unsafe_link_targets_are_dropped() {
    let link = |url: &str| json!({"@type": "richTextUrl", "text": plain("tap"), "url": url, "is_cached": false});
    let page = html(vec![paragraph(texts(vec![
        link("javascript:alert(1)"),
        link(" data:text/html,<b>x</b>"),
        link("https://example.com/?a=1&b=\"2\""),
    ]))]);
    assert_eq!(
        body(&page),
        "<p>taptap<a href=\"https://example.com/?a=1&amp;b=&quot;2&quot;\">tap</a></p>\n"
    );
}

#[test]
fn entities_become_tdesktops_inline_tags() {
    let page = html(vec![paragraph(texts(vec![
        styled("richTextBold", plain("b")),
        styled("richTextItalic", plain("i")),
        styled("richTextUnderline", plain("u")),
        styled("richTextStrikethrough", plain("s")),
        styled("richTextSpoiler", plain("hidden")),
        styled("richTextFixed", plain("a\nb")),
        styled("richTextSubscript", plain("2")),
        styled("richTextSuperscript", plain("3")),
        styled("richTextMarked", plain("m")),
        json!({"@type": "richTextMention", "text": plain("@quill"), "username": "quill"}),
        json!({"@type": "richTextMentionName", "text": plain("Ann"), "user_id": 42}),
        json!({"@type": "richTextEmailAddress", "text": plain("e"), "email_address": "a@b.c"}),
        json!({"@type": "richTextPhoneNumber", "text": plain("p"), "phone_number": "+123"}),
        json!({"@type": "richTextHashtag", "text": plain("#tag"), "hashtag": "tag"}),
        json!({"@type": "richTextCustomEmoji", "custom_emoji_id": "5368324170671202286", "alternative_text": "👍"}),
        json!({"@type": "richTextMathematicalExpression", "expression": "x<y"}),
        json!({"@type": "richTextDateTime", "text": plain("today"), "unix_time": 1700000000,
            "formatting_type": {"@type": "dateTimeFormattingTypeAbsolute",
                "time_precision": {"@type": "dateTimePartPrecisionShort"},
                "date_precision": {"@type": "dateTimePartPrecisionLong"},
                "show_day_of_week": true}}),
        json!({"@type": "richTextAnchorLink", "text": plain("up"), "anchor_name": "top", "url": ""}),
        json!({"@type": "richTextAnchor", "name": "top"}),
    ]))]);
    assert_eq!(
        body(&page),
        concat!(
            "<p><strong>b</strong><em>i</em><u>u</u><s>s</s><tg-spoiler>hidden</tg-spoiler>",
            "<code>a\nb</code><sub>2</sub><sup>3</sup><mark>m</mark>",
            "<a href=\"https://t.me/quill\">@quill</a><a href=\"tg://user?id=42\">Ann</a>",
            "<a href=\"mailto:a@b.c\">e</a><a href=\"tel:+123\">p</a>#tag",
            "<tg-emoji emoji-id=\"5368324170671202286\">👍</tg-emoji>",
            "<tg-math>x&lt;y</tg-math>",
            "<tg-time unix=\"1700000000\" format=\"wDt\">today</tg-time>",
            "<a href=\"#top\">up</a><a name=\"top\"></a></p>\n"
        )
    );
}

#[test]
fn text_blocks_use_tdesktops_structure() {
    let page = html(vec![
        json!({"@type": "pageBlockTitle", "title": plain("Big")}),
        json!({"@type": "pageBlockSectionHeading", "text": plain("Small"), "size": 9}),
        json!({"@type": "pageBlockAuthorDate", "author": plain("Ann"), "publish_date": 5}),
        json!({"@type": "pageBlockPreformatted", "text": plain("a < b\nc"), "language": " rust "}),
        json!({"@type": "pageBlockDivider"}),
        json!({"@type": "pageBlockFooter", "footer": plain("end")}),
        json!({"@type": "pageBlockThinking", "text": plain("...")}),
        json!({"@type": "pageBlockMathematicalExpression", "expression": "e^{i\\pi}"}),
        json!({"@type": "pageBlockBlockQuote", "blocks": [paragraph(plain("q"))], "credit": plain("who")}),
        json!({"@type": "pageBlockPullQuote", "text": plain("pull"), "credit": plain("")}),
        json!({"@type": "pageBlockDetails", "header": plain("More"), "blocks": [paragraph(plain("body"))], "is_open": true}),
        json!({"@type": "pageBlockChatLink", "title": "News & <co>", "username": "news", "accent_color_id": 0}),
        json!({"@type": "pageBlockDocument", "document": {"file_name": "a.pdf"}}),
        json!({"@type": "pageBlockButtonRow", "buttons": []}),
    ]);
    assert_eq!(
        body(&page),
        concat!(
            "<h1>Big</h1>\n",
            "<h6>Small</h6>\n",
            "<p class=\"byline\">Ann · day 5</p>\n",
            "<pre class=\"code\"><code class=\"language-rust\">a &lt; b\nc</code></pre>\n",
            "<hr>\n",
            "<footer>end</footer>\n",
            "<blockquote class=\"thinking\">...</blockquote>\n",
            "<tg-math-block>e^{i\\pi}</tg-math-block>\n",
            "<blockquote><p>q</p>\n<cite>who</cite></blockquote>\n",
            "<aside>pull</aside>\n",
            "<details open><summary>More</summary>\n<p>body</p>\n</details>\n",
            "<div class=\"channel\"><a href=\"https://t.me/news\">News &amp; &lt;co&gt;</a></div>\n",
        )
    );
}

#[test]
fn lists_and_tables() {
    let item = |text: &str, kind: &str, value: i64, checkbox: Option<bool>| {
        json!({"@type": "pageBlockListItem", "label": "", "blocks": [paragraph(plain(text))],
            "has_checkbox": checkbox.is_some(), "is_checked": checkbox == Some(true),
            "value": value, "type": kind})
    };
    let cell = |text: &str, header: bool, align: &str| {
        json!({"@type": "pageBlockTableCell", "text": plain(text), "is_header": header,
            "colspan": if header { 2 } else { 1 }, "rowspan": 1,
            "align": {"@type": align}, "valign": {"@type": "pageBlockVerticalAlignmentMiddle"}})
    };
    let page = html(vec![
        json!({"@type": "pageBlockList", "items": [item("one", "", 0, Some(true)), item("two", "", 0, None)]}),
        json!({"@type": "pageBlockList", "items": [item("first", "a", 3, None)]}),
        json!({"@type": "pageBlockTable", "caption": plain("Sizes"),
            "cells": [[cell("Head", true, "pageBlockHorizontalAlignmentCenter")],
                      [cell("x", false, "pageBlockHorizontalAlignmentLeft")]],
            "is_bordered": true, "is_striped": false, "is_compact": true}),
    ]);
    assert_eq!(
        body(&page),
        concat!(
            "<ul>\n<li><input type=\"checkbox\" disabled checked> one</li>\n<li>two</li>\n</ul>\n",
            "<ol type=\"a\">\n<li value=\"3\">first</li>\n</ol>\n",
            "<figure class=\"table-wrap\"><div class=\"table-title\">Sizes</div>",
            "<table class=\"bordered compact\">\n",
            "<tr><th colspan=\"2\" style=\"text-align:center;vertical-align:middle;\">Head</th></tr>\n",
            "<tr><td style=\"vertical-align:middle;\">x</td></tr>\n",
            "</table></figure>\n",
        )
    );
}

#[test]
fn media_points_at_saved_files_or_says_unavailable() {
    let video = json!({"@type": "pageBlockVideo",
        "video": {"@type": "video", "width": 640, "height": 360, "file_name": "clip.mp4",
            "mime_type": "video/mp4", "video": file(7)},
        "caption": {"@type": "pageBlockCaption", "text": plain(""), "credit": plain("")},
        "need_autoplay": true, "is_looped": false, "has_spoiler": true});
    let audio = json!({"@type": "pageBlockAudio",
        "audio": {"@type": "audio", "title": "Song", "performer": "Band", "file_name": "song.mp3",
            "mime_type": "audio/mpeg", "audio": file(8)},
        "caption": {"@type": "pageBlockCaption", "text": plain("Live"), "credit": plain("2026")}});
    let media = HashMap::from([
        (5, "media/photo_5.jpg".to_string()),
        (7, "media/my clip#1.mp4".to_string()),
    ]);
    let page = html_with(vec![photo_block(5), video, audio, photo_block(9)], media);
    assert_eq!(
        body(&page),
        concat!(
            "<figure class=\"media\"><img src=\"media/photo_5.jpg\" loading=\"lazy\" width=\"1200\" height=\"800\">",
            "<figcaption>Sunset</figcaption></figure>\n",
            "<figure class=\"media\"><video src=\"media/my%20clip%231.mp4\" autoplay muted loop playsinline ",
            "width=\"640\" height=\"360\" tg-spoiler></video></figure>\n",
            "<figure class=\"audio\"><div class=\"media-missing\">song.mp3</div>",
            "<div class=\"audio-meta\">Band — Song</div><figcaption>Live<br>\n2026</figcaption></figure>\n",
            "<figure class=\"media\"><div class=\"media-missing\">Media unavailable</div>",
            "<figcaption>Sunset</figcaption></figure>\n",
        )
    );
}

#[test]
fn collage_tiles_are_placed_by_percent_and_slideshows_get_their_script() {
    let collage = json!({"@type": "pageBlockCollage", "blocks": [photo_block(1), photo_block(2)],
        "caption": {"@type": "pageBlockCaption", "text": plain(""), "credit": plain("")}});
    let media = HashMap::from([
        (1, "media/a.jpg".to_string()),
        (2, "media/b.jpg".to_string()),
    ]);
    let page = html_with(vec![collage], media.clone());
    let body_html = body(&page);
    assert!(body_html.starts_with("<tg-collage><div class=\"collage-box\" style=\"aspect-ratio:"));
    assert_eq!(
        body_html
            .matches("<div class=\"collage-item\" style=\"left:")
            .count(),
        2
    );
    assert!(body_html.contains("left:0.000%;top:0.000%;"));
    assert!(!page.contains("<script>"));

    let slideshow = json!({"@type": "pageBlockSlideshow", "blocks": [photo_block(1), photo_block(2)],
        "caption": {"@type": "pageBlockCaption", "text": plain(""), "credit": plain("")}});
    let page = html_with(vec![slideshow], media);
    assert!(body(&page).contains("<div class=\"slideshow-box\" style=\"aspect-ratio:1200/800\">"));
    assert_eq!(body(&page).matches("class=\"slideshow-dot\"").count(), 2);
    assert!(page.contains("<script>\n(function(){"));
}

#[test]
fn the_collector_names_files_like_tdesktop() {
    let mut collector = Collector::default();
    let doc = |name: &str, mime: &str, id: i32| json!({"file_name": name, "mime_type": mime, "video": file(id)});
    let a = doc("Clip.mp4", "video/mp4", 11);
    let b = doc("clip.MP4", "video/mp4", 12);
    let c = doc("", "video/webm", 13);
    let long = doc(&format!("{}.mp4", "x".repeat(80)), "video/mp4", 14);
    for meta in [&a, &b, &c, &a, &long] {
        assert_eq!(
            collector.path(MediaKind::Video, meta, meta.get("video").unwrap()),
            None
        );
    }
    let photo = file(15);
    collector.path(MediaKind::Photo, &Value::Null, &photo);
    let names: Vec<&str> = collector
        .jobs
        .iter()
        .map(|job| job.relative.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "media/Clip.mp4".to_string(),
            "media/clip_12.MP4".to_string(),
            "media/video_13.webm".to_string(),
            format!("media/{}.mp4", "x".repeat(44)),
            "media/photo_15.jpg".to_string(),
        ]
    );
}

#[test]
fn the_title_is_the_first_heading_else_the_first_paragraph() {
    let page = rich(vec![
        paragraph(plain("  intro text  ")),
        json!({"@type": "pageBlockHeader", "header": styled("richTextBold", plain("Heading"))}),
    ]);
    assert_eq!(title_from_page(&page).as_deref(), Some("Heading"));
    let long = rich(vec![paragraph(plain(&"word ".repeat(30)))]);
    let title = title_from_page(&long).unwrap();
    assert!(title.chars().count() <= 48 && title.starts_with("word word"));
    assert_eq!(title_from_page(&rich(vec![])), None);
}

#[test]
fn deep_nesting_is_cut_off_instead_of_overflowing() {
    // serde_json refuses input nested deeper than 128, so that is as deep as
    // TDLib's JSON can reach the walker.
    let mut text = plain("deep");
    for _ in 0..120 {
        text = styled("richTextBold", text);
    }
    let page = html(vec![paragraph(text.clone())]);
    assert_eq!(page.matches("<strong>").count(), 49);
    assert!(!page.contains("deep"));
    let mut block = paragraph(text);
    for _ in 0..120 {
        block = json!({"@type": "pageBlockBlockQuote", "blocks": [block], "credit": plain("")});
    }
    let page = html(vec![block]);
    assert!(page.ends_with("</html>\n"));
    assert_eq!(page.matches("<blockquote>").count(), 49);
}

/// TDLib answers scripted per request type.
struct FakeTdlib {
    answers: Box<dyn FnMut(&Value) -> Value>,
    seen: Vec<String>,
}

impl Tdlib for FakeTdlib {
    fn call(&mut self, request: Value, _timeout: Duration) -> Result<Value, ExportError> {
        let kind = request
            .get("@type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.seen.push(kind);
        Ok((self.answers)(&request))
    }

    fn post_unanswered(&mut self, request: Value) {
        let _ = self.call(request, Duration::ZERO);
    }
}

fn request(base: &Path) -> RichHtmlRequest {
    RichHtmlRequest {
        chat_id: crate::ids::ChatId(77),
        message_id: crate::ids::MessageId(5),
        base_dir: base.to_path_buf(),
        fallback_title: "Chat".into(),
        labels: labels(),
    }
}

fn message_with(rich: Value) -> Value {
    json!({"@type": "message", "id": 5, "chat_id": 77, "can_be_saved": true,
        "content": {"@type": "messageRichMessage", "message": rich}})
}

#[test]
fn the_worker_completes_the_page_and_copies_its_media() {
    let dir = Scratch::new("copy");
    let files_root = dir.path().join("tdlib");
    std::fs::create_dir_all(&files_root).unwrap();
    let cached = files_root.join("photo.jpg");
    std::fs::write(&cached, b"jpg").unwrap();
    let base = dir.path().join("Downloads");
    let mut partial = rich(vec![paragraph(plain("partial"))]);
    partial["is_full"] = json!(false);
    let full = rich(vec![
        json!({"@type": "pageBlockTitle", "title": plain("Trip: day/1")}),
        photo_block(5),
        photo_block(6),
    ]);
    let cached_path = cached.to_string_lossy().to_string();
    let mut tdlib = FakeTdlib {
        answers: Box::new(move |request| match request["@type"].as_str().unwrap() {
            "getMessage" => message_with(partial.clone()),
            "getFullRichMessage" => full.clone(),
            "downloadFile" if request["file_id"] == 5 => json!({"@type": "file", "id": 5,
                "size": 3, "local": {"path": cached_path, "is_downloading_completed": true}}),
            _ => json!({"@type": "error", "code": 400, "message": "nope"}),
        }),
        seen: Vec::new(),
    };
    let html = worker::run(&request(&base), &mut tdlib, &files_root, &date).unwrap();
    assert_eq!(html, base.join("Trip_ day_1").join("Trip_ day_1.html"));
    assert_eq!(
        tdlib.seen,
        [
            "getMessage",
            "getFullRichMessage",
            "getFile",
            "downloadFile",
            "getFile",
            "downloadFile"
        ]
    );
    assert_eq!(
        std::fs::read(base.join("Trip_ day_1/media/photo_5.jpg")).unwrap(),
        b"jpg"
    );
    let page = std::fs::read_to_string(&html).unwrap();
    assert!(page.contains("<h1>Trip: day/1</h1>"));
    assert!(page.contains("<img src=\"media/photo_5.jpg\""));
    assert!(page.contains("<div class=\"media-missing\">Media unavailable</div>"));

    // A second save of the same message gets its own folder.
    let again = worker::run(&request(&base), &mut tdlib, &files_root, &date).unwrap();
    assert_eq!(
        again,
        base.join("Trip_ day_1 (2)").join("Trip_ day_1 (2).html")
    );
}

#[test]
fn files_outside_tdlibs_folder_are_never_copied() {
    let dir = Scratch::new("outside");
    let files_root = dir.path().join("tdlib");
    std::fs::create_dir_all(&files_root).unwrap();
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, b"secret").unwrap();
    let base = dir.path().join("out");
    let secret_path = secret.to_string_lossy().to_string();
    let mut tdlib = FakeTdlib {
        answers: Box::new(move |request| match request["@type"].as_str().unwrap() {
            "getMessage" => message_with(rich(vec![photo_block(5)])),
            _ => json!({"@type": "file", "id": 5, "size": 6,
                "local": {"path": secret_path, "is_downloading_completed": true}}),
        }),
        seen: Vec::new(),
    };
    let html = worker::run(&request(&base), &mut tdlib, &files_root, &date).unwrap();
    // No heading or paragraph: the chat's name is the title.
    assert_eq!(html, base.join("Chat").join("Chat.html"));
    assert!(!base.join("Chat/media/photo_5.jpg").exists());
    assert!(
        std::fs::read_to_string(&html)
            .unwrap()
            .contains("media-missing")
    );
}

#[test]
fn a_failed_or_cancelled_export_leaves_no_folder() {
    let dir = Scratch::new("failed");
    let base = dir.path().join("out");
    let mut not_rich = FakeTdlib {
        answers: Box::new(|_| json!({"@type": "message", "content": {"@type": "messageText"}})),
        seen: Vec::new(),
    };
    assert!(worker::run(&request(&base), &mut not_rich, dir.path(), &date).is_err());
    assert!(!base.exists());

    struct Cancelling;
    impl Tdlib for Cancelling {
        fn call(&mut self, request: Value, _: Duration) -> Result<Value, ExportError> {
            match request["@type"].as_str() {
                Some("getMessage") => Ok(message_with(rich(vec![
                    paragraph(plain("Hello")),
                    photo_block(5),
                ]))),
                _ => Err(ExportError::Cancelled),
            }
        }

        fn post_unanswered(&mut self, _: Value) {}
    }
    let result = worker::run(&request(&base), &mut Cancelling, dir.path(), &date);
    assert!(matches!(result, Err(ExportError::Cancelled)));
    assert!(!base.join("Hello").exists());
}

#[test]
fn only_a_single_sent_rich_message_in_an_unprotected_chat_qualifies() {
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId, MessageId};
    use crate::state::Session;
    use std::sync::Arc;
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink.clone());
    let seq = AtomicU64::new(0);
    let apply =
        |session: &mut Session, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>, json: &str| {
            let owned = crate::telegram::client::copy_and_parse(json, seq, sink).unwrap();
            session.apply(owned);
        };
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":77,"title":"Chat","type":{"@type":"chatTypePrivate","user_id":77},"unread_count":0}}"#,
    );
    session.open_chat(ChatId(77));
    let rich_message = json!({"@type": "updateNewMessage", "message": message_with(rich(vec![paragraph(plain("hi"))]))});
    apply(&mut session, &seq, &sink, &rich_message.to_string());
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":6,"chat_id":77,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"plain","entities":[]}}}}"#,
    );
    let chat = ChatId(77);
    assert_eq!(
        selection_target(&session, chat, &[MessageId(5)]),
        Some(MessageId(5))
    );
    assert_eq!(selection_target(&session, chat, &[MessageId(6)]), None);
    assert_eq!(
        selection_target(&session, chat, &[MessageId(5), MessageId(6)]),
        None
    );
    assert_eq!(selection_target(&session, chat, &[]), None);
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasProtectedContent","chat_id":77,"has_protected_content":true}"#,
    );
    assert_eq!(selection_target(&session, chat, &[MessageId(5)]), None);
}

/// A session with chat 77 open and these messages in it.
fn session_with(messages: &[Value]) -> crate::state::Session {
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId};
    use std::sync::Arc;
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = crate::state::Session::new(AccountKey::primary(), sink.clone());
    let seq = AtomicU64::new(0);
    let chat = json!({"@type": "updateNewChat", "chat": {"id": 77, "title": "Chat",
        "type": {"@type": "chatTypePrivate", "user_id": 77}, "unread_count": 0}});
    let updates = std::iter::once(chat).chain(
        messages
            .iter()
            .map(|message| json!({"@type": "updateNewMessage", "message": message})),
    );
    for (index, update) in updates.enumerate() {
        let owned =
            crate::telegram::client::copy_and_parse(&update.to_string(), &seq, &sink).unwrap();
        session.apply(owned);
        if index == 0 {
            session.open_chat(ChatId(77));
        }
    }
    session
}

fn rich_message(id: i64, extra: Value) -> Value {
    let mut message = message_with(rich(vec![paragraph(plain("hi"))]));
    message["id"] = json!(id);
    if let (Some(fields), Some(extra)) = (message.as_object_mut(), extra.as_object()) {
        fields.extend(extra.clone());
    }
    message
}

#[test]
fn unsent_self_destructing_and_unsaveable_messages_never_qualify() {
    use crate::ids::{ChatId, MessageId};
    let mut session = session_with(&[
        rich_message(5, json!({})),
        rich_message(6, json!({})),
        rich_message(7, json!({})),
        rich_message(
            8,
            json!({"self_destruct_type": {"@type": "messageSelfDestructTypeTimer",
                "self_destruct_time": 30}, "self_destruct_in": 30.0}),
        ),
    ]);
    // The send path marks its own rows as still sending or failed.
    if let Some(history) = session.histories.get_mut(&77) {
        if let Some(row) = history.messages.get_mut(&6) {
            row.pending = true;
        }
        if let Some(row) = history.messages.get_mut(&7) {
            row.failed = true;
        }
    }
    let chat = ChatId(77);
    let target = |session: &crate::state::Session, id: i64| {
        selection_target(session, chat, &[MessageId(id)])
    };
    assert_eq!(target(&session, 5), Some(MessageId(5)));
    assert_eq!(target(&session, 6), None, "pending");
    assert_eq!(target(&session, 7), None, "failed");
    assert_eq!(target(&session, 8), None, "self-destructing");

    // The message's menu learned it may not be saved (a bot's
    // `protect_content`): the action goes away for that message only.
    let actions = crate::telegram::envelope::MessageActions {
        can_be_saved: false,
        ..Default::default()
    };
    session.messages.message_menu_actions = Some((chat, MessageId(5), actions));
    assert_eq!(target(&session, 5), None);
    session.messages.message_menu_actions = Some((chat, MessageId(9), actions));
    assert_eq!(target(&session, 5), Some(MessageId(5)));
}

fn worker_with(message: Value) -> FakeTdlib {
    FakeTdlib {
        answers: Box::new(move |request| match request["@type"].as_str().unwrap() {
            "getMessage" => message.clone(),
            _ => json!({"@type": "error", "code": 400, "message": "nope"}),
        }),
        seen: Vec::new(),
    }
}

#[test]
fn messages_whose_content_cant_be_saved_are_refused() {
    let dir = Scratch::new("unsaveable");
    let base = dir.path().join("out");
    let page = rich(vec![paragraph(plain("Secret"))]);
    let run = |message: Value| {
        let mut tdlib = worker_with(message);
        worker::run(&request(&base), &mut tdlib, dir.path(), &date)
    };

    let mut protected = message_with(page.clone());
    protected["can_be_saved"] = json!(false);
    assert!(run(protected).is_err());
    let mut unknown = message_with(page.clone());
    unknown.as_object_mut().unwrap().remove("can_be_saved");
    assert!(run(unknown).is_err());
    let mut timed = message_with(page.clone());
    timed["self_destruct_type"] =
        json!({"@type": "messageSelfDestructTypeTimer", "self_destruct_time": 30});
    assert!(run(timed).is_err());
    assert!(!base.exists());

    // Ephemeral content answers for itself, whatever the message says.
    let ephemeral = |can_be_saved: bool| {
        json!({"@type": "message", "id": 5, "chat_id": 77, "can_be_saved": true,
            "content": {"@type": "messageText"},
            "ephemeral_content": {"@type": "ephemeralMessageContent",
                "can_be_saved": can_be_saved, "has_timestamped_media": false,
                "content": {"@type": "messageRichMessage", "message": page.clone()}}})
    };
    assert!(run(ephemeral(false)).is_err());
    assert!(!base.exists());
    let html = run(ephemeral(true)).unwrap();
    assert_eq!(html, base.join("Secret").join("Secret.html"));
}

#[test]
fn a_panic_while_writing_removes_the_folder() {
    let dir = Scratch::new("panic");
    let base = dir.path().join("out");
    let message = message_with(rich(vec![paragraph(plain("Boom")), photo_block(5)]));
    let mut tdlib = FakeTdlib {
        answers: Box::new(move |request| match request["@type"].as_str().unwrap() {
            "getMessage" => message.clone(),
            _ => panic!("worker panic (expected by this test)"),
        }),
        seen: Vec::new(),
    };
    let files_root = dir.path().to_path_buf();
    let request = request(&base);
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        worker::run(&request, &mut tdlib, &files_root, &date)
    }));
    assert!(unwound.is_err());
    assert!(base.exists());
    assert!(!base.join("Boom").exists());
}

#[test]
fn fragment_links_and_anchors_meet_on_one_name() {
    let page = html(vec![
        json!({"@type": "pageBlockAnchor", "name": " Plan B "}),
        paragraph(texts(vec![
            json!({"@type": "richTextAnchorLink", "text": plain("go"),
                "anchor_name": "Plan%20B", "url": ""}),
            json!({"@type": "richTextUrl", "text": plain("top"), "url": "##Top%2", "is_cached": false}),
        ])),
    ]);
    let body_html = body(&page);
    assert!(body_html.contains("<a name=\"plan b\"></a>"));
    assert!(body_html.contains("<a href=\"#plan b\">go</a>"));
    // A broken escape stays as written.
    assert!(body_html.contains("<a href=\"#top%2\">top</a>"));
}

#[test]
fn file_name_extensions_survive_windows() {
    let mut collector = Collector::default();
    let names: Vec<String> = ["clip. ", "notes.mp4 ", "..", "a.m4a"]
        .iter()
        .zip(1..)
        .map(|(name, id)| {
            let meta = json!({"file_name": name, "mime_type": "video/mp4"});
            collector.path(MediaKind::Video, &meta, &file(id));
            collector.jobs.last().unwrap().relative.clone()
        })
        .collect();
    for name in &names {
        assert!(!name.ends_with(['.', ' ']), "{name:?}");
    }
    assert!(names.iter().any(|name| name.ends_with(".m4a")));
}

#[test]
fn big_collages_are_split_instead_of_vanishing() {
    let photos: Vec<Value> = (1..=14).map(photo_block).collect();
    let collage = json!({"@type": "pageBlockCollage", "blocks": photos,
        "caption": {"@type": "pageBlockCaption", "text": plain("Album"), "credit": plain("")}});
    let page = html(vec![collage]);
    let body_html = body(&page);
    assert_eq!(body_html.matches("<div class=\"collage-box\"").count(), 2);
    assert_eq!(body_html.matches("<div class=\"collage-item\"").count(), 14);
    assert_eq!(
        body_html.matches("<figcaption>Album</figcaption>").count(),
        1
    );

    // One very tall photo: the album layout no longer overflows.
    let tall = json!({"@type": "pageBlockCollage", "blocks": [{"@type": "pageBlockPhoto",
        "photo": {"@type": "photo", "has_stickers": false, "sizes": [
            {"@type": "photoSize", "type": "y", "photo": file(1), "width": 1, "height": 2_000_000_000}]},
        "caption": {"@type": "pageBlockCaption", "text": plain(""), "credit": plain("")},
        "url": "", "has_spoiler": false}],
        "caption": {"@type": "pageBlockCaption", "text": plain(""), "credit": plain("")}});
    assert!(body(&html(vec![tall])).contains("collage-box"));
}
