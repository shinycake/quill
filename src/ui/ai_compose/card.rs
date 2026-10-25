//! The box's main view (tdesktop `ComposeAiContent`): the mode tabs, the
//! Style tab's style pills, the preview card with the original and the
//! result, and the "Style by" line under it.

use super::button::hint_bubble;
use super::{marked_link, pill_radius};
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use crate::ui::message_text::rich_text_line;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::{
    AiComposeBox, AiComposeMode, AiResult, CardState, DiffSegmentKind, ResultTitle, StyleChoice,
    StyleList, TextCompositionStyle, diff_segments, prefs,
};
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::MessageSender;
use quill::translate::language_name;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

const MODES: [AiComposeMode; 3] = [
    AiComposeMode::Translate,
    AiComposeMode::Style,
    AiComposeMode::Fix,
];

/// The Fix result as one text: changed runs underlined in `success`, the
/// old text of a replacement and deleted text struck out in `danger`
/// (tdesktop `BuildDiffDisplay`).
pub(super) fn diff_display(
    segments: &[quill::ai_compose::DiffSegment],
    success: Hsla,
    danger: Hsla,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let inserted = HighlightStyle {
        color: Some(success),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(success),
            wavy: false,
        }),
        ..Default::default()
    };
    let struck = HighlightStyle {
        color: Some(danger),
        strikethrough: Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(danger),
        }),
        ..Default::default()
    };
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut push = |text: &mut String, part: &str, style: Option<HighlightStyle>| {
        let start = text.len();
        text.push_str(part);
        if let Some(style) = style.filter(|_| !part.is_empty()) {
            runs.push((start..text.len(), style));
        }
    };
    for segment in segments {
        match &segment.kind {
            DiffSegmentKind::Same => push(&mut text, &segment.text, None),
            DiffSegmentKind::Inserted => push(&mut text, &segment.text, Some(inserted)),
            DiffSegmentKind::Replaced { old_text } => {
                push(&mut text, old_text, Some(struck));
                push(&mut text, &segment.text, Some(inserted));
            }
            DiffSegmentKind::Deleted => push(&mut text, &segment.text, Some(struck)),
        }
    }
    (text, runs)
}

/// "To {language}" or "To {language} ({style})" with the language part as
/// the link; `None` for the plain titles.
fn translate_title(title: &ResultTitle, styles: &StyleList) -> Option<(String, String)> {
    let (code, style) = match title {
        ResultTitle::ToLanguage(code) => (code, None),
        ResultTitle::ToLanguageStyle(code, name) => (code, Some(name)),
        ResultTitle::Original | ResultTitle::Result => return None,
    };
    let language = language_name(code);
    let name = match style {
        Some(name) => {
            let style_title = styles
                .find(name)
                .map_or(name.as_str(), |style| style.title.as_str());
            tr_with(
                keys::AI_COMPOSE_NAME_STYLE,
                &[("name", &language), ("style", style_title)],
            )
        }
        None => language,
    };
    Some((tr(keys::AI_COMPOSE_TO_LANGUAGE).to_string(), name))
}

