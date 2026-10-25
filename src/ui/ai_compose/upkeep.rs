//! Per-frame upkeep for the AI compose box: the composer button's blink,
//! the box's answers, a changed style list and the style pills' custom
//! emoji, plus the error toasts (tdesktop `ComposeAiContent::showError`).

use super::{MarkedLink, marked_link, session_mut};
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
        self.ai_style_edit_upkeep(window, cx);
        self.refresh_ai_compose_styles(window, cx);
        self.select_saved_ai_style(window, cx);
        self.ai_style_notice_upkeep(window, cx);
        self.ai_apply_in_place_upkeep(window, cx);
        self.ai_style_preview_upkeep(window, cx);
        self.fetch_ai_compose_emoji();
        self.fetch_ai_style_icons();
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

    /// The custom emoji the box and the preview show (the style pills,
    /// the editor's icon, the previewed style): resolve each once, then
    /// fetch its still.
    fn fetch_ai_compose_emoji(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        let open = self.ai_compose.open.as_ref();
        let pills = open
            .into_iter()
            .flat_map(|_| session.messages.ai_compose.styles.styles())
            .map(|style| style.custom_emoji_id);
        let editor = open
            .and_then(|open| open.editor.as_ref())
            .map(|editor| editor.emoji_id);
        let previewed = self
            .ai_compose
            .preview
            .as_ref()
            .and_then(|preview| preview.style.as_ref())
            .map(|style| style.custom_emoji_id);
        let mut unresolved = Vec::new();
        let mut files: Vec<FileId> = Vec::new();
        for id in pills.chain(editor).chain(previewed) {
            if id <= 0 || self.ai_emoji_still(id).is_some() {
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
                    files.extend(
                        [item.display_file_id(), item.thumb_file_id]
                            .into_iter()
                            .flatten(),
                    );
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
            AiBoxError::TooLong => Notification::new().message(tr(keys::AI_COMPOSE_ERROR_TOO_LONG)),
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
        Self::marked_toast("ai-compose-flood-toast", marked, cx, |this, _, cx| {
            this.open_premium(cx);
        })
        .icon(IconName::Star)
    }

    /// A toast whose text has bold runs and one link; a click on the link
    /// runs `on_link`.
    pub(super) fn marked_toast(
        id: &'static str,
        marked: MarkedLink,
        cx: &mut Context<Self>,
        on_link: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
    ) -> Notification {
        let app = cx.entity().downgrade();
        Notification::new().content(move |_, _, cx| {
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
                id,
                StyledText::new(marked.text.clone()).with_highlights(highlights),
            )
            .on_click(
                marked.link.iter().cloned().collect(),
                move |_, window, cx| {
                    let _ = app.update(cx, |this, cx| on_link(this, window, cx));
                },
            )
            .into_any_element()
        })
    }
}
