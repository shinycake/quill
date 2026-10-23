//! shared-media panel.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{SharedMediaTab, SharedMediaTabStatus};
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// Phase B2: the "Encryption key" section of a secret chat partner's
    /// Slice media-shared-gallery: per-chat shared-media gallery panel
    /// beside the conversation (TGX profile → Media / Files / Music / Links /
    /// Voice / GIFs). Each tab fetches one `searchChatMessages` page with its
    /// `searchMessagesFilter*` filter; tabs cache their first page. Loading,
    /// empty, and failed are three distinct per-tab states — the empty state
    /// never shows while a fetch is in flight.
    /// The gallery (or a user's playlist) is in the right column.
    pub(super) fn shared_media_panel_open(&self) -> bool {
        self.session().is_some_and(|session| {
            let media = &session.media.shared_media;
            media.playlist_user.is_some()
                || (media.open
                    && media.chat_id.is_some()
                    && media.tab_at(media.active_tab).is_some())
        })
    }

    pub(super) fn shared_media_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shared = &self.session()?.media.shared_media;
        if shared.playlist_user.is_none() && !shared.open {
            return None;
        }
        self.shared_media_panel_body(cx)
    }

    /// `shared_media_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn shared_media_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if let Some(user_id) = session.media.shared_media.playlist_user {
            return Some(self.playlist_panel(user_id, cx));
        }
        if !session.media.shared_media.open {
            return None;
        }
        let chat_id = session.media.shared_media.chat_id?;
        let chat = session.chats.get(&chat_id.0);
        let title = chat
            .map(|c| c.title.clone())
            .unwrap_or_else(|| tr(keys::PROFILE_SHARED_MEDIA).to_string());
        let is_channel = chat.is_some_and(|c| {
            matches!(
                c.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        let active_tab = session.media.shared_media.active_tab;
        let visible_tabs = session.shared_media_visible_tabs();
        let tab = session.media.shared_media.tab_at(active_tab)?;
        let status = tab.status;
        let total_count = tab.total_count;
        let searching = !tab.query.is_empty();
        // `error` stays an owned clone: the panel tree is boxed into
        // `AnyElement` (requires 'static), so borrowed `&str` children don't
        // compile here — same for the row labels below.
        let error = tab.error.clone();

        // Its width is the shell's (`shell_layout`).
        let mut panel = div()
            .id("shared-media-panel")
            .flex_1()
            .min_w_0()
            .h_full()
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
                    .child(div().flex_1())
                    .children(
                        self.shared_media_calendar_button(active_tab, cx)
                            .filter(|_| !searching),
                    )
                    .child(
                        div()
                            .id("shared-media-close")
                            .role(gpui_kit::Role::Button)
                            .aria_label(tr(keys::Q_OPS_CLOSE_SHARED_MEDIA))
                            .tab_index(0)
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
                let mut bar = TabBar::new(("shared-media-tabs", chat_id.0 as u64))
                    .underline()
                    .menu(true);
                for tab_option in &visible_tabs {
                    let is_active = *tab_option == active_tab;
                    let name = tab_option.label_in(is_channel);
                    let label = if is_active
                        && !searching
                        && total_count > 0
                        && status == SharedMediaTabStatus::Ready
                    {
                        format!("{name} · {total_count}")
                    } else {
                        name.to_string()
                    };
                    bar = bar.child(Tab::new().label(label));
                }
                let selected = visible_tabs
                    .iter()
                    .position(|tab| *tab == active_tab)
                    .unwrap_or(0);
                bar.selected_index(selected)
                    .on_click(move |ix, _window, cx| {
                        if let Some(tab) = visible_tabs.get(*ix).copied() {
                            let _ = weak.update(cx, |this, cx| {
                                this.select_shared_media_tab_ui(tab, cx);
                            });
                        }
                    })
            })
            .children(self.shared_media_search_row(active_tab, cx));

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
                .child(tr_with(
                    keys::Q_OPS_LOADING_2,
                    &[(
                        "to_lowercase",
                        &(active_tab.label_in(is_channel).to_lowercase()).to_string(),
                    )],
                ))
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
                        .child(tr_with(
                            keys::Q_LEFTOVERS_COULD_NOT_LOAD_MEDIA,
                            &[(
                                "tab",
                                &(active_tab.label_in(is_channel).to_lowercase()).to_string(),
                            )],
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
                        .role(gpui_kit::Role::Button)
                        .aria_label(tr(keys::Q_OPS_RETRY_LOADING_SHARED_MEDIA))
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(accent())
                        .child(tr(keys::Q_OPS_RETRY))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.retry_shared_media_ui(cx);
                        })),
                )
                .into_any_element(),
            SharedMediaTabStatus::Empty if searching => {
                self.shared_media_search_empty_state(active_tab, cx)
            }
            SharedMediaTabStatus::Empty => {
                self.shared_media_empty_state(active_tab, is_channel, cx)
            }
            SharedMediaTabStatus::Ready
                if matches!(active_tab, SharedMediaTab::Stories | SharedMediaTab::Gifts) =>
            {
                self.shared_media_grid(active_tab, cx)
            }
            SharedMediaTabStatus::Ready
                if matches!(
                    active_tab,
                    SharedMediaTab::Media
                        | SharedMediaTab::Photos
                        | SharedMediaTab::Videos
                        | SharedMediaTab::Gifs
                ) =>
            {
                self.shared_media_tile_grid(active_tab, cx)
            }
            SharedMediaTabStatus::Ready => self.shared_media_ready_list(active_tab, cx),
        };
        if let Some(bar) = self.shared_media_selection_bar(active_tab, cx) {
            panel = panel.child(bar);
        }
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
        let split = self.chat_prefs.profile_media_tabs_expanded;
        if let Some(live) = self.live.as_mut() {
            let gallery = &mut live.driver.session.media.shared_media;
            if !gallery.open {
                gallery.media_split = split;
            }
        }
        let opened = if let Some(live) = self.live.as_mut() {
            match live.driver.open_shared_media() {
                Ok(true) => {
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_SHARED_MEDIA)));
                    true
                }
                Ok(false) => {
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_SELECT_A_CHAT_TO_BROWSE)));
                    false
                }
                Err(_) => {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_OPS_COULD_NOT_OPEN_SHARED_MEDIA,
                    )));
                    false
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            match session.open_chat {
                Some(chat_id) => {
                    session.media.shared_media.open_for(chat_id);
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_SHARED_MEDIA)));
                    true
                }
                None => {
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_OPS_SELECT_A_CHAT_TO_BROWSE)));
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
            session.media.shared_media.close();
        }
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_OPS_SHARED_MEDIA_CLOSED)));
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
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_LOAD_THAT_TAB)));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.media.shared_media.select_tab_dropping_search(tab);
        }
        cx.notify();
    }

    pub(super) fn retry_shared_media_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let tab = live.driver.session.media.shared_media.active_tab;
            if live.driver.fetch_shared_media(tab).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_RETRY_LOADING_SHARED,
                )));
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
                Ok(_) => self
                    .connection
                    .set_note(StatusNote::silent(tr(keys::Q_OPS_JUMPED_TO_MESSAGE))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_OPEN_THAT_MESSAGE,
                ))),
            }
        }
        cx.notify();
    }
}