impl QuillApp {
    pub(super) fn ai_compose_card_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(open) = self.ai_compose.open.as_ref() else {
            return div().into_any_element();
        };
        let ai = &open.ai;
        let style_hint =
            ai.mode() == AiComposeMode::Style && !prefs::current().style_tooltip_hidden;
        v_flex()
            .gap_3()
            .child(self.ai_compose_tabs(ai.mode(), cx))
            .when(ai.mode() == AiComposeMode::Style, |column| {
                column.child(self.ai_compose_style_row(ai.style(), cx))
            })
            .when(style_hint, |column| {
                column.child(
                    h_flex()
                        .justify_center()
                        .child(hint_bubble(tr(keys::AI_COMPOSE_STYLE_TOOLTIP), cx)),
                )
            })
            .child(self.ai_compose_preview(ai, cx))
            .children(self.ai_compose_author(ai, cx))
            .into_any_element()
    }

    fn ai_compose_tabs(&self, mode: AiComposeMode, cx: &mut Context<Self>) -> AnyElement {
        let selected = MODES.iter().position(|m| *m == mode).unwrap_or(0);
        let tab = |selector: &'static str, icon: IconName, label: &'static str| {
            Tab::new()
                .flex_1()
                .aria_label(label)
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(Icon::new(icon).size_4())
                        .child(label),
                )
                .debug_selector(move || selector.into())
        };
        TabBar::new("ai-compose-tabs")
            .pill()
            .w_full()
            .selected_index(selected)
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                if let Some(mode) = MODES.get(*index).copied() {
                    this.ai_compose_update(window, cx, |ai| ai.set_mode(mode));
                }
            }))
            .child(tab(
                "ai-compose-tab-translate",
                IconName::Languages,
                tr(keys::AI_COMPOSE_TAB_TRANSLATE),
            ))
            .child(tab(
                "ai-compose-tab-style",
                IconName::Sparkles,
                tr(keys::AI_COMPOSE_TAB_STYLE),
            ))
            .child(tab(
                "ai-compose-tab-fix",
                IconName::SpellCheck,
                tr(keys::AI_COMPOSE_TAB_FIX),
            ))
            .into_any_element()
    }

    /// A style's emoji still, or a sparkle until it is downloaded (or for a
    /// style without one).
    pub(super) fn ai_style_glyph(&self, style: &TextCompositionStyle) -> AnyElement {
        let still = (style.custom_emoji_id > 0)
            .then(|| self.custom_emoji_still(style.custom_emoji_id))
            .flatten();
        match still {
            Some(path) => img(path)
                .size(px(16.))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => Icon::new(IconName::Sparkles)
                .size(px(14.))
                .into_any_element(),
        }
    }

    /// The Style tab's pills (tdesktop `LabeledEmojiScrollTabs`). The styles
    /// delegate adds the "Create" pill and the pills' context menu here.
    fn ai_compose_style_row(&self, picked: &StyleChoice, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let pills = session
            .messages
            .ai_compose
            .styles
            .styles()
            .iter()
            .map(|style| {
                let selected = matches!(picked, StyleChoice::Style(name) if *name == style.name);
                let name = style.name.clone();
                Button::new(SharedString::from(format!("ai-style-{}", style.name)))
                    .small()
                    .rounded(pill_radius())
                    .map(|pill| {
                        if selected {
                            pill.primary()
                        } else {
                            pill.outline()
                        }
                    })
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(self.ai_style_glyph(style))
                            .child(style.title.clone()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        prefs::update(|prefs| prefs.style_tooltip_hidden = true);
                        this.ai_compose_update(window, cx, |ai| ai.select_style(&name));
                    }))
            });
        h_flex()
            .id("ai-compose-styles")
            .gap_1p5()
            .pb_1()
            .overflow_x_scroll()
            .children(pills)
            .into_any_element()
    }

    /// The preview card (tdesktop `ComposeAiPreviewCard`): the original
    /// collapsed to a line over the result while one is asked for or ready,
    /// otherwise the original alone under "Original".
    fn ai_compose_preview(&self, ai: &AiComposeBox, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, surface, radius) = (theme.muted_foreground, theme.muted, theme.radius_lg);
        let line = muted.opacity(0.25);
        let original = quill::composer::parse_format_markup(ai.source_text()).0;
        let font = self.msg_font();
        let mut card = v_flex()
            .id("ai-compose-card")
            .w_full()
            .min_w_0()
            .gap_2()
            .px_4()
            .pt_3()
            .pb_4()
            .rounded(radius)
            .bg(surface);
        if ai.original_visible() {
            let expanded = ai.original_expanded();
            let expandable = original.contains('\n') || original.chars().count() > 48;
            let (icon, label) = if expanded {
                (
                    IconName::ChevronUp,
                    tr(keys::SR_AI_COMPOSE_COLLAPSE_ORIGINAL),
                )
            } else {
                (
                    IconName::ChevronDown,
                    tr(keys::SR_AI_COMPOSE_EXPAND_ORIGINAL),
                )
            };
            card = card
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(section_title(tr(keys::AI_COMPOSE_ORIGINAL), muted))
                        .when(expandable, |row| {
                            row.child(
                                Button::new("ai-compose-original-toggle")
                                    .ghost()
                                    .xsmall()
                                    .icon(icon)
                                    .tooltip(label)
                                    .accessibility_label(label)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(open) = this.ai_compose.open.as_mut() {
                                            open.ai.toggle_original_expanded();
                                        }
                                        cx.notify();
                                    })),
                            )
                        }),
                )
                .child(div().text_sm().text_color(muted).min_w_0().map(|text| {
                    if expanded {
                        text.child(original.clone())
                    } else {
                        let shown = match original.split_once('\n') {
                            Some((first, _)) => format!("{first}…"),
                            None => original.clone(),
                        };
                        text.truncate().child(shown)
                    }
                }))
                .child(div().h(px(1.)).w_full().my_1().bg(line));
        }
        let title = ai.result_title();
        let styles = self
            .session()
            .map(|s| s.messages.ai_compose.styles.clone())
            .unwrap_or_default();
        let header = h_flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(self.ai_compose_result_title(&title, &styles, muted, cx))
            .when(ai.emojify_visible(), |row| {
                row.child(
                    Checkbox::new("ai-compose-emojify")
                        .checked(ai.add_emojis())
                        .label(tr(keys::AI_COMPOSE_EMOJIFY))
                        .on_click(cx.listener(|this, checked: &bool, window, cx| {
                            let checked = *checked;
                            this.ai_compose_update(window, cx, |ai| ai.set_add_emojis(checked));
                        })),
                )
            });
        let body = match ai.card() {
            CardState::Loading { .. } => v_flex()
                .gap_2()
                .py_1()
                .child(Skeleton::new().h(px(12.)).w_full().bg(line))
                .child(Skeleton::new().h(px(12.)).w_full().bg(line))
                .child(Skeleton::new().h(px(12.)).w(relative(0.6)).bg(line))
                .into_any_element(),
            CardState::Ready(result) => self.ai_compose_result(ai.mode(), result, font, cx),
            CardState::Waiting | CardState::Failed => {
                div().text_size(font).child(original).into_any_element()
            }
        };
        let copy = ai.result().map(|result| result.text.clone());
        card.child(header)
            .child(body)
            .when_some(copy, |card, text| {
                card.child(
                    h_flex().justify_end().child(
                        Button::new("ai-compose-copy")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .tooltip(tr(keys::SR_AI_COMPOSE_COPY_RESULT))
                            .accessibility_label(tr(keys::SR_AI_COMPOSE_COPY_RESULT))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }),
                    ),
                )
            })
            .into_any_element()
    }

    /// "Original" / "Result", or in Translate "To {language}" whose
    /// language opens the language picker.
    fn ai_compose_result_title(
        &self,
        title: &ResultTitle,
        styles: &StyleList,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((template, name)) = translate_title(title, styles) else {
            let text = match title {
                ResultTitle::Original => tr(keys::AI_COMPOSE_ORIGINAL),
                _ => tr(keys::AI_COMPOSE_RESULT),
            };
            return section_title(text, muted).into_any_element();
        };
        let marked = marked_link(&template, "language", "");
        let before = marked.link.as_ref().map_or(marked.text.as_str(), |link| {
            marked.text.get(..link.start).unwrap_or_default()
        });
        let after = marked
            .link
            .as_ref()
            .and_then(|link| marked.text.get(link.end..))
            .unwrap_or_default();
        h_flex()
            .items_center()
            .gap_1()
            .child(section_title(before.trim_end().to_string(), muted))
            .child(
                div().debug_selector(|| "ai-compose-language".into()).child(
                    Button::new("ai-compose-language")
                        .link()
                        .small()
                        .label(name)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_ai_compose_view(AiComposeView::Languages, cx);
                        })),
                ),
            )
            .when(!after.is_empty(), |row| {
                row.child(section_title(after.to_string(), muted))
            })
            .into_any_element()
    }

    fn ai_compose_result(
        &self,
        mode: AiComposeMode,
        result: &AiResult,
        font: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let (AiComposeMode::Fix, Some(diff)) = (mode, result.diff.as_ref()) {
            let theme = cx.theme();
            let (text, runs) = diff_display(&diff_segments(diff), theme.success, theme.danger);
            return div()
                .id("ai-compose-result")
                .text_size(font)
                .child(StyledText::new(text).with_highlights(runs))
                .into_any_element();
        }
        let seen: HashSet<(i64, u64, u64, bool)> = HashSet::new();
        div()
            .id("ai-compose-result")
            .w_full()
            .min_w_0()
            .child(rich_text_line(
                &result.text,
                &result.entities,
                (0, 3),
                false,
                &seen,
                font,
                &HashMap::new(),
                cx,
            ))
            .into_any_element()
    }

    /// "Style by {user}" under a style result that names its creator.
    fn ai_compose_author(&self, ai: &AiComposeBox, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let user_id = ai.author_user_id(&session.messages.ai_compose.styles)?;
        let sender = MessageSender::User { user_id };
        let name = session
            .user(user_id)
            .map(|user| user.first_name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| session.sender_label(sender));
        let marked = marked_link(tr(keys::AI_COMPOSE_AUTHOR), "user", "");
        let start = marked
            .link
            .as_ref()
            .map_or(marked.text.len(), |link| link.start);
        let before = marked.text.get(..start).unwrap_or_default().to_string();
        let after = marked.text.get(start..).unwrap_or_default().to_string();
        let muted = cx.theme().muted_foreground;
        Some(
            h_flex()
                .justify_center()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(muted)
                .child(before.trim_end().to_string())
                .child(
                    div().debug_selector(|| "ai-compose-author".into()).child(
                        Button::new("ai-compose-author")
                            .link()
                            .small()
                            .label(name)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_ai_compose(cx);
                                this.close_kit_dialog_if_done(
                                    crate::ui::shell::DialogKind::AiCompose,
                                    window,
                                    cx,
                                );
                                this.open_avatar_profile(sender, window, cx);
                            })),
                    ),
                )
                .when(!after.is_empty(), |row| row.child(after))
                .into_any_element(),
        )
    }
}

