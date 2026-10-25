//! The sticker panel on a demo `QuillApp`: the category strip with the
//! Premium category, the lock on Premium stickers, the Premium preview a
//! locked sticker opens, and the sticker that flies into the chat. The demo
//! session has no sender, so nothing reaches an account.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::{PanelCell, PanelRow, PanelTab, StickerSource};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::send_fly;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, ImageSource, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId};
use quill::premium_hub::PremiumFeature;
use quill::premium_hub::promo::{PreviewOpen, PromoPage};
use quill::telegram::envelope::{MessageContent, StickerContent, StickerFormat, StickerItem};
use quill::telegram::{EmojiCategory, EmojiCategorySource};
use std::sync::Arc;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn sticker(file: i32, premium: bool) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id: i64::from(file),
        set_id: 77,
        emoji: "😀".into(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(file),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: premium,
        premium_animation: premium.then_some(FileId(900)),
    }
}

fn category(name: &str, source: EmojiCategorySource) -> EmojiCategory {
    EmojiCategory {
        name: name.into(),
        icon: None,
        is_greeting: false,
        source,
    }
}

/// A demo account with Premium or without, and a strip of two categories.
fn seed(app: &mut QuillApp, premium: bool) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.payments.premium_option = Some(premium);
    session.stickers.emoji.sticker_categories_asked = true;
    session.stickers.emoji.sticker_categories = vec![
        category("Premium", EmojiCategorySource::Premium),
        category("Smile", EmojiCategorySource::Search(vec!["😀".into()])),
    ];
    session.stickers.stickers.found_stickers = vec![sticker(1, true), sticker(2, false)];
}

fn open_stickers(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| app.open_media_panel(PanelTab::Stickers, cx));
    frames(cx);
}

fn found_cells(app: &QuillApp) -> usize {
    app.pickers
        .media_panel
        .rows
        .iter()
        .map(|row| match row {
            PanelRow::Cells { cells, .. } => cells
                .iter()
                .filter(|cell| {
                    matches!(
                        cell,
                        PanelCell::Sticker {
                            source: StickerSource::Found,
                            ..
                        }
                    )
                })
                .count(),
            PanelRow::Header { .. } => 0,
        })
        .sum()
}

fn header(app: &QuillApp) -> Option<String> {
    app.pickers
        .media_panel
        .rows
        .iter()
        .find_map(|row| match row {
            PanelRow::Header { label, .. } => Some(label.to_string()),
            PanelRow::Cells { .. } => None,
        })
}

#[gpui_kit::test]
fn choosing_a_category_lists_its_stickers_and_back_returns(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| seed(app, true));
    open_stickers(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pickers.media_panel.category, None);
        assert_ne!(header(app).as_deref(), Some("Premium"));
    });

    app.update(vcx, |app, cx| app.choose_sticker_category(Some(0), cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pickers.media_panel.category, Some(0));
        assert_eq!(header(app).as_deref(), Some("Premium"));
        assert_eq!(found_cells(app), 2, "the answer fills the grid");
    });

    // Typing in the search field leaves the category.
    app.update_in(vcx, |app, window, cx| {
        app.pickers
            .sticker_search_input
            .update(cx, |input, cx| input.set_value("cat", window, cx));
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pickers.media_panel.category, None);
        assert_eq!(header(app).as_deref(), Some("Search results"));
    });

    // Back from a category, and a tab change, clear it too.
    app.update_in(vcx, |app, window, cx| {
        app.pickers
            .sticker_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        app.choose_sticker_category(Some(1), cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(header(app).as_deref(), Some("Smile"));
    });
    app.update(vcx, |app, cx| app.choose_sticker_category(None, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.pickers.media_panel.category, None);
        assert_ne!(header(app).as_deref(), Some("Smile"));
    });
}

#[gpui_kit::test]
fn premium_stickers_are_hidden_where_premium_cannot_be_bought(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| seed(app, false));
    open_stickers(&app, vcx);
    app.update(vcx, |app, cx| app.choose_sticker_category(Some(0), cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(found_cells(app), 2, "a locked sticker is listed");
        assert!(app.sticker_listed(StickerSource::Found, 0));
    });
    // Premium is not sold here: the Premium sticker leaves the list.
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.payments.premium_available = Some(false);
        session.revision += 1;
    });
    app.update(vcx, |_, cx| cx.notify());
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.sticker_listed(StickerSource::Found, 0));
        assert!(app.sticker_listed(StickerSource::Found, 1));
        assert_eq!(found_cells(app), 1);
    });
}

