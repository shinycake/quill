//! The Translate tab's language picker (tdesktop `ChooseLanguageBox` with
//! the box's pinned row of translate styles): a search field and the
//! languages, with "Neutral" and the styles under them.

use super::pill_radius;
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::StyleChoice;
use quill::lang::{keys, tr};
use quill::translate::search_languages;

impl QuillApp {
    pub(super) fn ai_compose_languages_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(open) = self.ai_compose.open.as_ref() else {
            return div().into_any_element();
        };
        let current = open.ai.translate_to().to_string();
        let query = self.ai_compose.language_search.read(cx).value().to_string();
        let theme = cx.theme();
        let (hover, border, primary, muted) = (
            theme.accent,
            theme.border,
            theme.primary,
            theme.muted_foreground,
        );
        let languages = search_languages(&query);
        let empty = languages.is_empty();
        let rows = languages.into_iter().map(|(code, english, native)| {
            let selected = code == current;
            div()
                .id(SharedString::from(format!("ai-compose-lang-{code}")))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .role(Role::Button)
                .aria_label(english)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.ai_compose_update(window, cx, |ai| ai.set_translate_to(code));
                    this.set_ai_compose_view(AiComposeView::Card, cx);
                }))
                .child(div().w(px(18.)).flex_none().when(selected, |slot| {
                    slot.child(Icon::new(IconName::Check).size(px(16.)).text_color(primary))
                }))
                .child(div().flex_1().min_w_0().text_sm().child(english))
                .when(native != english, |row| {
                    row.child(div().text_xs().text_color(muted).child(native))
                })
        });
        v_flex()
            .gap_2()
            .child(
                Textarea::new(&self.ai_compose.language_search)
                    .aria_label(tr(keys::Q_MISC_SEARCH_LANGUAGES))
                    .h(px(36.)),
            )
            .when(empty, |column| {
                column.child(
                    div()
                        .px_3()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::LANGUAGES_NONE)),
                )
            })
            .child(
                v_flex()
                    .border_t_1()
                    .border_color(border)
                    .pt_1()
                    .children(rows),
            )
            .into_any_element()
    }

    pub(super) fn ai_compose_languages_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(open) = self.ai_compose.open.as_ref() else {
            return div().into_any_element();
        };
        let picked = open.ai.translate_style().clone();
        let styles = self
            .session()
            .map(|s| s.messages.ai_compose.styles.styles().to_vec())
            .unwrap_or_default();
        let pill = |id: SharedString, selected: bool, glyph: AnyElement, title: String| {
            Button::new(id)
                .small()
                .rounded(pill_radius())
                .map(|pill| {
                    if selected {
                        pill.primary()
                    } else {
                        pill.outline()
                    }
                })
                .child(h_flex().gap_1().items_center().child(glyph).child(title))
        };
        let neutral = pill(
            "ai-translate-style-neutral".into(),
            picked == StyleChoice::None,
            div().child("\u{1F3F3}\u{FE0F}").into_any_element(),
            tr(keys::AI_COMPOSE_STYLE_NEUTRAL).to_string(),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.pick_ai_translate_style(StyleChoice::None, window, cx);
        }));
        let styled = styles.iter().map(|style| {
            let choice = StyleChoice::Style(style.name.clone());
            let selected = picked == choice;
            pill(
                SharedString::from(format!("ai-translate-style-{}", style.name)),
                selected,
                self.ai_style_glyph(style),
                style.title.clone(),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_ai_translate_style(choice.clone(), window, cx);
            }))
        });
        v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .id("ai-compose-translate-styles")
                    .gap_1p5()
                    .pb_1()
                    .overflow_x_scroll()
                    .child(neutral)
                    .children(styled),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("ai-compose-languages-cancel")
                        .ghost()
                        .label(tr(keys::CANCEL))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_ai_compose_view(AiComposeView::Card, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn pick_ai_translate_style(
        &mut self,
        choice: StyleChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_compose_update(window, cx, |ai| ai.select_translate_style(choice));
        self.set_ai_compose_view(AiComposeView::Card, cx);
    }
}
