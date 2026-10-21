//! The message menu keeps tdesktop's order for a message with the most media
//! actions: a song with a caption (media actions, Copy Text, Translate), read
//! from the accessibility tree over the `ready-audio` demo (gpui-kit's test
//! support needs the `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::MessageMenuState;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::MessageContent;

const CHAT: ChatId = ChatId(11);
const SONG: MessageId = MessageId(801);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-audio")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot
        .borrow()
        .clone()
        .expect("the window builder fills the slot");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    (app, vcx)
}

/// Menu rows top to bottom, as the last paint drew them.
fn labels_on_screen(vcx: &mut VisualTestContext) -> Vec<String> {
    let mut rows: Vec<(f32, String)> = vcx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|row| {
                row.role() == Some(gpui_kit::Role::MenuItem)
                    && row.visible()
                    && row.focused().is_some()
            })
            .map(|row| {
                (
                    f32::from(row.bounds().origin.y),
                    row.label().unwrap_or_default().to_string(),
                )
            })
            .collect()
    });
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, label)| label).collect()
}

/// A song in the open chat gets a caption and the menu opens on it.
#[gpui_kit::test]
fn a_song_menu_lists_media_actions_before_copy_text_and_translate(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(CHAT);
            // The server's tone limits admit the 214 s demo song.
            session.messages.tone_limits = quill::message_menu::ToneLimits {
                max_size: i64::MAX,
                max_duration: 600,
                max_count: 10,
            };
            if let Some(message) = session
                .histories
                .get_mut(&CHAT.0)
                .and_then(|history| history.messages.get_mut(&SONG.0))
                && let MessageContent::Audio(audio) = &mut message.content
            {
                audio.caption = "Recorded on the night drive home".into();
            }
        }
        app.message_ui.menu = Some(MessageMenuState {
            chat_id: CHAT,
            message_id: SONG,
            position: point(px(500.), px(120.)),
        });
        cx.notify();
    });
    vcx.run_until_parked();
    let labels = labels_on_screen(vcx);
    let folder = if cfg!(target_os = "macos") {
        "Show in Finder"
    } else {
        "Show in Folder"
    };
    let expected = [
        folder,
        "Save for Notifications",
        "Save to...",
        "Copy Filename",
        "Copy Text",
        "Translate",
    ];
    let found: Vec<&str> = labels
        .iter()
        .map(String::as_str)
        .filter(|label| expected.contains(label))
        .collect();
    assert_eq!(
        found, expected,
        "media rows in tdesktop order, then Copy Text, then Translate (all rows: {labels:?})"
    );
}