fn premium_features() -> quill::premium_hub::PremiumInfo {
    quill::premium_hub::parse_premium_features(&serde_json::json!({
        "features": [
            {"@type": "premiumFeatureIncreasedLimits"},
            {"@type": "premiumFeatureUniqueStickers"},
        ],
        "limits": [],
    }))
}

#[gpui_kit::test]
fn a_locked_sticker_opens_the_premium_preview_instead_of_sending(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        seed(app, false);
        let session = app.demo_session.as_mut().unwrap();
        session.payments.hub.premium = Some(premium_features());
    });
    app.update(vcx, |app, cx| {
        app.send_sticker_pick(FileId(1), "😀".into(), 512, 512, None, cx);
    });
    app.read_with(vcx, |app, _| {
        let hub = &app.session().unwrap().payments.hub;
        // The locked sticker leads the stickers feature's preview.
        assert_eq!(
            hub.promo_page,
            PromoPage::Preview {
                feature: PremiumFeature::UniqueStickers.into(),
                opened: PreviewOpen::Box,
            }
        );
        assert_eq!(
            hub.premium_requested.as_ref().map(|s| s.file_id),
            Some(FileId(1))
        );
        assert_ne!(app.connection.status_note(), "sticker sent");
    });

    // With Premium the same sticker is just sent.
    app.update(vcx, |app, cx| {
        app.demo_session
            .as_mut()
            .unwrap()
            .payments
            .hub
            .premium_requested = None;
        app.demo_session.as_mut().unwrap().payments.premium_option = Some(true);
        app.demo_session.as_mut().unwrap().open_chat(ChatId(11));
        app.send_sticker_pick(FileId(1), "😀".into(), 512, 512, None, cx);
    });
    app.read_with(vcx, |app, _| {
        assert!(
            app.session()
                .unwrap()
                .payments
                .hub
                .premium_requested
                .is_none()
        );
        assert_eq!(app.connection.status_note(), "sticker sent");
    });
}

#[gpui_kit::test]
fn a_sent_sticker_flies_from_the_panel_to_its_message_and_lands(cx: &mut TestAppContext) {
    send_fly::reset();
    send_fly::TEST_DURATION.with(|d| d.set(Some(std::time::Duration::from_millis(1500))));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        seed(app, true);
        let session = app.demo_session.as_mut().unwrap();
        session.stickers.stickers.favorites = vec![sticker(5, false)];
        // The demo sends with a temporary id derived from this.
        session.view_generation = quill::ids::ViewGeneration(7);
    });
    let chat = app
        .read_with(vcx, |app, _| app.session().and_then(|s| s.open_chat))
        .expect("the demo opens a chat");
    open_stickers(&app, vcx);

    // The cell is chosen: what it shows is noted, then the sticker is sent.
    let image = Arc::new(gpui_kit::RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image::RgbaImage::new(2, 2)),
    ])));
    app.update(vcx, |app, cx| {
        send_fly::note_source(FileId(5), Some(ImageSource::Render(image)), (512, 512));
        app.send_sticker_pick(FileId(5), "😀".into(), 512, 512, None, cx);
    });
    assert_eq!(send_fly::flight_state(), Some(false), "armed, waiting");

    // TDLib's answer: the message with the clock's date.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i32;
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().unwrap();
        let history = session.histories.get_mut(&chat.0).unwrap();
        let mut message = history
            .messages
            .values()
            .find(|m| m.is_outgoing)
            .expect("an outgoing demo message")
            .clone();
        message.id = quill::ids::MessageId(5000);
        message.date = now;
        message.content = MessageContent::Sticker(StickerContent {
            emoji: "😀".into(),
            width: 512,
            height: 512,
            format: StickerFormat::Webp,
            file_id: FileId(5),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            is_premium: false,
            requires_premium: false,
            premium_animation: None,
            set_id: 77,
        });
        history.messages.insert(5000, message);
        session.revision += 1;
        app.notify_conversation(cx);
        cx.notify();
    });
    app.update(vcx, |app, cx| app.close_media_panel(cx));
    frames(vcx);
    assert_eq!(
        send_fly::flight_state(),
        Some(true),
        "the message's sticker was painted: the picture is on its way"
    );

    // It lands after the reveal duration and the message shows again.
    std::thread::sleep(std::time::Duration::from_millis(1600));
    frames(vcx);
    assert_eq!(send_fly::flight_state(), None);
    send_fly::reset();
}

