//! The style answers and their toasts: the editor's create / edit answer
//! (tdesktop create_ai_tone_box.cpp `ShowToneToast`), added, removed and
//! deleted styles (preview_ai_tone_box.cpp `ShowToneAddedToast`,
//! `ShowToneRemovedToast`) and the saved-style limit
//! (`ShowAiComposeToneLimitError`).

use super::{marked_link, session_mut};
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use crate::ui::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::StyleLimitNotice;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::{StyleError, StyleNotice};

/// tdesktop `aiComposeToneToastIconSize`.
const TOAST_EMOJI: f32 = 32.;

/// The limit toast's text with its `**` marks and `{link}` still in it.
pub(super) fn style_limit_text(notice: StyleLimitNotice) -> String {
    match notice {
        StyleLimitNotice::Final { count } => trn_with(
            keys::AI_COMPOSE_TONE_SAVED_LIMIT_FINAL,
            count,
            &[("count", &count.to_string())],
        ),
        StyleLimitNotice::Promo {
            count,
            premium_count,
        } => trn_with(
            keys::AI_COMPOSE_TONE_SAVED_LIMIT,
            count,
            &[
                ("count", &count.to_string()),
                ("premium_count", &format!("**{premium_count}**")),
            ],
        ),
    }
}

/// A plain toast; an empty error type reads as the generic error.
pub(super) fn text_toast(text: &str) -> Notification {
    if text.is_empty() {
        Notification::new().message(tr(keys::AI_COMPOSE_ERROR))
    } else {
        Notification::new().message(text.to_string())
    }
}

