//! shared-media panel.

use super::app::{PaneMode, QuillApp};
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session, SharedMediaTab, SharedMediaTabStatus};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatKind;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Slice media-shared-gallery fixture: the gallery open on chat 11 with the
/// Media tab empty and the Files tab holding two injected documents — all
/// through the real `foundChatMessages` reducer path, no live Telegram.
pub(super) fn apply_ready_shared_media(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = ChatId(11);
    let tab = session.shared_media.open_for(chat_id);
    assert_eq!(tab, SharedMediaTab::Media);
    // Media tab: an empty `foundChatMessages` → the Empty state (never a
    // blank panel, never "empty" while loading).
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Media);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Media,
            generation,
        },
        Some(chat_id),
    );
    let empty = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&empty, seq, &dyn_sink) {
        session.apply(owned);
    }
    assert_eq!(
        session.shared_media.tabs[SharedMediaTab::Media.index()].status,
        SharedMediaTabStatus::Empty
    );
    // Files tab: two injected documents → the Ready state with rows.
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Files);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Files,
            generation,
        },
        Some(chat_id),
    );
    let files = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{{"id":201,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"report.pdf","mime_type":"application/pdf","document":{{"@type":"file","id":901,"size":12345,"expected_size":12345,"local":{{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false}},"remote":{{"@type":"remoteFile","id":"x"}}}}}},"caption":{{"@type":"formattedText","text":"Q3 numbers","entities":[]}}}}}},{{"id":202,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"demo-notes.txt","mime_type":"text/plain","document":{{"@type":"file","id":902,"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false}},"remote":{{"@type":"remoteFile","id":"x"}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&files, seq, &dyn_sink) {
        session.apply(owned);
    }
    assert_eq!(
        session.shared_media.tabs[SharedMediaTab::Files.index()].status,
        SharedMediaTabStatus::Ready
    );
    assert_eq!(
        session.shared_media.tabs[SharedMediaTab::Files.index()]
            .items
            .len(),
        2
    );
}

impl QuillApp {
    /// Phase B2: the "Encryption key" section of a secret chat partner's
    /// Slice media-shared-gallery: per-chat shared-media gallery panel
    /// beside the conversation (TGX profile → Media / Files / Music / Links /
    /// Voice / GIFs). Each tab fetches one `searchChatMessages` page with its
    /// `searchMessagesFilter*` filter; tabs cache their first page. Loading,
    /// empty, and failed are three distinct per-tab states — the empty state
    /// never shows while a fetch is in flight.
    pub(super) fn shared_media_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.shared_media.open {
            return None;
        }
        let chat_id = session.shared_media.chat_id?;
        let chat = session.chats.get(&chat_id.0);
        let title = chat
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Shared media".to_string());
        let is_channel = chat.is_some_and(|c| {
            matches!(
                c.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        let active_tab = session.shared_media.active_tab;
        let tab = &session.shared_media.tabs[active_tab.index()];
        let status = tab.status;
        let total_count = tab.total_count;
        // `error` stays an owned clone: the panel tree is boxed into
        // `AnyElement` (requires 'static), so borrowed `&str` children don't
        // compile here — same for the row labels below.
        let error = tab.error.clone();

        let mut panel = div()
            .id("shared-media-panel")
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().child(title))
                    .child(
                        div()
                            .id("shared-media-close")
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_color(cx.theme().muted_foreground)
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_shared_media_ui(cx);
                            })),
                    ),
            )
            .child({
                // kit Phase 3: the media/voice/links/files/music tab strip
                // as a kit underline `TabBar` — the active tab keeps its
                // "· count" suffix while the tab is ready.
                let weak = cx.weak_entity();
                let mut bar = TabBar::new(("shared-media-tabs", chat_id.0 as u64)).underline();
                for tab_option in SharedMediaTab::ALL {
                    let is_active = tab_option == active_tab;
                    let label =
                        if is_active && total_count > 0 && status == SharedMediaTabStatus::Ready {
                            format!("{} · {total_count}", tab_option.label())
                        } else {
                            tab_option.label().to_string()
                        };
                    bar = bar.child(Tab::new().label(label));
                }
                bar.selected_index(active_tab.index())
                    .on_click(move |ix, _window, cx| {
                        if let Some(tab) = SharedMediaTab::ALL.get(*ix).copied() {
                            let _ = weak.update(cx, |this, cx| {
                                this.select_shared_media_tab_ui(tab, cx);
                            });
                        }
                    })
            });

        let content: AnyElement = match status {
            SharedMediaTabStatus::Idle | SharedMediaTabStatus::Loading => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_6()
                .py_10()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!("Loading {}…", active_tab.label().to_lowercase()))
                .into_any_element(),
            SharedMediaTabStatus::Failed => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .px_6()
                .py_10()
                .child(div().text_3xl().child("⚠"))
                .child(
                    div()
                        .font_semibold()
                        .text_color(cx.theme().foreground)
                        .child(format!(
                            "Couldn't load {}.",
                            active_tab.label().to_lowercase()
                        )),
                )
                .when(!error.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(error.clone()),
                    )
                })
                .child(
                    div()
                        .id("shared-media-retry")
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(accent())
                        .child("Retry")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.retry_shared_media_ui(cx);
                        })),
                )
                .into_any_element(),
            SharedMediaTabStatus::Empty => {
                self.shared_media_empty_state(active_tab, is_channel, cx)
            }
            SharedMediaTabStatus::Ready => {
                let mut list = div()
                    .id(("shared-media-items", active_tab.index() as u64))
                    .flex_1()
                    .overflow_y_scroll()
                    .py_1();
                for item in tab.items.iter() {
                    let message_id = item.message_id;
                    list = list.child(
                        div()
                            .id(("shared-media-item", message_id.0 as u64))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .child(div().text_lg().child(item.glyph))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child(item.label.clone()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.jump_to_shared_media_item_ui(message_id, cx);
                            })),
                    );
                }
                list.into_any_element()
            }
        };
        panel = panel.child(content);
        Some(panel.into_any_element())
    }

    /// Slice media-shared-gallery: the ONE empty-state renderer for all six
    /// tabs (ponytail: parameterized per tab, not six bespoke panels).
    /// Glyph + TGX title + TGX description (`SharedMediaTab::empty_title` /
    /// `empty_hint`), in the app's existing muted-centered empty-state
    /// style ("No downloads yet." pattern).
    pub(super) fn shared_media_empty_state(
        &self,
        tab: SharedMediaTab,
        is_channel: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .py_10()
            .child(div().text_3xl().child(tab.glyph()))
            .child(
                div()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(tab.empty_title()),
            );
        for line in tab.empty_hint(is_channel).split('\n') {
            body = body.child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(line.to_string()),
            );
        }
        body.id(("shared-media-empty", tab.index() as u64))
            .into_any_element()
    }

    /// Slice media-shared-gallery: open the gallery for the open chat (live
    /// fetches the active tab; the demo fixture seeds session state
    /// directly, so the demo path only opens the panel).
    pub(super) fn open_shared_media_ui(&mut self, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        let opened = if let Some(live) = self.live.as_mut() {
            match live.driver.open_shared_media() {
                Ok(true) => {
                    self.status_note = "shared media".into();
                    true
                }
                Ok(false) => {
                    self.status_note = "select a chat to browse its shared media".into();
                    false
                }
                Err(_) => {
                    self.status_note = "could not open shared media".into();
                    false
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            match session.open_chat {
                Some(chat_id) => {
                    session.shared_media.open_for(chat_id);
                    self.status_note = "shared media".into();
                    true
                }
                None => {
                    self.status_note = "select a chat to browse its shared media".into();
                    false
                }
            }
        } else {
            false
        };
        if opened {
            cx.notify();
        }
    }

    pub(super) fn close_shared_media_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_shared_media();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.shared_media.close();
        }
        self.status_note = "shared media closed".into();
        cx.notify();
    }

    /// Slice media-shared-gallery: tab switch — fetches only unfetched tabs
    /// (live); demo tabs never fetch (fixture-seeded).
    pub(super) fn select_shared_media_tab_ui(
        &mut self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_shared_media_tab(tab).is_err() {
                self.status_note = "could not load that tab".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.shared_media.select_tab(tab);
        }
        cx.notify();
    }

    pub(super) fn retry_shared_media_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let tab = live.driver.session.shared_media.active_tab;
            if live.driver.fetch_shared_media(tab).is_err() {
                self.status_note = "could not retry loading shared media".into();
            }
        }
        cx.notify();
    }

    /// Slice media-shared-gallery: gallery row click — close the gallery and
    /// jump to the message (live only; demo rows are inert).
    pub(super) fn jump_to_shared_media_item_ui(
        &mut self,
        message_id: quill::ids::MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.jump_to_shared_media_item(message_id) {
                Ok(_) => self.status_note = "jumped to message".into(),
                Err(_) => self.status_note = "could not open that message".into(),
            }
        }
        cx.notify();
    }
}