#[gpui_kit::test]
fn no_flight_when_message_effects_are_off(cx: &mut TestAppContext) {
    send_fly::reset();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        seed(app, true);
        let session = app.demo_session.as_mut().unwrap();
        session.open_chat(ChatId(11));
        session.stickers.stickers.favorites = vec![sticker(5, false)];
    });
    open_stickers(&app, vcx);
    quill::power_saving::set(quill::power_saving::Flag::ChatEffects.bit());
    app.update(vcx, |app, cx| {
        send_fly::note_source(
            FileId(5),
            Some(ImageSource::Render(Arc::new(gpui_kit::RenderImage::new(
                smallvec::SmallVec::from_buf([image::Frame::new(image::RgbaImage::new(2, 2))]),
            )))),
            (512, 512),
        );
        app.send_sticker_pick(FileId(5), "😀".into(), 512, 512, None, cx);
    });
    quill::power_saving::set(0);
    assert_eq!(send_fly::flight_state(), None);
}

/// The system battery saver forces "Effects in messages" on: no flight, even
/// with every switch left at "plays". Ignoring the saver restores the flight.
#[gpui_kit::test]
fn battery_saver_stops_the_send_fly_unless_ignored(cx: &mut TestAppContext) {
    send_fly::reset();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        seed(app, true);
        let session = app.demo_session.as_mut().unwrap();
        session.open_chat(ChatId(11));
        session.stickers.stickers.favorites = vec![sticker(5, false)];
    });
    open_stickers(&app, vcx);
    let pick = |app: &mut QuillApp, cx: &mut gpui_kit::Context<QuillApp>| {
        send_fly::note_source(
            FileId(5),
            Some(ImageSource::Render(Arc::new(gpui_kit::RenderImage::new(
                smallvec::SmallVec::from_buf([image::Frame::new(image::RgbaImage::new(2, 2))]),
            )))),
            (512, 512),
        );
        app.send_sticker_pick(FileId(5), "😀".into(), 512, 512, None, cx);
    };
    quill::battery_saving::set_state(Some(true));
    quill::battery_saving::set_ignore(false);
    quill::battery_saving::apply();
    app.update(vcx, |app, cx| pick(app, cx));
    let while_saving = send_fly::flight_state();
    send_fly::reset();
    quill::battery_saving::set_ignore(true);
    quill::battery_saving::apply();
    app.update(vcx, |app, cx| pick(app, cx));
    let while_ignored = send_fly::flight_state();
    quill::battery_saving::set_state(None);
    quill::battery_saving::set_ignore(false);
    quill::battery_saving::apply();
    send_fly::reset();
    assert_eq!(while_saving, None, "the saver forces the effect off");
    assert_eq!(while_ignored, Some(false), "ignored: the flight is armed");
}

/// Recently used custom emoji, which the Emoji tab lists in its first rows.
fn seed_recent_custom_emoji(app: &mut QuillApp) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.stickers.emoji.custom_emoji_stickers = (0..8)
        .map(|ix| StickerItem {
            custom_emoji_id: Some(5000 + i64::from(ix)),
            ..sticker(1000 + ix, false)
        })
        .collect();
    session.settings.media_prefs.recent_custom_emoji_ids = (0..8).map(|ix| 5000 + ix).collect();
}

/// Custom emoji rows used to ask the list where they were while the list
/// was laying them out, which aborted the app as the panel opened.
#[gpui_kit::test]
fn the_emoji_panel_opens_with_custom_emoji_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| seed_recent_custom_emoji(app));
    app.update(vcx, |app, cx| app.open_media_panel(PanelTab::Emoji, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let panel = &app.pickers.media_panel;
        let custom_row = panel.rows.iter().position(|row| {
            matches!(row, PanelRow::Cells { cells, .. }
                if cells.iter().any(|cell| matches!(cell, PanelCell::Custom { .. })))
        });
        let custom_row = custom_row.expect("recent custom emoji are listed");
        assert!(panel.on_screen.contains(custom_row), "and drawn on screen");
        let last = panel.rows.len() - 1;
        assert!(
            !panel.on_screen.contains(last),
            "the catalog's end is below the fold"
        );
    });
}