impl QuillApp {
    pub(super) fn push_ai_toast(
        &self,
        toast: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(toast, cx);
        });
    }

    /// A style toast: the style's emoji (a sparkle until it is
    /// downloaded), a title and the text.
    pub(super) fn ai_style_toast(
        &self,
        title: Option<String>,
        text: String,
        emoji_id: i64,
    ) -> Notification {
        let Some(path) = self.ai_emoji_still(emoji_id) else {
            return Notification::new()
                .icon(IconName::Sparkles)
                .when_some(title, |toast, title| toast.title(title))
                .message(text);
        };
        Notification::new().content(move |_, _, _| {
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    img(path.clone())
                        .size(px(TOAST_EMOJI))
                        .flex_none()
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    v_flex()
                        .gap_0p5()
                        .text_sm()
                        .when_some(title.clone(), |column, title| {
                            column.child(div().font_semibold().child(title))
                        })
                        .child(text.clone()),
                )
                .into_any_element()
        })
    }

    /// tdesktop `ShowAiComposeToneLimitError`: the final toast, or the
    /// Premium promo whose "Premium" opens Premium at the AI feature.
    pub(super) fn ai_style_limit_toast(
        &self,
        notice: StyleLimitNotice,
        cx: &mut Context<Self>,
    ) -> Notification {
        let text = style_limit_text(notice);
        match notice {
            StyleLimitNotice::Final { .. } => Self::marked_toast(
                "ai-style-limit-toast",
                marked_link(&text, "", ""),
                cx,
                |_, _, _| {},
            ),
            StyleLimitNotice::Promo { .. } => {
                let marked = marked_link(&text, "link", tr(keys::AI_COMPOSE_TONE_SAVED_LIMIT_LINK));
                Self::marked_toast("ai-style-limit-toast", marked, cx, |this, window, cx| {
                    this.close_ai_compose(cx);
                    this.ai_compose.preview = None;
                    this.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
                    this.show_premium_ai_compose_preview(cx);
                })
                .icon(IconName::Star)
            }
        }
    }

    /// The create / edit answer for the open editor: back to the card with
    /// the style selected and its toast, or the editor again with the
    /// error.
    pub(super) fn ai_style_edit_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(answer) = session_mut(self).and_then(|s| s.messages.ai_compose.style_edit.take())
        else {
            return;
        };
        let Some(open) = self.ai_compose.open.as_mut() else {
            return;
        };
        let Some(editor) = open.editor.as_mut() else {
            return;
        };
        let toast = match answer.outcome {
            Ok(style) => {
                let key = match editor.editing {
                    Some(_) => keys::AI_COMPOSE_TONE_UPDATED,
                    None => keys::AI_COMPOSE_TONE_CREATED,
                };
                open.editor = None;
                open.view = AiComposeView::Card;
                open.select_saved = Some(style.name.clone());
                Some(self.ai_style_toast(
                    Some(tr_with(key, &[("title", &style.title)])),
                    tr(keys::AI_COMPOSE_TONE_CREATED_DESCRIPTION).to_string(),
                    style.custom_emoji_id,
                ))
            }
            Err(error) => {
                editor.pending = false;
                match error {
                    // The limit notice shows its own toast.
                    StyleError::SavedTooMany => None,
                    StyleError::Invalid => Some(text_toast(tr(keys::AI_COMPOSE_TONE_INVALID))),
                    StyleError::Other(kind) => Some(text_toast(&kind)),
                }
            }
        };
        if let Some(toast) = toast {
            self.push_ai_toast(toast, window, cx);
        }
        cx.notify();
    }

    /// A style just created or edited is selected once the box lists it.
    pub(super) fn select_saved_ai_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.ai_compose.open.as_mut() else {
            return;
        };
        let Some(name) = open.select_saved.clone() else {
            return;
        };
        if !open.style_names.contains(&name) {
            return;
        }
        open.select_saved = None;
        self.ai_compose_update(window, cx, |ai| ai.select_style(&name));
    }

    /// Added, removed, deleted, the limit and the errors of an add or
    /// remove. Removed and deleted toast only when the preview asked; it
    /// closes then, as after Add.
    pub(super) fn ai_style_notice_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(notice) =
            session_mut(self).and_then(|s| s.messages.ai_compose.style_notice.take())
        else {
            return;
        };
        let toast = match notice {
            StyleNotice::Added(name) => {
                let style = self
                    .ai_compose
                    .preview
                    .take()
                    .and_then(|preview| preview.style)
                    .filter(|style| style.name == name)
                    .or_else(|| {
                        self.session()
                            .and_then(|s| s.messages.ai_compose.styles.find(&name).cloned())
                    });
                style.map(|style| {
                    self.ai_style_toast(
                        Some(tr(keys::AI_COMPOSE_TONE_ADDED).to_string()),
                        tr_with(
                            keys::AI_COMPOSE_TONE_ADDED_DESCRIPTION,
                            &[("name", &style.title)],
                        ),
                        style.custom_emoji_id,
                    )
                })
            }
            StyleNotice::Removed | StyleNotice::Deleted => {
                let asked = self
                    .ai_compose
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.removing);
                asked.then(|| {
                    self.ai_compose.preview = None;
                    let key = match notice {
                        StyleNotice::Deleted => keys::AI_COMPOSE_TONE_DELETED,
                        _ => keys::AI_COMPOSE_TONE_REMOVED,
                    };
                    Notification::new().icon(IconName::Trash).message(tr(key))
                })
            }
            StyleNotice::Limit(limit) => Some(self.ai_style_limit_toast(limit, cx)),
            StyleNotice::Invalid => Some(text_toast(tr(keys::AI_COMPOSE_TONE_INVALID))),
            StyleNotice::Error(kind) => Some(text_toast(&kind)),
        };
        if let Some(toast) = toast {
            self.push_ai_toast(toast, window, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::style_limit_text;
    use quill::ai_compose::StyleLimitNotice;
    use quill::lang::{keys, trn_with};

    #[test]
    fn used_by_counts_people() {
        let used = |count: i64| {
            trn_with(
                keys::AI_COMPOSE_TONE_PREVIEW_USED_BY,
                count,
                &[("count", &count.to_string())],
            )
        };
        assert_eq!(used(1), "Used by 1 person.");
        assert_eq!(used(12), "Used by 12 people.");
    }

    #[test]
    fn limit_texts_follow_the_count() {
        assert_eq!(
            style_limit_text(StyleLimitNotice::Final { count: 1 }),
            "You can save up to **1** style. Delete one to add another."
        );
        assert_eq!(
            style_limit_text(StyleLimitNotice::Promo {
                count: 5,
                premium_count: 20
            }),
            "Subscribe to {link} to save up to **20** styles, or delete one of your **5** styles to add another."
        );
    }
}
