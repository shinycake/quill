//! `ready-agent-screenshot`: an agent takes a screenshot of the ready chat
//! list through the real driver path (the bridge, the paint filter and the
//! window's own renderer), so what an agent gets can be checked on each
//! platform. `QUILL_DEMO_SHOT_SCOPE=granted` (the default) grants the open
//! chat and one more; `everything` turns "All other chats" on.
//! `QUILL_DEMO_SHOT_OUT=<file.png>` saves the screenshot the agent got;
//! the window itself keeps showing the takeover layer the screenshot
//! leaves out. `QUILL_DEMO_SHOT_MENU=<chat id>` leaves that chat's context
//! menu open and `QUILL_DEMO_SHOT_PEEK=<chat id>` its peek preview, to see
//! them cut with the chat's row.
#![deny(clippy::indexing_slicing)]

use super::agent_state::active_account;
use super::app::QuillApp;
use super::chat_row::ChatPreviewState;
use super::menu_states::ChatMenuState;
use gpui_kit::*;
use quill::agent::grants::{self, ChatGrant, Scope};
use quill::agent::ui_driver::bridge::{self, UiCommand, UiReply};
use quill::agent::ui_driver::shot::ShotOptions;
use quill::agent::{access, activity};
use quill::ids::ChatId;
use std::time::Duration;

/// The chat id an environment variable names, if any.
fn env_chat(name: &str) -> Option<ChatId> {
    std::env::var(name).ok()?.parse().ok().map(ChatId)
}

fn demo_screenshot(app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
    // Over the second and third rows of the demo's chat list.
    let anchor = point(px(48.), px(200.));
    if let Some(chat_id) = env_chat("QUILL_DEMO_SHOT_MENU") {
        app.chat_list.menu = Some(ChatMenuState::at(chat_id, anchor));
    }
    if let Some(chat_id) = env_chat("QUILL_DEMO_SHOT_PEEK") {
        app.chat_list.preview = Some(ChatPreviewState { chat_id, anchor });
    }
    quill::fast_buttons::set_persistence(false);
    access::set_enabled(true);
    access::set_ui_driver(true);
    bridge::set_paused(false);
    let account = active_account();
    let everything =
        std::env::var("QUILL_DEMO_SHOT_SCOPE").is_ok_and(|scope| scope == "everything");
    grants::update(&account, |g| {
        if everything {
            g.all_chats = Some(Scope::Read);
        }
        for (chat_id, title) in [(11, "Demo chat A"), (13, "Demo channel")] {
            g.chats.insert(
                chat_id,
                ChatGrant {
                    scope: Some(Scope::Read),
                    title: title.to_string(),
                },
            );
        }
    });
    // The demo window lives until the process ends.
    std::mem::forget(activity::connect("Claude Code"));
    let out = std::env::var_os("QUILL_DEMO_SHOT_OUT").map(std::path::PathBuf::from);
    QuillApp::spawn_main_in(window, cx, async move |app, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        let options = ShotOptions {
            full_resolution: true,
        };
        let asked = std::time::Instant::now();
        let ticket = match bridge::post_with_shot("Claude Code", UiCommand::ReadTree, Some(options)) {
            Ok(ticket) => ticket,
            Err(error) => {
                eprintln!("quill screenshot demo: {error:?}");
                return;
            }
        };
        for _ in 0..500 {
            let _ = app.update_in(cx, |app, window, cx| {
                app.agent_driver_tick(window, cx);
            });
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
            let Some(reply) = ticket.poll() else {
                continue;
            };
            match reply {
                UiReply::Shot { tree, shot } => {
                    eprintln!(
                        "quill screenshot demo: {}x{} scale {} scope {} png {} bytes, {} nodes, answered in {} ms",
                        shot.width,
                        shot.height,
                        shot.scale,
                        shot.scope.as_str(),
                        shot.png.len(),
                        tree.nodes.len(),
                        asked.elapsed().as_millis()
                    );
                    if let Some(path) = &out
                        && let Err(error) = std::fs::write(path, &shot.png)
                    {
                        eprintln!("quill screenshot demo: {}: {error}", path.display());
                    }
                }
                other => eprintln!("quill screenshot demo: {other:?}"),
            }
            return;
        }
        eprintln!("quill screenshot demo: no answer");
    })
    .detach();
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-screenshot")
        .setup(demo_screenshot)
        .timing(2500, 3500),
];
