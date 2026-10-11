//! State reducer tests: mask sticker sets.
use super::common::*;
use crate::state::{MasksTab, StickersPurpose};

fn mask_set_json(id: i64, title: &str) -> String {
    format!(
        r#"{{"@type":"stickerSetInfo","id":"{id}","title":"{title}","name":"{title}Masks","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeMask"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":3,"covers":[]}}"#
    )
}

fn sets_json(extra: crate::ids::RequestId, sets: &[(i64, &str)]) -> String {
    let rows: Vec<String> = sets.iter().map(|(id, t)| mask_set_json(*id, t)).collect();
    format!(
        r#"{{"@type":"stickerSets","total_count":{},"sets":[{}],"@extra":"{}"}}"#,
        rows.len(),
        rows.join(","),
        extra.0
    )
}

#[test]
fn installed_and_archived_masks_stay_out_of_the_regular_panel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);

    let extra = session.request(StickersPurpose::GetInstalledMaskSets.into(), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &sets_json(extra, &[(11, "Hats"), (12, "Beards")]),
    );
    let masks = &session.stickers.masks;
    assert!(masks.installed_loaded);
    assert_eq!(
        masks.installed.iter().map(|s| s.id).collect::<Vec<_>>(),
        [11, 12]
    );
    assert_eq!(masks.installed[0].size, 3);
    assert!(session.stickers.stickers.sets.is_empty());
    assert!(session.stickers.emoji.installed_sets.is_empty());

    // First archived page replaces, the next appends without duplicates.
    let extra = session.request(
        StickersPurpose::GetArchivedMaskSets { first_page: true }.into(),
        None,
    );
    apply_json(&mut session, &seq, &sink, &sets_json(extra, &[(21, "Old")]));
    let extra = session.request(
        StickersPurpose::GetArchivedMaskSets { first_page: false }.into(),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &sets_json(extra, &[(21, "Old"), (22, "Older")]),
    );
    let ids: Vec<_> = session
        .stickers
        .masks
        .archived
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [21, 22]);
    assert!(session.stickers.stickers.archived.is_empty());
    assert!(!session.stickers.masks.archived_has_more);
}

#[test]
fn a_stray_sticker_sets_answer_is_not_taken_for_masks() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickerSets","total_count":1,"sets":[{}]}}"#,
            mask_set_json(11, "Hats")
        ),
    );
    assert!(session.stickers.masks.installed.is_empty());
    assert!(!session.stickers.masks.installed_loaded);
}

#[test]
fn a_mask_set_answer_fills_the_strip_for_that_set() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(StickersPurpose::GetMaskSet { set_id: 11 }.into(), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickerSet","id":"11","title":"Hats","name":"HatsMasks","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeMask"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9101","set_id":"11","width":512,"height":512,"emoji":"🎩","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeMask","mask_position":{{"@type":"maskPosition","point":{{"@type":"maskPointForehead"}},"x_shift":0.0,"y_shift":0.0,"scale":1.0}}}},"thumbnail":null,"sticker":null}}],"emojis":[],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    let stickers = &session.stickers.masks.set_stickers[&11];
    assert_eq!(stickers.len(), 1);
    assert_eq!(stickers[0].emoji, "🎩");
    assert!(!stickers[0].requires_premium);
}

#[test]
fn a_confirmed_change_drops_both_lists_and_an_error_marks_failure() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.stickers.masks.installed_loaded = true;
    session.stickers.masks.archived_loaded = true;
    session.stickers.masks.archived_has_more = true;
    session.stickers.masks.set_requested.insert(11);

    let extra = session.request(
        StickersPurpose::ChangeMaskSet {
            set_id: 11,
            installed: false,
            archived: true,
        }
        .into(),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    let masks = &session.stickers.masks;
    assert!(!masks.installed_loaded && !masks.archived_loaded && !masks.archived_has_more);
    assert!(masks.set_requested.is_empty());

    let extra = session.request(StickersPurpose::ReorderInstalledMaskSets.into(), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"nope","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.stickers.masks.failed);
}

#[test]
fn installed_order_updates_route_by_sticker_type() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(StickersPurpose::GetInstalledMaskSets.into(), None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &sets_json(extra, &[(11, "A"), (12, "B")]),
    );

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateInstalledStickerSets","sticker_type":{"@type":"stickerTypeMask"},"sticker_set_ids":["12","11"]}"#,
    );
    let ids: Vec<_> = session
        .stickers
        .masks
        .installed
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [12, 11]);
    assert!(session.stickers.emoji.installed_sets.is_empty());
}

#[test]
fn the_tab_defaults_to_installed() {
    assert_eq!(MasksTab::default(), MasksTab::Installed);
}