fn section_title(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .text_xs()
        .font_semibold()
        .text_color(color)
        .child(text.into())
}

#[cfg(test)]
mod diff_tests {
    use super::diff_display;
    use gpui_kit::{Hsla, hsla};
    use quill::ai_compose::{DiffSegment, DiffSegmentKind};

    fn seg(text: &str, kind: DiffSegmentKind) -> DiffSegment {
        DiffSegment {
            text: text.into(),
            kind,
        }
    }

    #[test]
    fn replaced_text_shows_old_struck_then_new_underlined() {
        let (success, danger): (Hsla, Hsla) = (hsla(0.3, 1., 0.5, 1.), hsla(0., 1., 0.5, 1.));
        let (text, runs) = diff_display(
            &[
                seg(
                    "the",
                    DiffSegmentKind::Replaced {
                        old_text: "teh".into(),
                    },
                ),
                seg(" big", DiffSegmentKind::Inserted),
                seg(" cat", DiffSegmentKind::Same),
                seg(" very", DiffSegmentKind::Deleted),
            ],
            success,
            danger,
        );
        assert_eq!(text, "tehthe big cat very");
        let ranges: Vec<_> = runs.iter().map(|(range, _)| range.clone()).collect();
        assert_eq!(ranges, vec![0..3, 3..6, 6..10, 14..19]);
        let struck: Vec<bool> = runs
            .iter()
            .map(|(_, style)| style.strikethrough.is_some())
            .collect();
        assert_eq!(struck, vec![true, false, false, true]);
        assert!(runs.iter().all(|(_, style)| style.color.is_some()));
    }

    #[test]
    fn a_replacement_with_no_new_text_strikes_the_old_only() {
        let (text, runs) = diff_display(
            &[seg(
                "",
                DiffSegmentKind::Replaced {
                    old_text: "gone".into(),
                },
            )],
            hsla(0.3, 1., 0.5, 1.),
            hsla(0., 1., 0.5, 1.),
        );
        assert_eq!(text, "gone");
        assert_eq!(runs.len(), 1);
    }
}
