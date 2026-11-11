//! Screenshot demo: message widgets from widget-sandbox's compatibility
//! corpus running in a chat (docs/decisions/message-widgets.md).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;
use std::sync::atomic::Ordering;

register_demos![
    // Tabs, a canvas bar chart and a theme-following card, in one chat.
    DemoSpec::chats("ready-widgets", "screenshot demo — message widgets")
        .setup(QuillApp::demo_ready_widgets),
    // Widgets as bots send them: an animated game, a weather card, a
    // product card and a widget without CSS under its caption.
    DemoSpec::chats(
        "ready-widgets-showcase",
        "screenshot demo — widget showcase"
    )
    .setup(QuillApp::demo_widgets_showcase),
];

/// The demo's widgets: (message id, outgoing, caption, html).
const WIDGETS: [(i64, bool, &str, &str); 3] = [
    (
        120,
        false,
        "Tabs",
        include_str!("../../../third_party/widgets/sandbox/corpus/compat/tabs.html"),
    ),
    (
        121,
        true,
        "Weekly numbers",
        include_str!("../../../third_party/widgets/sandbox/corpus/compat/canvas-bar-chart.html"),
    ),
    (
        122,
        false,
        "",
        include_str!("../../../third_party/widgets/sandbox/corpus/compat/dark-mode.html"),
    ),
];

/// The showcase's widgets, as [`WIDGETS`].
const SHOWCASE: [(i64, bool, &str, &str); 4] = [
    (130, false, "", include_str!("widgets/game.html")),
    (131, false, "", include_str!("widgets/weather.html")),
    (132, false, "", include_str!("widgets/product.html")),
    (
        133,
        true,
        "Your order is on its way",
        include_str!("widgets/order.html"),
    ),
];

impl QuillApp {
    fn demo_ready_widgets(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_widgets(&WIDGETS, "screenshot demo — message widgets");
    }

    fn demo_widgets_showcase(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_widgets(&SHOWCASE, "screenshot demo — widget showcase");
    }

    fn demo_widgets(&mut self, widgets: &[(i64, bool, &str, &str)], note: &'static str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        for &(id, outgoing, caption, html) in widgets {
            let json = serde_json::json!({
                "@type": "updateNewMessage",
                "message": {
                    "id": id, "chat_id": 11, "is_outgoing": outgoing, "date": 1_700_000_000 + id,
                    "content": {
                        "@type": "messageWidget", "width": 0, "height": 0, "html": html,
                        "public_data": "", "chat_data": "", "widget_id": "0",
                        "caption": {"@type": "formattedText", "text": caption, "entities": []},
                        "show_caption_above_media": false, "is_copy": false,
                        "can_send_queries": false, "can_send_background_queries": false,
                    },
                },
            })
            .to_string();
            if let Some(message) =
                quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &sink)
            {
                session.apply(message);
            }
        }
        session.open_chat(ChatId(11));
        self.set_note(StatusNote::silent(note));
    }
}
