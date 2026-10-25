//! The New Style / Edit Style view (tdesktop boxes/create_ai_tone_box.cpp
//! `SetupToneBox`): the icon, the name, the instructions, "Add a link to my
//! account" and, when editing, Delete Style; Create or Save in the footer.

use super::{attention_button, pill_radius};
use crate::ui::ai_compose_state::{AiComposeView, EditorWarning, StyleEditor};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::TextCompositionStyle;
use quill::lang::{keys, tr};

/// tdesktop `aiToneIconPreview`: the circle and the emoji in it.
const ICON_CIRCLE: f32 = 80.;
const ICON_INNER: f32 = 54.;

/// `text` cut to `max` characters (tdesktop `setMaxLength`), on one line
/// when `single_line`; `None` when it already fits.
pub(super) fn clamp_style_field(
    text: &str,
    max: Option<usize>,
    single_line: bool,
) -> Option<String> {
    let line: String = if single_line {
        text.chars().filter(|c| *c != '\n').collect()
    } else {
        text.to_string()
    };
    let clamped: String = match max {
        Some(max) => line.chars().take(max).collect(),
        None => line,
    };
    (clamped != text).then_some(clamped)
}

impl QuillApp {
    pub(super) fn open_ai_style_editor(
        &mut self,
        editing: Option<TextCompositionStyle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title, prompt) = editing
            .as_ref()
            .map(|style| (style.title.clone(), style.prompt.clone()))
            .unwrap_or_default();
        self.ai_compose.style_name.update(cx, |state, cx| {
            state.set_value(title, window, cx);
            state.focus(window, cx);
        });
        self.ai_compose
            .style_prompt
            .update(cx, |state, cx| state.set_value(prompt, window, cx));
        let Some(open) = self.ai_compose.open.as_mut() else {
            return;
        };
        open.editor = Some(StyleEditor {
            emoji_id: editing.as_ref().map_or(0, |style| style.custom_emoji_id),
            show_creator: editing
                .as_ref()
                .is_some_and(|style| style.creator_user_id != 0),
            editing: editing.map(|style| style.name),
            warning: None,
            pending: false,
        });
        open.view = AiComposeView::StyleEditor;
        cx.notify();
    }

    pub(in crate::ui) fn clamp_ai_style_field(
        &mut self,
        field: &Entity<TextareaState>,
        single_line: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let limits = self
            .session()
            .map(|s| s.messages.ai_compose.limits)
            .unwrap_or_default();
        let max = if single_line {
            limits.title_length_max
        } else {
            limits.prompt_length_max
        };
        let text = field.read(cx).value();
        if let Some(clamped) = clamp_style_field(&text, max, single_line) {
            field.update(cx, |state, cx| state.set_value(clamped, window, cx));
        }
    }

    fn ai_style_editor(&self) -> Option<&StyleEditor> {
        self.ai_compose.open.as_ref()?.editor.as_ref()
    }

    pub(super) fn ai_style_editor_mut(&mut self) -> Option<&mut StyleEditor> {
        self.ai_compose.open.as_mut()?.editor.as_mut()
    }

    pub(super) fn ai_style_editor_title(&self) -> &'static str {
        match self.ai_style_editor().and_then(|e| e.editing.as_ref()) {
            Some(_) => tr(keys::AI_COMPOSE_EDIT_TONE_TITLE),
            None => tr(keys::AI_COMPOSE_CREATE_TONE_TITLE),
        }
    }

    pub(super) fn ai_style_editor_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.ai_style_editor() else {
            return div().into_any_element();
        };
        let warning = editor.warning;
        let theme = cx.theme();
        let (circle, muted, danger) = (theme.muted, theme.muted_foreground, theme.danger);
        let warn = move |which: EditorWarning, key: quill::lang::Key| {
            (warning == Some(which)).then(|| {
                div()
                    .debug_selector(|| "ai-style-warning".into())
                    .text_xs()
                    .text_color(danger)
                    .child(tr(key))
            })
        };
        let icon = match self.ai_emoji_still(editor.emoji_id) {
            Some(path) => img(path)
                .size(px(ICON_INNER))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => Icon::new(IconName::Sparkles)
                .size(px(32.))
                .text_color(muted)
                .into_any_element(),
        };
        let editing = editor.editing.clone();
        v_flex()
            .gap_2()
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .pt_1()
                    .pb_3()
                    .child(
                        div()
                            .id("ai-style-icon")
                            .debug_selector(|| "ai-style-icon".into())
                            .size(px(ICON_CIRCLE))
                            .rounded_full()
                            .bg(circle)
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .role(Role::Button)
                            .aria_label(tr(keys::AI_COMPOSE_TONE_ICON_TITLE))
                            .tab_index(0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_ai_style_icons(cx);
                            }))
                            .child(icon),
                    )
                    .children(warn(EditorWarning::Icon, keys::AI_COMPOSE_TONE_WARN_ICON)),
            )
            .child(
                Textarea::new(&self.ai_compose.style_name)
                    .aria_label(tr(keys::AI_COMPOSE_TONE_NAME_PLACEHOLDER)),
            )
            .children(warn(EditorWarning::Name, keys::AI_COMPOSE_TONE_WARN_NAME))
            .child(
                Textarea::new(&self.ai_compose.style_prompt)
                    .aria_label(tr(keys::AI_COMPOSE_TONE_PROMPT_PLACEHOLDER)),
            )
            .children(warn(
                EditorWarning::Prompt,
                keys::AI_COMPOSE_TONE_WARN_PROMPT,
            ))
            .child(
                h_flex().justify_center().py_2().child(
                    Checkbox::new("ai-style-author")
                        .label(tr(keys::AI_COMPOSE_TONE_AUTHOR))
                        .checked(editor.show_creator)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if let Some(editor) = this.ai_style_editor_mut() {
                                editor.show_creator = *checked;
                            }
                            cx.notify();
                        })),
                ),
            )
            .when_some(editing, |column, name| {
                column.child(
                    Button::new("ai-style-delete")
                        .custom(attention_button(cx))
                        .w_full()
                        .rounded(pill_radius())
                        .label(tr(keys::AI_COMPOSE_TONE_DELETE))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_remove_ai_style(&name, false, window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn ai_style_editor_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.ai_style_editor() else {
            return div().into_any_element();
        };
        let label = match editor.editing {
            Some(_) => tr(keys::AI_COMPOSE_TONE_SAVE),
            None => tr(keys::AI_COMPOSE_TONE_CREATE),
        };
        div()
            .w_full()
            .debug_selector(|| "ai-style-submit".into())
            .child(
                Button::new("ai-style-submit")
                    .primary()
                    .w_full()
                    .rounded(pill_radius())
                    .label(label)
                    .disabled(editor.pending)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_ai_style_editor(window, cx);
                    })),
            )
            .into_any_element()
    }

    /// tdesktop's `save`: the icon, then the name, then the instructions
    /// must be set; the first one missing gets the warning and nothing is
    /// sent.
    pub(super) fn submit_ai_style_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .ai_compose
            .style_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        let prompt = self
            .ai_compose
            .style_prompt
            .read(cx)
            .value()
            .trim()
            .to_string();
        let Some(editor) = self.ai_style_editor_mut() else {
            return;
        };
        if editor.pending {
            return;
        }
        editor.warning = if editor.emoji_id == 0 {
            Some(EditorWarning::Icon)
        } else if title.is_empty() {
            Some(EditorWarning::Name)
        } else if prompt.is_empty() {
            Some(EditorWarning::Prompt)
        } else {
            None
        };
        cx.notify();
        if editor.warning.is_some() {
            return;
        }
        editor.pending = true;
        let editor = editor.clone();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let sent = match editor.editing.as_deref() {
            Some(name) => live.driver.edit_text_composition_style(
                name,
                &title,
                editor.emoji_id,
                &prompt,
                editor.show_creator,
            ),
            None => live.driver.create_text_composition_style(
                &title,
                editor.emoji_id,
                &prompt,
                editor.show_creator,
            ),
        };
        if sent.is_err() {
            if let Some(editor) = self.ai_style_editor_mut() {
                editor.pending = false;
            }
            self.ai_compose_error_toast(
                &quill::ai_compose::AiBoxError::Other(String::new()),
                window,
                cx,
            );
        }
    }
}

#[cfg(test)]
mod clamp_tests {
    use super::clamp_style_field;

    #[test]
    fn a_field_within_its_limit_is_kept() {
        assert_eq!(clamp_style_field("Pirate", Some(12), true), None);
        assert_eq!(clamp_style_field("a\nb", None, false), None);
    }

    #[test]
    fn a_long_name_is_cut_by_characters_and_kept_on_one_line() {
        assert_eq!(
            clamp_style_field("Pi\nrat\u{e9}s", Some(6), true).as_deref(),
            Some("Pirat\u{e9}")
        );
    }
}
