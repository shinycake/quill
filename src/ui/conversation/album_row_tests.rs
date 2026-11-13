//! An album row carries what a single message's bubble does: the reactions
//! of its items (whichever item they sit on, following them as they come
//! and go), its headers (reply quote, "Forwarded from") and its replies
//! bar (Telegram Desktop `GroupedMedia` inside one `Message`).
use super::*;

const ALBUM: i64 = 56;

fn file(id: i32) -> String {
    crate::ui::demo::demo_file_json(990 + id, "", false)
}

fn caption(text: &str) -> String {
    format!(r#"{{"@type":"formattedText","text":"{text}","entities":[]}}"#)
}

fn message(id: i32, outgoing: bool, content: String) -> String {
    let sender = if outgoing {
        String::new()
    } else {
        r#""sender_id":{"@type":"messageSenderUser","user_id":21},"#.to_string()
    };
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,{sender}"is_outgoing":{outgoing},"date":1790631900,"media_album_id":"{ALBUM}","content":{content}}}}}"#
    )
}

fn photo(id: i32, outgoing: bool, text: &str) -> String {
    let file = file(id);
    let caption = caption(text);
    message(
        id,
        outgoing,
        format!(
            r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":320,"height":200,"progressive_sizes":[]}}]}},"caption":{caption},"has_spoiler":false,"is_secret":false}}"#
        ),
    )
}

fn video(id: i32, outgoing: bool, text: &str) -> String {
    let file = file(id);
    let caption = caption(text);
    message(
        id,
        outgoing,
        format!(
            r#"{{"@type":"messageVideo","video":{{"@type":"video","duration":3,"width":320,"height":180,"file_name":"v.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{caption},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
        ),
    )
}

fn document(id: i32, text: &str) -> String {
    let file = file(id);
    let caption = caption(text);
    message(
        id,
        false,
        format!(
            r#"{{"@type":"messageDocument","document":{{"@type":"document","file_name":"{id}.pdf","mime_type":"application/pdf","minithumbnail":null,"thumbnail":null,"document":{file}}},"caption":{caption}}}"#
        ),
    )
}

fn audio(id: i32, text: &str) -> String {
    let file = file(id);
    let caption = caption(text);
    message(
        id,
        false,
        format!(
            r#"{{"@type":"messageAudio","audio":{{"@type":"audio","duration":30,"title":"T{id}","performer":"","file_name":"{id}.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[],"audio":{file}}},"caption":{caption}}}"#
        ),
    )
}

/// `item` (an `updateNewMessage`) with more message fields
/// (`,"reply_to":{…}`).
fn with(item: String, fields: &str) -> String {
    let body = item.trim_suffix("}}");
    format!("{body}{fields}}}}}")
}

/// `updateMessageInteractionInfo` with one 😭 reaction, or none.
fn reactions(id: i32, reacted: bool) -> String {
    let list = if reacted {
        r#","reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"😭"},"total_count":1,"is_chosen":false,"recent_sender_ids":[{"@type":"messageSenderUser","user_id":22}]}],"are_tags":false,"can_get_added_reactions":true}"#
    } else {
        ""
    };
    format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":{id},"interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0{list}}}}}"#
    )
}

fn album(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, items: &[String]) {
    open_group(app, vcx);
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    apply(app, vcx, &items);
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
}

fn react(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i32, reacted: bool) {
    let update = reactions(id, reacted);
    apply(app, vcx, &[update.as_str()]);
    frames(vcx);
}

const MOSAIC: &str = "album-mosaic-56";
const FILE_ALBUM: &str = "file-album-56";

/// The reaction row `chips` sits under `above`, inside `within` when given.
fn assert_chips_under(
    vcx: &mut VisualTestContext,
    chips: &'static str,
    above: &'static str,
    within: Option<&'static str>,
) {
    let id = chips;
    let chips = vcx.debug_bounds(chips);
    let above_bounds = vcx.debug_bounds(above);
    let (Some(chips), Some(above_bounds)) = (chips, above_bounds) else {
        panic!("the reactions of {id} and {above} are painted: {chips:?} {above_bounds:?}");
    };
    assert!(
        above_bounds.bottom() <= chips.top(),
        "reactions sit under {above}: {above_bounds:?} {chips:?}"
    );
    if let Some(within) = within {
        let Some(bubble) = vcx.debug_bounds(within) else {
            panic!("{within} is painted");
        };
        assert!(
            bubble.top() <= chips.top() && chips.bottom() <= bubble.bottom(),
            "reactions sit inside {within}: {bubble:?} {chips:?}"
        );
    }
}

#[gpui_kit::test]
fn a_reaction_on_the_second_photo_shows_under_the_album_and_leaves_with_it(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[photo(204, false, ""), photo(205, false, "")]);
    assert!(vcx.debug_bounds(MOSAIC).is_some(), "the album is painted");
    assert!(vcx.debug_bounds("reaction-chips-205").is_none());
    react(&app, vcx, 205, true);
    assert_chips_under(vcx, "reaction-chips-205", MOSAIC, None);
    react(&app, vcx, 205, false);
    assert!(
        vcx.debug_bounds("reaction-chips-205").is_none(),
        "a removed reaction leaves the album"
    );
}

