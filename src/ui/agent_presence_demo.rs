//! `ready-agent-connected`: the connected strip over the ready chat list.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::access;

/// `ready-agent-connected`: the strip with one agent connected, or two with
/// `QUILL_DEMO_AGENT=two`.
fn demo_connected(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    access::set_enabled(true);
    let two = std::env::var("QUILL_DEMO_AGENT").is_ok_and(|mode| mode == "two");
    let mut names = vec!["Claude Code"];
    if two {
        names.push("Cursor");
    }
    let guards: Vec<_> = names
        .into_iter()
        .map(quill::agent::activity::connect)
        .collect();
    // The demo window lives until the process ends.
    std::mem::forget(guards);
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-connected").setup(demo_connected),
];
