//! The emoji set box in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): switching to an installed set draws emoji
//! from its sprites, switching back returns to the system font. No
//! network: the pack is a tiny fixture written into the app data root.
use crate::ui::app::QuillApp;
use crate::ui::emoji_sprites;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::emoji_sets_style::catalog::{EmojiSetId, load_current, set_dir};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = std::env::temp_dir().join(format!("quill-emoji-style-ui-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    write_fixture_pack();
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// A one-sheet "Noto" pack: 😀 in cell 0 (red), 👍🏽 in cell 1 (blue).
fn write_fixture_pack() {
    let root = quill::settings::safe_app_root().unwrap();
    let dir = set_dir(&root, EmojiSetId::Noto);
    std::fs::create_dir_all(&dir).unwrap();
    let config = r#"{"version":1,"cell":16,"columns":2,"rows":1,"sheets":1,"index":{"😀":[0,0],"👍🏽":[0,1]}}"#;
    std::fs::write(dir.join("config.json"), config).unwrap();
    let mut sheet = image::RgbaImage::new(32, 16);
    for (x, _, pixel) in sheet.enumerate_pixels_mut() {
        *pixel = if x < 16 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        };
    }
    let mut bytes = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
        .encode(&sheet, 32, 16, image::ExtendedColorType::Rgba8)
        .unwrap();
    std::fs::write(dir.join("emoji_0.webp"), bytes).unwrap();
}

#[gpui_kit::test]
fn switching_sets_draws_emoji_from_sprites(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    app.update(&mut vcx, |app, cx| app.open_emoji_set_style(cx));
    vcx.run_until_parked();
    assert_eq!(emoji_sprites::active_set(), EmojiSetId::System);
    assert!(emoji_sprites::pieces("hi 😀").is_none());

    app.update(&mut vcx, |app, cx| {
        app.choose_emoji_set(EmojiSetId::Noto, cx)
    });
    vcx.run_until_parked();
    assert_eq!(emoji_sprites::active_set(), EmojiSetId::Noto);
    let root = quill::settings::safe_app_root().unwrap();
    assert_eq!(load_current(&root), EmojiSetId::Noto);
    // The first ask starts decoding the sheet; once in, the emoji is a
    // picture and the text around it stays text.
    let _ = emoji_sprites::pieces("hi 😀 and 👍🏽!");
    vcx.run_until_parked();
    let pieces = emoji_sprites::pieces("hi 😀 and 👍🏽!").expect("sprites");
    let kinds: Vec<bool> = pieces.iter().map(|p| p.image.is_some()).collect();
    assert_eq!(kinds, [false, true, false, true, false]);
    // Emoji the pack lacks keep the system font.
    assert!(emoji_sprites::pieces("🙂 only").is_none());

    app.update(&mut vcx, |app, cx| {
        app.choose_emoji_set(EmojiSetId::System, cx)
    });
    vcx.run_until_parked();
    assert_eq!(emoji_sprites::active_set(), EmojiSetId::System);
    assert_eq!(load_current(&root), EmojiSetId::System);
    assert!(emoji_sprites::pieces("hi 😀").is_none());
}
