use super::{
    Accessory, Bar, BarInputs, CIRCLE_DIAMETER, PINNED_SPACE, PickerItem, Pin, PinKind,
    PinnedPanel, StickerSection, Track, ViewerBar, badge_text, bar, drop_index, emoji_items,
    index_at, panel_width, pin_x, pinned_panel, position_text, reordered_ids, sticker_items,
};
use quill::ids::{ChatId, FileId};
use quill::telegram::envelope::{StickerFormat, StickerItem};

fn chat(id: i64, kind: PinKind) -> Pin {
    Pin {
        kind,
        chat_id: Some(ChatId(id)),
        title: format!("Chat {id}"),
        photo: None,
        badge: None,
        online: false,
        pinned: true,
    }
}

fn saved_unpinned() -> Pin {
    Pin {
        pinned: false,
        ..chat(7, PinKind::SavedMessages)
    }
}

fn sticker(id: i64, premium: bool) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id,
        set_id: 1,
        emoji: "😀".into(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(id as i32),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: premium,
        premium_animation: None,
    }
}

fn signed_in() -> BarInputs {
    BarInputs {
        signed_in: true,
        ..BarInputs::default()
    }
}

#[test]
fn lock_recording_and_sign_out_empty_the_bar() {
    let empty = PinnedPanel::default;
    assert_eq!(bar(&BarInputs::default(), empty), Bar::Empty);
    let locked = BarInputs {
        locked: true,
        song: Some(Track::default()),
        ..signed_in()
    };
    assert_eq!(bar(&locked, empty), Bar::Empty);
    let recording = BarInputs {
        recording: true,
        song: Some(Track::default()),
        ..signed_in()
    };
    assert_eq!(bar(&recording, empty), Bar::Empty);
}

#[test]
fn a_song_takes_over_and_the_viewer_comes_first() {
    let track = Track {
        playing: true,
        duration: 60.,
        ..Track::default()
    };
    let song = BarInputs {
        song: Some(track),
        composer_selection: true,
        ..signed_in()
    };
    assert_eq!(bar(&song, PinnedPanel::default), Bar::Audio(track));
    let viewer = BarInputs {
        viewer: Some(ViewerBar::Photo),
        ..song
    };
    assert_eq!(
        bar(&viewer, PinnedPanel::default),
        Bar::Viewer(ViewerBar::Photo)
    );
}

#[test]
fn the_accessory_prefers_formatting_then_the_picker() {
    let accessory = |inputs: BarInputs| match bar(&inputs, PinnedPanel::default) {
        Bar::Main { accessory, .. } => accessory,
        other => panic!("expected the main bar, got {other:?}"),
    };
    assert_eq!(accessory(signed_in()), Accessory::None);
    assert_eq!(
        accessory(BarInputs {
            chat_accepts_stickers_or_text: true,
            ..signed_in()
        }),
        Accessory::Picker
    );
    assert_eq!(
        accessory(BarInputs {
            chat_accepts_stickers_or_text: true,
            composer_selection: true,
            ..signed_in()
        }),
        Accessory::Format
    );
}

#[test]
fn the_panel_leads_with_the_archive_and_an_unpinned_saved_messages() {
    let pinned = vec![chat(1, PinKind::Chat), chat(2, PinKind::Chat)];
    let panel = pinned_panel(true, Some(saved_unpinned()), pinned.clone());
    let kinds: Vec<_> = panel.pins.iter().map(|pin| pin.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            PinKind::Archive,
            PinKind::SavedMessages,
            PinKind::Chat,
            PinKind::Chat
        ]
    );
    assert!(!panel.pins[0].reorderable() && !panel.pins[1].reorderable());
    assert!(panel.pins[2].reorderable());
    // A pinned Saved Messages keeps its pinned place only.
    let mut with_saved = pinned;
    with_saved.insert(1, chat(7, PinKind::SavedMessages));
    let panel = pinned_panel(false, Some(saved_unpinned()), with_saved);
    assert_eq!(panel.pins.len(), 3);
    assert_eq!(panel.pins[1].kind, PinKind::SavedMessages);
    assert!(panel.pins[1].reorderable());
}