#[gpui_kit::test]
fn an_own_captioned_photo_and_video_album_shows_its_reactions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(
        &app,
        vcx,
        &[photo(204, true, ""), video(205, true, "Both of them")],
    );
    react(&app, vcx, 204, true);
    assert_chips_under(vcx, "reaction-chips-204", MOSAIC, None);
}

#[gpui_kit::test]
fn a_bare_own_album_shows_its_reactions_under_the_pictures(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[video(204, true, ""), photo(205, true, "")]);
    react(&app, vcx, 205, true);
    assert_chips_under(vcx, "reaction-chips-205", MOSAIC, None);
}

#[gpui_kit::test]
fn a_file_album_shows_reactions_in_its_bubble_under_the_last_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[document(204, ""), document(205, "Notes")]);
    react(&app, vcx, 205, true);
    assert_chips_under(
        vcx,
        "reaction-chips-205",
        "file-album-item-205",
        Some(FILE_ALBUM),
    );
    react(&app, vcx, 205, false);
    assert!(vcx.debug_bounds("reaction-chips-205").is_none());
}

#[gpui_kit::test]
fn an_audio_album_shows_reactions_in_its_bubble(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[audio(204, ""), audio(205, "")]);
    react(&app, vcx, 204, true);
    assert_chips_under(
        vcx,
        "reaction-chips-204",
        "file-album-item-205",
        Some(FILE_ALBUM),
    );
}

#[gpui_kit::test]
fn the_first_item_s_reactions_win_when_several_items_have_some(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[photo(204, false, ""), photo(205, false, "")]);
    react(&app, vcx, 205, true);
    react(&app, vcx, 204, true);
    assert!(vcx.debug_bounds("reaction-chips-204").is_some());
    assert!(
        vcx.debug_bounds("reaction-chips-205").is_none(),
        "one reaction row per album"
    );
}

const REPLY_TO_201: &str = r#","reply_to":{"@type":"messageReplyToMessage","chat_id":31,"message_id":201,"quote":null,"checklist_task_id":0,"poll_option_id":""}"#;
const FORWARDED_FROM_BOB: &str = r#","forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":22},"date":1790631000,"source":null,"public_service_announcement_type":""}"#;

/// The headers of `head` open the bubble, above `below`.
fn assert_headers_above(vcx: &mut VisualTestContext, head: &'static str, below: &'static str) {
    let painted = [head, below].map(|id| vcx.debug_bounds(id));
    let [Some(head_bounds), Some(below_bounds)] = painted else {
        panic!("{head} and {below} are painted: {painted:?}");
    };
    assert!(
        head_bounds.bottom() <= below_bounds.top(),
        "the headers sit above {below}: {head_bounds:?} {below_bounds:?}"
    );
}

#[gpui_kit::test]
fn an_album_sent_as_a_reply_shows_the_quote_above_the_pictures(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(
        &app,
        vcx,
        &[
            with(photo(204, true, ""), REPLY_TO_201),
            with(photo(205, true, ""), REPLY_TO_201),
        ],
    );
    assert_headers_above(vcx, "row-headers-204", MOSAIC);
}

#[gpui_kit::test]
fn a_forwarded_album_names_where_it_came_from(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(
        &app,
        vcx,
        &[
            with(photo(204, false, ""), FORWARDED_FROM_BOB),
            with(video(205, false, "Look"), FORWARDED_FROM_BOB),
        ],
    );
    assert_headers_above(vcx, "row-headers-204", MOSAIC);
}

#[gpui_kit::test]
fn a_file_album_sent_as_a_reply_shows_the_quote_above_its_files(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(
        &app,
        vcx,
        &[
            with(document(204, ""), REPLY_TO_201),
            with(document(205, ""), REPLY_TO_201),
        ],
    );
    assert_headers_above(vcx, "row-headers-204", "file-album-item-204");
}

#[gpui_kit::test]
fn an_album_with_replies_shows_its_replies_bar_under_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    album(&app, vcx, &[photo(204, false, ""), photo(205, false, "")]);
    assert!(vcx.debug_bounds("reply-bar-204").is_none());
    let replies = r#"{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":204,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":{"@type":"messageReplyInfo","reply_count":2,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":300}}}"#;
    apply(&app, vcx, &[replies]);
    frames(vcx);
    let painted = [MOSAIC, "reply-bar-204"].map(|id| vcx.debug_bounds(id));
    let [Some(mosaic), Some(bar)] = painted else {
        panic!("the album and its replies bar are painted: {painted:?}");
    };
    assert!(mosaic.bottom() <= bar.top(), "the bar sits under the album");
}
