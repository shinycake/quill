use crate::ids::{FileId, RequestId};
use crate::telegram::envelope::StickerFormat;
use crate::telegram::requests::*;
use serde_json::Value;

fn parse(json: String) -> Value {
    serde_json::from_str(&json).unwrap()
}

#[test]
fn a_new_sticker_is_a_local_file_with_its_emoji() {
    let v = parse(add_sticker_to_set(
        RequestId(3),
        "MySet",
        &NewStickerFile::Local("/tmp/s.png".into()),
        StickerFormat::Webp,
        "😀🔥",
    ));
    assert_eq!(v["@type"], "addStickerToSet");
    assert_eq!(v["@extra"], RequestId(3).as_extra());
    assert_eq!(v["user_id"], 0);
    assert_eq!(v["name"], "MySet");
    assert_eq!(v["sticker"]["@type"], "newSticker");
    assert_eq!(v["sticker"]["sticker"]["@type"], "inputFileLocal");
    assert_eq!(v["sticker"]["sticker"]["path"], "/tmp/s.png");
    assert_eq!(v["sticker"]["format"]["@type"], "stickerFormatWebp");
    assert_eq!(v["sticker"]["emojis"], "😀🔥");
    assert!(v["sticker"]["mask_position"].is_null());
    assert_eq!(v["sticker"]["keywords"], serde_json::json!([]));
}

#[test]
fn an_existing_sticker_keeps_its_format() {
    let v = parse(add_sticker_to_set(
        RequestId(4),
        "MySet",
        &NewStickerFile::Existing(FileId(41)),
        StickerFormat::Tgs,
        "👋",
    ));
    assert_eq!(v["sticker"]["sticker"]["@type"], "inputFileId");
    assert_eq!(v["sticker"]["sticker"]["id"], 41);
    assert_eq!(v["sticker"]["format"]["@type"], "stickerFormatTgs");
    let v = parse(add_sticker_to_set(
        RequestId(5),
        "MySet",
        &NewStickerFile::Existing(FileId(42)),
        StickerFormat::Webm,
        "👋",
    ));
    assert_eq!(v["sticker"]["format"]["@type"], "stickerFormatWebm");
}

#[test]
fn the_other_edits_name_the_sticker_or_the_set() {
    let v = parse(remove_sticker_from_set(RequestId(6), FileId(41)));
    assert_eq!(v["@type"], "removeStickerFromSet");
    assert_eq!(v["sticker"]["id"], 41);
    let v = parse(set_sticker_position_in_set(RequestId(7), FileId(41), 3));
    assert_eq!(v["@type"], "setStickerPositionInSet");
    assert_eq!(v["position"], 3);
    let v = parse(set_sticker_set_title(RequestId(8), "MySet", "Cats"));
    assert_eq!(v["@type"], "setStickerSetTitle");
    assert_eq!(v["name"], "MySet");
    assert_eq!(v["title"], "Cats");
    let v = parse(delete_sticker_set(RequestId(9), "MySet"));
    assert_eq!(v["@type"], "deleteStickerSet");
    assert_eq!(v["name"], "MySet");
}
