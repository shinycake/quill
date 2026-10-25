//! Per-frame upkeep for the AI compose box: the composer button's blink,
//! the box's answers, a changed style list and the style pills' custom
//! emoji, plus the error toasts (tdesktop `ComposeAiContent::showError`).

use super::{marked_link, session_mut};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ai_compose::AiBoxError;
use quill::ids::FileId;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(in crate::ui) fn ai_compose_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_ai_compose_button(cx);
        let answer = session_mut(self).and_then(|s| s.messages.ai_compose.box_answer.take());
        if let Some(answer) = answer {
            let error = self
                .ai_compose
                .open
                .as_mut()
                .and_then(|open| match answer.outcome {
                    Ok(result) => open.ai.accept(answer.token, result),
                    Err(error) => open.ai.fail(answer.token, error),
                });
            if let Some(error) = error {
                self.ai_compose_error_toast(&error, window, cx);
            }
            cx.notify();
        }
        self.refresh_ai_compose_styles(window, cx);
        self.fetch_ai_compose_emoji();
    }

    /// A style list that changed under the open box (tdesktop
    /// `refreshTones`) drops a picked style the server no longer lists.
    fn refresh_ai_compose_styles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let names = self.ai_compose_style_names();
        let Some(open) = self.ai_compose.open.as_mut() else {
            return;
        };
        if open.style_names == names {
            return;
        }
        open.style_names = names;
        let styles = self
            .session()
            .map(|s| s.messages.ai_compose.styles.clone())
            .unwrap_or_default();
        self.ai_compose_update(window, cx, |ai| ai.refresh_styles(&styles));
    }

    /// The style pills' emoji: resolve each custom emoji once, then fetch
    /// its still.
    fn fetch_ai_compose_emoji(&mut self) {
        if self.ai_compose.open.is_none() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let mut unresolved = Vec::new();
        let mut files: Vec<FileId> = Vec::new();
        for style in session.messages.ai_compose.styles.styles() {
            let id = style.custom_emoji_id;
            if id <= 0 {
                continue;
            }
            let sticker = session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(id));
            match sticker {
                Some(item) if self.panel_still(item).is_none() => {
                    files.extend([item.display_file_id(), item.thumb_file_id].into_iter().flatten());
                }
                Some(_) => {}
                None if !self.ai_compose.emoji_asked.contains(&id) => unresolved.push(id),
                None => {}
            }
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let _ = live.driver.ensure_media_files(&files);
        for id in unresolved {
            match live.driver.resolve_custom_emoji(id) {
                Ok(None) => break,
                Ok(Some(_)) | Err(_) => {
                    self.ai_compose.emoji_asked.insert(id);
                }
            }
        }
    }

    pub(super) fn ai_compose_error_toast(
        &mut self,
        error: &AiBoxError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let toast = match error {
            AiBoxError::FloodPremium => self.ai_compose_flood_toast(cx),
            AiBoxError::TooLong => {
                Notification::new().message(tr(keys::AI_COMPOSE_ERROR_TOO_LONG))
            }
            AiBoxError::StyleInvalid(kind) | AiBoxError::Other(kind) if !kind.is_empty() => {
                Notification::new().message(kind.clone())
            }
            AiBoxError::StyleInvalid(_) | AiBoxError::Other(_) => {
                Notification::new().message(tr(keys::AI_COMPOSE_ERROR))
            }
        };
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(toast, cx);
        });
    }

    /// tdesktop `ShowPremiumPromoToast`: no title, a star, the text with
    /// its "Telegram Premium" link opening Premium.
    fn ai_compose_flood_toast(&self, cx: &mut Context<Self>) -> Notification {
        let marked = marked_link(
            tr(keys::AI_COMPOSE_FLOOD_TEXT),
            "link",
            tr(keys::AI_COMPOSE_FLOOD_LINK),
        );
        let app = cx.entity().downgrade();
        Notification::new()
            .icon(IconName::Star)
            .content(move |_, _, cx| {
                let bold = HighlightStyle {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                };
                let link = HighlightStyle {
                    color: Some(cx.theme().primary),
                    ..bold
                };
                let mut highlights: Vec<_> = marked
                    .bold
                    .iter()
                    .map(|range| (range.clone(), bold))
                    .chain(marked.link.iter().map(|range| (range.clone(), link)))
                    .collect();
                highlights.sort_by_key(|(range, _)| range.start);
                let app = app.clone();
                InteractiveText::new(
                    "ai-compose-flood-toast",
                    StyledText::new(marked.text.clone()).with_highlights(highlights),
                )
                .on_click(marked.link.iter().cloned().collect(), move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.open_premium(cx));
                })
                .into_any_element()
            })
    }
}
