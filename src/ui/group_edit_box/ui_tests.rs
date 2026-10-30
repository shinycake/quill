//! The Edit box: it opens filled from the chat, an empty name blocks
//! Save, Save closes it, Escape discards, a copied image opens the crop,
//! and the name's emoji toggle inserts at the cursor.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::photo_editor_profile::clipboard_image;
use crate::ui::profile_photo_sources::PhotoTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, ElementId, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(
    cx: &mut TestAppContext,
    name: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(name);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn title_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| {
        app.group_edit_box()
            .map(|edit| edit.title_input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

fn type_title(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(edit) = app.group_edit_box() {
                edit.title_input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            }
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_box_opens_filled_with_its_rows(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
    assert_eq!(title_text(&app, vcx), "Demo supergroup");
    let description = app.read_with(vcx, |app, cx| {
        app.group_edit_box()
            .map(|edit| edit.description_input.read(cx).value().to_string())
            .unwrap_or_default()
    });
    assert!(description.starts_with("Weekend hikes"));
    // Kit components register for lookup; the manage rows are plain
    // rows and show in the screenshots instead.
    for id in ["group-edit-photo", "group-edit-save", "group-edit-cancel"] {
        assert!(
            vcx.update(|window, _| window.try_find(id).is_some()),
            "{id}"
        );
    }
}

#[gpui_kit::test]
fn an_empty_name_keeps_the_box_open(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "   ");
    vcx.update(|window, cx| window.click("group-edit-save", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_some()));
    assert!(!app.read_with(vcx, |app, _| {
        app.connection.status_note().contains("Demo mode")
    }));
}

#[gpui_kit::test]
fn save_sends_and_closes(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-channel-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "Quill Builds");
    vcx.update(|window, cx| window.click("group-edit-save", cx));
    frames(vcx);
    // The demo has no live session: the save reaches the send step.
    assert!(app.read_with(vcx, |app, _| {
        app.connection.status_note().contains("Demo mode")
    }));
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::GroupEditBox)));
}

#[gpui_kit::test]
fn escape_discards_the_edits(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-basic-group-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "Unsaved");
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::GroupEditBox)));
}

#[gpui_kit::test]
fn the_crop_shows_over_the_box_and_hands_it_back(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-photo-crop");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog
        == Some(DialogKind::ProfilePhotoCrop)));
    vcx.update(|window, cx| window.click("photo-editor-done", cx));
    frames(vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
    assert!(app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .is_some_and(|edit| edit.photo.is_some())
    }));
}

fn copy_png(vcx: &mut VisualTestContext, width: u32, height: u32) {
    let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([40, 120, 200, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("png");
    let image = gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes.into_inner());
    vcx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&image));
}

fn paste(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let target = PhotoTarget::Chat(ChatId(61));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.paste_photo_from_clipboard(target, window, cx)
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_copied_image_opens_the_crop(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    // Text on the clipboard: the menu leaves "From clipboard" out.
    vcx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("hikes".into()));
    assert!(vcx.update(|_, cx| clipboard_image(cx).is_none()));
    copy_png(vcx, 400, 300);
    assert!(vcx.update(|_, cx| clipboard_image(cx).is_some()));
    paste(&app, vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog
        == Some(DialogKind::ProfilePhotoCrop)));
    vcx.update(|window, cx| window.click("photo-editor-done", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .is_some_and(|edit| edit.photo.is_some())
    }));
}

#[gpui_kit::test]
fn a_too_long_copied_image_is_refused(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    copy_png(vcx, 2000, 100);
    paste(&app, vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
    assert!(app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .is_some_and(|edit| edit.photo_note.is_some())
    }));
}

#[gpui_kit::test]
fn the_emoji_toggle_inserts_at_the_cursor(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "Hikers");
    vcx.update(|window, cx| window.click("group-edit-title-emoji", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_title_emoji_active()));
    let routed = vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.route_group_title_emoji("\u{26F0}", window, cx)
        })
    });
    assert!(routed);
    frames(vcx);
    assert_eq!(title_text(&app, vcx), "Hikers\u{26F0}");
    // The toggle closes it; a pick then goes back to the composer.
    vcx.update(|window, cx| window.click("group-edit-title-emoji", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.group_title_emoji_active()));
    assert!(!vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.route_group_title_emoji("x", window, cx))
    }));
}

