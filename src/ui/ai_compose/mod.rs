//! The AI compose box (tdesktop boxes/compose_ai_box.cpp) and the
//! composer's AI button (ui/controls/compose_ai_button_factory.cpp). The
//! box's logic is `quill::ai_compose::AiComposeBox`; this module draws it,
//! sends what its transitions return and feeds the answers back.
#![deny(clippy::indexing_slicing)]

mod about;
mod apply_in_place;
mod button;
mod card;
mod demo;
mod dialog;
mod languages;
mod style_demo;
mod style_editor;
mod style_icon;
mod style_menu;
mod style_notices;
mod style_preview;
mod upkeep;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "dialog_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "style_tests.rs"]
mod style_tests;

use super::ai_compose_state::{AiComposeOpen, AiComposeView};
use super::app::QuillApp;
use super::status_note::StatusNote;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::ButtonCustomVariant;
use gpui_kit::*;
use quill::ai_compose::{AiBoxError, AiComposeBox, AiSend};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::Session;
use quill::telegram::envelope::ChatKind;
use std::ops::Range;

/// `template` with `{tag}` replaced by `link`: the text without its
/// `**` marks, the bold ranges and the link's range (tdesktop marked text
/// with one `tr::link`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarkedLink {
    pub(super) text: String,
    pub(super) bold: Vec<Range<usize>>,
    pub(super) link: Option<Range<usize>>,
}

pub(super) fn marked_link(template: &str, tag: &str, link: &str) -> MarkedLink {
    let placeholder = format!("{{{tag}}}");
    let (before, after, found) = match template.split_once(&placeholder) {
        Some((before, after)) => (before, after, true),
        None => (template, "", false),
    };
    let (mut text, mut bold) = super::screen_chooser::split_bold(before);
    let link = found.then(|| {
        let start = text.len();
        text.push_str(link);
        start..text.len()
    });
    let (rest, rest_bold) = super::screen_chooser::split_bold(after);
    let offset = text.len();
    text.push_str(&rest);
    bold.extend(
        rest_bold
            .into_iter()
            .map(|range| range.start + offset..range.end + offset),
    );
    MarkedLink { text, bold, link }
}

fn pill_radius() -> Pixels {
    px(999.)
}

/// tdesktop `aiToneDeleteButton`: the attention colour on the box
/// background (Delete Style, Remove Style).
fn attention_button(cx: &App) -> ButtonCustomVariant {
    let danger = cx.theme().danger;
    ButtonCustomVariant::new(cx)
        .color(transparent_black())
        .foreground(danger)
        .hover(danger.opacity(0.1))
        .active(danger.opacity(0.16))
}

fn session_mut(app: &mut QuillApp) -> Option<&mut Session> {
    match app.live.as_mut() {
        Some(live) => Some(&mut live.driver.session),
        None => app.demo_session.as_deref_mut(),
    }
}