#[test]
fn slots_follow_tdesktop_geometry() {
    assert_eq!(pin_x(0), 15.);
    assert_eq!(pin_x(2), 15. + 2. * (CIRCLE_DIAMETER + PINNED_SPACE));
    assert_eq!(panel_width(0), 0.);
    assert_eq!(panel_width(3), pin_x(3) - PINNED_SPACE);
    assert_eq!(index_at(16., 3), Some(0));
    // The gap between pictures belongs half to each side.
    assert_eq!(index_at(pin_x(1) - 14., 3), Some(1));
    assert_eq!(index_at(pin_x(1) - 16., 3), Some(0));
    assert_eq!(index_at(-20., 3), None);
    assert_eq!(index_at(pin_x(3) + 5., 3), None);
}

#[test]
fn dragging_reorders_only_pinned_chats() {
    let pitch = CIRCLE_DIAMETER + PINNED_SPACE;
    // Slots 0-1 are the archive and Saved Messages.
    assert_eq!(drop_index(2, pitch * 0.4, 2, 5), 2);
    assert_eq!(drop_index(2, pitch * 0.6, 2, 5), 3);
    assert_eq!(drop_index(2, pitch * 9., 2, 5), 4);
    assert_eq!(drop_index(3, -pitch * 9., 2, 5), 2);
    let panel = pinned_panel(
        true,
        Some(saved_unpinned()),
        vec![
            chat(1, PinKind::Chat),
            chat(2, PinKind::Chat),
            chat(3, PinKind::Chat),
        ],
    );
    assert_eq!(reordered_ids(&panel.pins, 2, 4), Some(vec![2, 3, 1]));
    assert_eq!(reordered_ids(&panel.pins, 3, 3), None);
}

#[test]
fn stickers_list_favorites_recent_then_sets() {
    let favorites = [sticker(1, false)];
    let recent = [sticker(1, false), sticker(2, false), sticker(3, true)];
    let set = [sticker(4, false)];
    let sets = [StickerSection {
        title: "Cats",
        stickers: &set,
    }];
    let items = sticker_items(&favorites, &recent, &sets, false);
    let shape: Vec<String> = items
        .iter()
        .map(|item| match item {
            PickerItem::Title(title) => title.clone(),
            PickerItem::Sticker(sticker) => sticker.file_id.0.to_string(),
            PickerItem::Emoji(emoji) => emoji.clone(),
        })
        .collect();
    assert_eq!(
        shape,
        vec!["Favorite stickers", "1", "Recently used", "2", "Cats", "4"]
    );
    assert_eq!(
        sticker_items(&[], &[], &[], true),
        vec![PickerItem::Title("No stickers found".into())]
    );
}

#[test]
fn emoji_start_with_the_recent_ones() {
    let items = emoji_items(&["🔥".to_string()]);
    assert_eq!(items[0], PickerItem::Title("Recently used".into()));
    assert_eq!(items[1], PickerItem::Emoji("🔥".into()));
    assert_eq!(items[2], PickerItem::Title("Smileys & Emotion".into()));
    assert!(items.len() > 1000);
    assert!(!items.iter().any(|item| matches!(
        item,
        PickerItem::Emoji(emoji) if emoji.contains('\u{1F3FB}')
    )));
}

#[test]
fn positions_read_like_tdesktop() {
    assert_eq!(position_text(42.7, 190.), "0:42 / 3:10");
    assert_eq!(position_text(3725., 7200.), "1:02:05 / 2:00:00");
    assert_eq!(position_text(f64::NAN, -1.), "0:00 / 0:00");
}

#[test]
fn long_badge_counts_keep_their_last_two_digits() {
    assert_eq!(badge_text(7), "7");
    assert_eq!(badge_text(999), "999");
    assert_eq!(badge_text(1234), "..34");
}
