//! Screenshot demo: the gallery's Files tab answering a search.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::{
    SharedMediaAction, SharedMediaItem, SharedMediaTab, SharedMediaTabStatus, SharedMediaVisual,
};

register_demos![
    // The Files tab answering a search. `QUILL_DEMO_SHARED_MEDIA_QUERY`
    // picks the text (default "report"); one nothing matches shows the
    // empty state.
    DemoSpec::chats(
        "ready-shared-media-search",
        "screenshot demo — shared media search (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_shared_media_search),
];

/// Files of the demo gallery: name and unix date.
const DEMO_FILES: [(&str, i32); 7] = [
    ("Q3 report.pdf", 1_760_300_000),
    ("Annual report draft.docx", 1_759_000_000),
    ("Budget.xlsx", 1_758_500_000),
    ("report-final.xlsx", 1_757_100_000),
    ("Trip itinerary.pdf", 1_756_000_000),
    ("Design review notes.md", 1_755_000_000),
    ("Reportage scans.zip", 1_752_000_000),
];

impl QuillApp {
    pub(in crate::ui) fn demo_shared_media_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query =
            std::env::var("QUILL_DEMO_SHARED_MEDIA_QUERY").unwrap_or_else(|_| "report".to_string());
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        session
            .media
            .chat_media_counts
            .entry(chat_id.0)
            .or_default()
            .insert(2, 24);
        let gallery = &mut session.media.shared_media;
        gallery.open_for_tab(chat_id, SharedMediaTab::Files);
        let needle = query.to_lowercase();
        let items: Vec<SharedMediaItem> = DEMO_FILES
            .iter()
            .zip(1_i32..)
            .filter(|((name, _), _)| name.to_lowercase().contains(&needle))
            .map(|((name, date), id)| SharedMediaItem {
                message_id: MessageId(i64::from(id)),
                glyph: SharedMediaTab::Files.glyph(),
                label: (*name).to_string(),
                message: None,
                date: *date,
                action: SharedMediaAction::Message,
                visual: SharedMediaVisual::None,
                meta: Default::default(),
            })
            .collect();
        if let Some(state) = gallery.tabs.get_mut(SharedMediaTab::Files.index()) {
            state.status = if items.is_empty() {
                SharedMediaTabStatus::Empty
            } else {
                SharedMediaTabStatus::Ready
            };
            state.total_count = items.len() as i32;
            state.items = items;
            state.query = query.clone();
        }
        self.media_search.key = Some((chat_id, SharedMediaTab::Files));
        self.media_search
            .input
            .update(cx, |input, cx| input.set_value(query, window, cx));
        cx.notify();
    }
}