impl QuillApp {
    /// The composer's AI button click (tdesktop
    /// `HistoryWidget::showAiComposeBox`): a box over the draft, on the
    /// Style tab, which asks for nothing until a style is picked.
    pub(super) fn open_ai_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_ui.rich_editor_open {
            return;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        let secret = self.session().is_some_and(|s| {
            s.chats
                .get(&chat_id.0)
                .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
        });
        if secret {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_CHAT_AI_TOOLS_UNAVAILABLE_HERE)));
            cx.notify();
            return;
        }
        quill::ai_compose::prefs::update(|prefs| prefs.tooltip_hidden = true);
        let markup = self.composer_markup(cx);
        let plain = quill::composer::parse_format_markup(&markup).0;
        let mut ai = AiComposeBox::new(
            markup,
            quill::translate::detect_language(&plain),
            &self.translate_ui_language(),
        );
        let send = ai.start();
        self.ai_compose.open = Some(AiComposeOpen {
            chat_id,
            ai,
            view: AiComposeView::Card,
            style_names: self.ai_compose_style_names(),
            editor: None,
            select_saved: None,
        });
        self.ai_compose_dispatch(send, window, cx);
    }

    pub(super) fn close_ai_compose(&mut self, cx: &mut Context<Self>) {
        self.ai_compose.open = None;
        cx.notify();
    }

    pub(super) fn ai_compose_update(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut AiComposeBox) -> Option<AiSend>,
    ) {
        let Some(open) = self.ai_compose.open.as_mut() else {
            return;
        };
        let send = change(&mut open.ai);
        self.ai_compose_dispatch(send, window, cx);
    }

    pub(super) fn set_ai_compose_view(&mut self, view: AiComposeView, cx: &mut Context<Self>) {
        if let Some(open) = self.ai_compose.open.as_mut() {
            open.view = view;
        }
        cx.notify();
    }

    fn ai_compose_dispatch(
        &mut self,
        send: Option<AiSend>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
        let (Some(send), Some(open)) = (send, self.ai_compose.open.as_ref()) else {
            return;
        };
        let chat_id: ChatId = open.chat_id;
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if live
            .driver
            .ai_box_request(chat_id, &send.request, send.token)
            .is_ok()
        {
            return;
        }
        let error = self
            .ai_compose
            .open
            .as_mut()
            .and_then(|open| open.ai.fail(send.token, AiBoxError::Other(String::new())));
        if let Some(error) = error {
            self.ai_compose_error_toast(&error, window, cx);
        }
    }

    fn ai_compose_style_names(&self) -> Vec<String> {
        self.session()
            .map(|s| {
                s.messages
                    .ai_compose
                    .styles
                    .names()
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The result in the composer (tdesktop `applyAndClose`), then close.
    pub(super) fn apply_ai_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.ai_compose.open.as_ref() else {
            return;
        };
        let Some(result) = open.ai.result() else {
            return;
        };
        let markup = quill::composer::entities_to_markup(&result.text, &result.entities);
        let chat_id = open.chat_id;
        self.ai_compose.open = None;
        if self.session().and_then(|s| s.open_chat) == Some(chat_id) {
            self.set_composer_markup(&markup, window, cx);
        }
        cx.notify();
    }

    /// Apply, then send through the composer's send path (tdesktop
    /// `sendResult`).
    pub(super) fn send_ai_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat_id = self.ai_compose.open.as_ref().map(|open| open.chat_id);
        self.apply_ai_compose(window, cx);
        if chat_id.is_some() && self.session().and_then(|s| s.open_chat) == chat_id {
            self.submit_from_send_button(window, cx);
        }
    }

    /// Whether the box offers Send next to Apply (tdesktop
    /// `HistoryWidget::canSendAiComposeDirect`): the send button would
    /// send now, with no slow-mode wait and no Stars charge.
    pub(super) fn ai_compose_can_send(&self) -> bool {
        let Some(chat_id) = self.ai_compose.open.as_ref().map(|open| open.chat_id) else {
            return false;
        };
        let free = self.session().is_some_and(|s| {
            s.open_chat == Some(chat_id)
                && s.send_gate(chat_id) == quill::paid_messages::SendGate::Free
        });
        free && self.send_button_kind_now(false, true) == quill::send_button::SendButtonKind::Send
    }
}

#[cfg(test)]
mod marked_tests {
    use super::{MarkedLink, marked_link};

    #[test]
    fn link_replaces_its_tag_between_bold_runs() {
        let marked = marked_link(
            "Get {link} for **50x** more AI text transformations per day.",
            "link",
            "Telegram Premium",
        );
        assert_eq!(
            marked.text,
            "Get Telegram Premium for 50x more AI text transformations per day."
        );
        assert_eq!(marked.link, Some(4..20));
        assert_eq!(marked.bold, vec![25..28]);
    }

    #[test]
    fn bold_before_the_link_keeps_its_range() {
        let marked = marked_link("**Style** by {user}", "user", "Maya");
        assert_eq!(
            marked,
            MarkedLink {
                text: "Style by Maya".into(),
                bold: std::iter::once(0..5).collect(),
                link: Some(9..13),
            }
        );
    }

    #[test]
    fn a_template_without_the_tag_has_no_link() {
        let marked = marked_link("**Co**nfidential", "link", "x");
        assert_eq!(marked.text, "Confidential");
        assert_eq!(marked.link, None);
        assert_eq!(marked.bold, vec![0..2]);
    }
}
