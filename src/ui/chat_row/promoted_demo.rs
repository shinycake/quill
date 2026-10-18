//! Screenshot demo of promoted chats in the chat list: an announcement
//! with the server's badge text, and a proxy sponsor.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::lang::PackValue;
use quill::promotion::Promotion;

register_demos![
    DemoSpec::chats(
        "ready-promoted-chats",
        "screenshot demo — chat list: promoted chats"
    )
    .setup(QuillApp::demo_promoted_chats),
];

impl QuillApp {
    fn demo_promoted_chats(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let mut ids: Vec<i64> = session.chats.keys().copied().collect();
        ids.sort_unstable();
        ids.reverse();
        let top = session.chats.values().map(|c| c.order).max().unwrap_or(0);
        let psa = Promotion::Psa {
            kind: "covid".into(),
        };
        let promoted = [(ids.first(), psa), (ids.get(1), Promotion::MtprotoProxy)];
        for (offset, (id, promotion)) in (1..).zip(promoted) {
            let Some(id) = id else { continue };
            if let Some(chat) = session.chats.get_mut(id) {
                chat.order = top + offset;
                chat.is_pinned = false;
            }
            session.chat_list.promoted_chats.insert(*id, promotion);
        }
        session.chat_list.psa_labels.store(vec![(
            "cloud_lng_badge_psa_covid".into(),
            PackValue::Ordinary("COVID-19 Info".into()),
        )]);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — promoted chats"));
    }
}
