//! Per-frame upkeep for the AI compose box: the composer button's blink,
//! the box's answers, a changed style list and the style pills' custom
//! emoji, plus the error toasts (tdesktop `ComposeAiContent::showError`).
//!
//! The box and the apply-in-place shortcut belong to the window they were
//! opened in (a chat window's composer, or the main window's). Their
//! answers are taken in that window's frame (`ai_compose_window_upkeep`,
//! a step of every window's upkeep), so they land while the main window
//! is minimized and their toasts show in that window. The style preview
//! a link opens is taken in the frame of the window that shows it, and so
//! are the style notices and the custom emoji they draw. Only the composer
//! button's blink, which follows the main window's field, stays in the main
//! window's upkeep.

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
    /// The main window's part: the composer button's blink.
    pub(in crate::ui) fn ai_compose_frame_upkeep(&mut self, cx: &mut Context<Self>) {
        self.sync_ai_compose_button(cx);
    }

    /// The window in place's own part: the box opened over its composer
    /// and the shortcut it asked for. The box goes when its window shows
    /// another conversation, another topic of the same forum included
    /// (checked in that window's frame), or closed (checked in any).
    pub(in crate::ui) fn ai_compose_window_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let here = self.windows.scope;
        if let Some((asked, owner)) = self
            .ai_compose
            .open
            .as_ref()
            .map(|o| (o.conversation.clone(), o.window))
        {
            let closed = owner.is_some_and(|chat| !self.windows.has(chat));
            let left = owner == here && self.open_conversation() != Some(asked);
            if closed || left {
                self.close_ai_compose(cx);
            }
        }
        let box_here = self
            .ai_compose
            .open
            .as_ref()
            .is_none_or(|open| open.window == here);
        if box_here {
            self.ai_compose_box_upkeep(window, cx);
        }
        let preview_here = self.ai_style_preview_is_here();
        // An added or removed style says so in the window that asked:
        // the preview's, else the box's, else the window of the last press.
        if self.ai_style_notice_is_here(box_here, preview_here) {
            self.ai_style_notice_upkeep(window, cx);
        }
        if preview_here {
            self.ai_style_preview_upkeep(window, cx);
        }
        // The pills', the editor's and the preview's custom emoji, asked
        // for in the frame of the window that draws them.
        let drawn_here = (box_here && self.ai_compose.open.is_some()) || preview_here;
        if drawn_here {
            self.fetch_ai_compose_emoji();
            self.fetch_ai_style_icons();
        }
        self.ai_apply_in_place_upkeep(window, cx);
    }

    /// Whether the style notice waiting in the session is this window's:
    /// the window of the preview it answers (Add Style, Remove), else the
    /// window of the open box (its style menu), else the window of the last
    /// press. Taken in the main window's frame only, the toasts showed
    /// behind a chat window, and with the main window minimized its preview
    /// did not close after Add or Remove.
    fn ai_style_notice_is_here(&self, box_here: bool, preview_here: bool) -> bool {
        if self.ai_compose.preview.is_some() {
            return preview_here;
        }
        if self.ai_compose.open.is_some() {
            return box_here;
        }
        self.windows.pressed_here()
    }

    /// The style preview's answers land in the window that shows its box,
    /// or in the main window once that window closed.
    fn ai_style_preview_is_here(&self) -> bool {
        if self.ai_compose.preview.is_none() {
            return false;
        }
        let here = self.windows.scope;
        let owner = self
            .windows
            .window_of_box(crate::ui::shell::DialogKind::AiStylePreview)
            .filter(|chat| self.windows.has(*chat));
        owner == here
    }

    /// The box's answers, in its own window's frame (or any, with no box
    /// open: the answer is dropped).
    fn ai_compose_box_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    }

    /// A style list that changed under the open box (tdesktop
    /// `refreshTones`) drops a picked style the server no longer lists.
    fn refresh_ai_compose_styles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_compose.open.is_none() {
            return;
        }
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
            .and_then(|preview| preview.style())
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
                // The session's own lookup stage per id says whether it is
                // out, answered or gave up; a failed lookup frees the id.
                None if !session.stickers.emoji.custom_lookups.attempted(id) => unresolved.push(id),
                None => {}
            }
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let _ = live.driver.ensure_media_files(&files);
        for id in unresolved {
            // A lookup already out, or one that did not go out, ends the
            // pass; the id is asked for again next frame.
            if !matches!(live.driver.resolve_custom_emoji(id), Ok(Some(_))) {
                break;
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
            this.open_premium_from("ai_compose", cx);
        })
        .icon(IconName::Star)
    }

    pub(in crate::ui) fn marked_toast(
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