#[gpui_kit::test]
fn escape_and_cancel_close_the_emoji_panel(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.click("group-edit-title-emoji", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.pickers.media_panel.open));
    // Escape closes the topmost surface: the panel, then the box.
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.pickers.media_panel.open));
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_some()));
    vcx.update(|window, cx| window.click("group-edit-title-emoji", cx));
    frames(vcx);
    // A click off the panel (on the photo above it) lands on its cover: the
    // panel closes and the box stays.
    vcx.update(|window, cx| window.click("group-edit-photo", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.pickers.media_panel.open));
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_some()));
    vcx.update(|window, cx| window.click("group-edit-title-emoji", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_title_emoji_active()));
    // Closing the box closes its panel.
    vcx.update(|_, cx| app.update(cx, |app, cx| app.close_group_edit_box(cx)));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_none()));
}

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find((id, 1u64)).is_some())
}

#[gpui_kit::test]
fn author_profiles_show_only_while_signing(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-channel-edit-box");
    let vcx = &mut vcx;
    assert!(shown(vcx, "g2-toggle-sign-messages"));
    assert!(shown(vcx, "g2-toggle-show-authors"));
    vcx.update(|window, cx| window.click(("g2-toggle-sign-messages", 1u64), cx));
    frames(vcx);
    assert!(!shown(vcx, "g2-toggle-show-authors"));
    let signatures = app.read_with(vcx, |app, _| {
        app.group_edit_box().and_then(|edit| edit.signatures)
    });
    assert_eq!(
        signatures.map(|s| (s.sign, s.profiles)),
        Some((false, false))
    );
    vcx.update(|window, cx| window.click(("g2-toggle-sign-messages", 1u64), cx));
    frames(vcx);
    assert!(shown(vcx, "g2-toggle-show-authors"));
}

#[gpui_kit::test]
fn locked_auto_translate_asks_for_boosts(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-channel-edit-box");
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.click(("tr-toggle-auto-translate", 1u64), cx));
    frames(vcx);
    let (choice, ask) = app.read_with(vcx, |app, _| {
        (
            app.group_edit_box().and_then(|edit| edit.auto_translate),
            app.earn.ask_boost,
        )
    });
    assert_eq!(choice, None);
    let ask = ask.expect("the boost box opens");
    assert_eq!((ask.chat_id, ask.level), (62, 3));
}

#[gpui_kit::test]
fn the_history_box_hands_its_choice_to_save(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            if let Some(edit) = app.group_edit_box_mut() {
                edit.history_box = Some(false);
            }
            cx.notify();
        })
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog
        == Some(DialogKind::GroupHistoryBox)));
    vcx.update(|window, cx| {
        window
            .within("group-history-visibility")
            .click(ElementId::Integer(0), cx)
    });
    vcx.update(|window, cx| window.click("group-history-save", cx));
    frames(vcx);
    let edit = app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .map(|edit| (edit.history_box, edit.history_visible))
    });
    assert_eq!(edit, Some((None, Some(true))));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
}

/// The Manage rows read the chat's member lists, which go with the chat's
/// history once it is pruned: the open box holds the chat.
#[gpui_kit::test]
fn the_open_box_holds_its_chat_from_pruning(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    let held = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .map(|session| {
                    session
                        .history_pruning
                        .held_chats()
                        .collect::<Vec<ChatId>>()
                })
                .unwrap_or_default()
        })
    };
    let chat = app.read_with(vcx, |app, _| app.group_edit_box().map(|edit| edit.chat_id));
    let chat = chat.expect("the box is open");
    assert_eq!(held(&app, vcx), vec![chat]);
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(held(&app, vcx).is_empty(), "closing the box lets it go");
}
