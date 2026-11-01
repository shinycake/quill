//! A received file album is one row: its files stacked in one bubble,
//! each with its own caption under it.
use super::*;

fn document(id: i32, name: &str, caption: &str) -> String {
    let file = crate::ui::demo::demo_file_json(990 + id, "", false);
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,"sender_id":{{"@type":"messageSenderUser","user_id":21}},"is_outgoing":false,"date":1790631900,"media_album_id":"55","content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"{name}","mime_type":"application/pdf","minithumbnail":null,"thumbnail":null,"document":{file}}},"caption":{{"@type":"formattedText","text":"{caption}","entities":[]}}}}}}}}"#
    )
}

#[gpui_kit::test]
fn a_received_two_file_album_is_one_bubble(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (first, second) = (document(204, "a.pdf", ""), document(205, "b.pdf", "Both"));
    apply(&app, vcx, &[first.as_str(), second.as_str()]);
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
    let albums: Vec<Vec<MessageId>> = app.read_with(vcx, |app, _| {
        app.history
            .rows
            .iter()
            .filter_map(|row| match row {
                HistoryRow::Album {
                    album_id: 55,
                    messages,
                    ..
                } => Some(messages.iter().map(|m| m.id).collect()),
                _ => None,
            })
            .collect()
    });
    assert_eq!(albums, [vec![MessageId(204), MessageId(205)]]);
    let painted = [
        "file-album-55",
        "file-album-item-204",
        "file-album-item-205",
    ]
    .map(|id| vcx.debug_bounds(id));
    let [Some(bubble), Some(a), Some(b)] = painted else {
        panic!("the album and both files are painted: {painted:?}");
    };
    assert!(a.bottom() <= b.top(), "the files stack in order");
    assert!(bubble.top() <= a.top() && b.bottom() <= bubble.bottom());
}

#[gpui_kit::test]
fn each_file_of_an_album_keeps_its_own_caption_under_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (first, second) = (
        document(204, "a.pdf", "First"),
        document(205, "b.pdf", "Second"),
    );
    apply(&app, vcx, &[first.as_str(), second.as_str()]);
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
    let painted = [
        "file-album-item-204",
        "file-album-caption-204",
        "file-album-item-205",
        "file-album-caption-205",
    ]
    .map(|id| vcx.debug_bounds(id));
    let [Some(a), Some(a_caption), Some(b), Some(b_caption)] = painted else {
        panic!("both files and both captions are painted: {painted:?}");
    };
    assert!(a.top() < a_caption.top() && a_caption.bottom() <= a.bottom());
    assert!(
        a_caption.bottom() <= b.top(),
        "the first caption sits above the second file"
    );
    assert!(b.top() < b_caption.top() && b_caption.bottom() <= b.bottom());
}
