//! The style preview an `addstyle/<name>` link opens (tdesktop
//! boxes/preview_ai_tone_box.cpp `PreviewAiToneBox`): the icon, the title,
//! an example before and after, who uses and made the style, the apply
//! hotkey and Add Style, or Remove / Delete Style once it is saved.

use super::card::section_title;
use super::style_menu::{ai_apply_shortcut_text, bound_to, set_bound_style};
use super::style_notices::text_toast;
use super::{attention_button, pill_radius, session_mut};
use crate::ui::ai_compose_state::AiStylePreview;
use crate::ui::app::QuillApp;
use crate::ui::message_text::rich_text_line;
use crate::ui::shell::{DialogKind, QuillShell, scrollable_dialog_content};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::{StyleExample, StyleExamples, TextCompositionStyle, fresh_token};
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::StyleError;
use quill::telegram::envelope::MessageSender;
use quill::text::TextEntity;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

const ICON_CIRCLE: f32 = 80.;
const ICON_INNER: f32 = 54.;

/// The refresh icon's turn while an example loads (tdesktop
/// `PreviewAiToneExampleCard`: 360° in 600 ms with easeOutBack, then a
/// 1000 ms pause, again while loading).
const SPIN: Duration = Duration::from_millis(600);
const SPIN_PAUSE: Duration = Duration::from_millis(1000);

pub(super) fn spin_angle(elapsed: Duration) -> f32 {
    let cycle = (SPIN + SPIN_PAUSE).as_secs_f32();
    let at = elapsed.as_secs_f32() % cycle;
    let t = (at / SPIN.as_secs_f32()).min(1.);
    ease_out_back(t) * std::f32::consts::TAU
}

fn ease_out_back(t: f32) -> f32 {
    let (c1, c3) = (1.70158_f32, 2.70158_f32);
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}

impl QuillApp {
    pub(in crate::ui) fn open_ai_style_preview(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let token = fresh_token();
        let sent = self.live.as_mut().is_none_or(|live| {
            live.driver
                .search_text_composition_style(&name, token)
                .is_ok()
        });
        if !sent {
            self.push_ai_toast(text_toast(tr(keys::AI_COMPOSE_TONE_INVALID)), window, cx);
            return;
        }
        self.ai_compose.preview = Some(AiStylePreview {
            name,
            token,
            style: None,
            examples: StyleExamples::new(false),
            example: None,
            example_token: 0,
            example_asked_at: None,
            removing: false,
        });
        cx.notify();
    }

    pub(super) fn ai_style_preview_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai_style_lookup_upkeep(window, cx);
        self.ai_style_example_upkeep(window, cx);
        if self
            .ai_compose
            .preview
            .as_ref()
            .is_some_and(|preview| preview.examples.loading())
        {
            self.request_animation_tick(60, cx);
        }
    }

    fn ai_style_lookup_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self
            .ai_compose
            .preview
            .as_ref()
            .filter(|preview| preview.style.is_none())
            .map(|preview| preview.token)
        else {
            return;
        };
        let outcome = session_mut(self).and_then(|s| {
            let slot = &mut s.messages.ai_compose.style_lookup;
            let answered = slot
                .as_ref()
                .is_some_and(|lookup| lookup.token == token && lookup.outcome.is_some());
            if answered { slot.take()?.outcome } else { None }
        });
        let Some(outcome) = outcome else {
            return;
        };
        match outcome {
            Ok(style) => {
                if let Some(preview) = self.ai_compose.preview.as_mut() {
                    preview.examples = StyleExamples::new(style.english_example.is_some());
                    preview.example = style.english_example.clone();
                    preview.style = Some(style);
                }
                let loaded = self
                    .ai_compose
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.example.is_some());
                if !loaded {
                    self.load_ai_style_example(window, cx);
                }
            }
            Err(error) => {
                self.ai_compose.preview = None;
                let toast = match error {
                    StyleError::Other(kind) => text_toast(&kind),
                    StyleError::Invalid | StyleError::SavedTooMany => {
                        text_toast(tr(keys::AI_COMPOSE_TONE_INVALID))
                    }
                };
                self.push_ai_toast(toast, window, cx);
            }
        }
        cx.notify();
    }

    fn ai_style_example_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self
            .ai_compose
            .preview
            .as_ref()
            .filter(|preview| preview.examples.loading())
            .map(|preview| preview.example_token)
        else {
            return;
        };
        let outcome = session_mut(self).and_then(|s| {
            let slot = &mut s.messages.ai_compose.example;
            let answered = slot
                .as_ref()
                .is_some_and(|answer| answer.token == token && answer.outcome.is_some());
            if answered { slot.take()?.outcome } else { None }
        });
        let Some(outcome) = outcome else {
            return;
        };
        let Some(preview) = self.ai_compose.preview.as_mut() else {
            return;
        };
        preview.example_asked_at = None;
        match outcome {
            Ok(example) => {
                preview.examples.loaded();
                preview.example = Some(example);
            }
            Err(error) => {
                preview.examples.failed();
                let kind = match error {
                    StyleError::Other(kind) => kind,
                    StyleError::Invalid | StyleError::SavedTooMany => String::new(),
                };
                self.push_ai_toast(text_toast(&kind), window, cx);
            }
        }
        cx.notify();
    }

    fn load_ai_style_example(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = self.ai_compose.preview.as_mut() else {
            return;
        };
        let Some(number) = preview.examples.start_next() else {
            return;
        };
        let token = fresh_token();
        preview.example_token = token;
        preview.example_asked_at = Some(Instant::now());
        let name = preview.name.clone();
        cx.notify();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let number = i32::try_from(number).unwrap_or(i32::MAX);
        if live
            .driver
            .get_text_composition_style_example(&name, number, token)
            .is_err()
            && let Some(preview) = self.ai_compose.preview.as_mut()
        {
            preview.examples.failed();
            preview.example_asked_at = None;
            self.push_ai_toast(text_toast(""), window, cx);
        }
    }

    fn build_ai_style_preview_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AiStylePreview, |this, _, cx| {
                this.ai_compose.preview = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let body = this.ai_style_preview_view(cx);
            let footer = this.ai_style_preview_footer(cx);
            dialog
                .overlay(true)
                .content(scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn ai_style_preview_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(preview) = self.ai_compose.preview.as_ref() else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (circle, muted) = (theme.muted, theme.muted_foreground);
        let Some(style) = preview.style.as_ref() else {
            return v_flex()
                .debug_selector(|| "ai-style-preview-loading".into())
                .items_center()
                .gap_3()
                .pt_1()
                .child(Skeleton::new().size(px(ICON_CIRCLE)).rounded_full())
                .child(Skeleton::new().w(px(140.)).h(px(18.)))
                .child(Skeleton::new().w(px(260.)).h(px(14.)))
                .child(self.ai_style_example_card(preview, cx))
                .into_any_element();
        };
        let icon = match self.ai_emoji_still(style.custom_emoji_id) {
            Some(path) => img(path)
                .size(px(ICON_INNER))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => Icon::new(IconName::Sparkles)
                .size(px(32.))
                .text_color(muted)
                .into_any_element(),
        };
        let shortcut = (!style.name.is_empty())
            .then(|| ai_apply_shortcut_text(cx))
            .flatten();
        v_flex()
            .debug_selector(|| "ai-style-preview".into())
            .items_center()
            .gap_1()
            .pt_1()
            .child(
                div()
                    .size(px(ICON_CIRCLE))
                    .rounded_full()
                    .bg(circle)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            )
            .child(
                div()
                    .pt_2()
                    .text_size(px(17.))
                    .font_semibold()
                    .child(style.title.clone()),
            )
            .child(
                div()
                    .pb_3()
                    .text_sm()
                    .text_color(muted)
                    .text_center()
                    .child(tr(keys::AI_COMPOSE_TONE_PREVIEW_ABOUT)),
            )
            .child(self.ai_style_example_card(preview, cx))
            .children(self.ai_style_attribution(style, cx))
            .when_some(shortcut, |column, shortcut| {
                let name = style.name.clone();
                column.child(
                    h_flex().justify_center().py_2().child(
                        Checkbox::new("ai-style-preview-hotkey")
                            .label(tr_with(
                                keys::AI_COMPOSE_BIND_USE_HOTKEY,
                                &[("keys", &shortcut)],
                            ))
                            .checked(bound_to(&name))
                            .on_click(cx.listener(move |_, checked: &bool, _, cx| {
                                if *checked {
                                    set_bound_style(Some(&name));
                                } else if bound_to(&name) {
                                    set_bound_style(None);
                                }
                                cx.notify();
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    fn ai_style_example_card(
        &self,
        preview: &AiStylePreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (bg, muted, line) = (
            theme.muted,
            theme.muted_foreground,
            theme.muted_foreground.opacity(0.25),
        );
        let loading = preview.examples.loading() || preview.style.is_none();
        let max = self
            .session()
            .and_then(|s| s.messages.ai_compose.limits.example_count);
        let another = preview.style.is_some() && preview.examples.can_show_another(max);
        let angle = preview
            .example_asked_at
            .map_or(0., |at| spin_angle(at.elapsed()));
        let font = self.msg_font();
        let body = |text: &str, entities: &[TextEntity], key: u64, cx: &mut Context<Self>| {
            if loading {
                return v_flex()
                    .gap_1p5()
                    .py_1()
                    .child(Skeleton::new().w_full().h(px(12.)))
                    .child(Skeleton::new().w(relative(0.6)).h(px(12.)))
                    .into_any_element();
            }
            let seen: HashSet<(i64, u64, u64, bool)> = HashSet::new();
            rich_text_line(
                text,
                entities,
                (0, key),
                false,
                &seen,
                font,
                &HashMap::new(),
                cx,
            )
        };
        let empty = StyleExample {
            source_text: String::new(),
            source_entities: Vec::new(),
            result_text: String::new(),
            result_entities: Vec::new(),
        };
        let example = preview.example.as_ref().unwrap_or(&empty);
        let before = body(&example.source_text, &example.source_entities, 11, cx);
        let after = body(&example.result_text, &example.result_entities, 12, cx);
        v_flex()
            .debug_selector(|| "ai-style-example".into())
            .w_full()
            .gap_2()
            .px_4()
            .pt_3()
            .pb_4()
            .rounded(px(22.))
            .bg(bg)
            .child(
                h_flex()
                    .items_center()
                    .child(section_title(tr(keys::AI_COMPOSE_BEFORE), muted))
                    .child(div().flex_1())
                    .when(another, |row| {
                        row.child(
                            div().debug_selector(|| "ai-style-another".into()).child(
                                Button::new("ai-style-another")
                                    .ghost()
                                    .xsmall()
                                    .rounded(px(8.))
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .items_center()
                                            .child(
                                                Icon::new(IconName::RefreshCw)
                                                    .size(px(12.))
                                                    .rotate(radians(angle)),
                                            )
                                            .child(tr(keys::AI_COMPOSE_TONE_PREVIEW_ADD_EXAMPLE)),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.load_ai_style_example(window, cx);
                                    })),
                            ),
                        )
                    }),
            )
            .child(before)
            .child(div().h(px(1.)).w_full().bg(line))
            .child(section_title(tr(keys::AI_COMPOSE_AFTER), muted))
            .child(after)
            .into_any_element()
    }

    fn ai_style_attribution(
        &self,
        style: &TextCompositionStyle,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let used = (style.install_count > 0).then(|| {
            trn_with(
                keys::AI_COMPOSE_TONE_PREVIEW_USED_BY,
                style.install_count,
                &[("count", &style.install_count.to_string())],
            )
        });
        let creator = (style.creator_user_id != 0)
            .then(|| session.user(style.creator_user_id))
            .flatten()
            .map(|user| user.first_name.trim().to_string())
            .filter(|name| !name.is_empty());
        if used.is_none() && creator.is_none() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let sender = MessageSender::User {
            user_id: style.creator_user_id,
        };
        let created = creator.map(|name| {
            let template = tr(keys::AI_COMPOSE_TONE_PREVIEW_CREATED_BY);
            let (before, after) = template.split_once("{user}").unwrap_or((template, ""));
            h_flex()
                .items_center()
                .gap_1()
                .child(before.trim_end().to_string())
                .child(
                    div().debug_selector(|| "ai-style-creator".into()).child(
                        Button::new("ai-style-creator")
                            .link()
                            .xsmall()
                            .label(name)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.ai_compose.preview = None;
                                this.close_kit_dialog_if_done(
                                    DialogKind::AiStylePreview,
                                    window,
                                    cx,
                                );
                                this.open_avatar_profile(sender, window, cx);
                            })),
                    ),
                )
                .when(!after.is_empty(), |row| row.child(after.to_string()))
        });
        Some(
            h_flex()
                .flex_wrap()
                .justify_center()
                .items_center()
                .gap_1()
                .pt_2()
                .text_xs()
                .text_color(muted)
                .children(used)
                .children(created)
                .into_any_element(),
        )
    }

    fn ai_style_preview_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(style) = self
            .ai_compose
            .preview
            .as_ref()
            .and_then(|preview| preview.style.as_ref())
        else {
            return div().into_any_element();
        };
        let installed = self
            .session()
            .and_then(|s| s.messages.ai_compose.styles.find(&style.name))
            .filter(|saved| saved.is_custom);
        let name = style.name.clone();
        let selector = match installed {
            Some(_) => "ai-style-preview-remove",
            None => "ai-style-preview-add",
        };
        let button = match installed {
            Some(saved) => Button::new("ai-style-preview-remove")
                .custom(attention_button(cx))
                .label(if saved.is_creator {
                    tr(keys::AI_COMPOSE_TONE_DELETE)
                } else {
                    tr(keys::AI_COMPOSE_TONE_REMOVE)
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_remove_ai_style(&name, true, window, cx);
                })),
            None => Button::new("ai-style-preview-add")
                .primary()
                .label(tr(keys::AI_COMPOSE_TONE_PREVIEW_ADD))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.add_text_composition_style(&name);
                    }
                    cx.notify();
                })),
        };
        div()
            .w_full()
            .debug_selector(move || selector.into())
            .child(button.w_full().rounded(pill_radius()))
            .into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    AiStylePreview => DialogSpec::new(
        7725,
        |app| app.ai_compose.preview.is_some(),
        QuillApp::build_ai_style_preview_dialog,
    ),
}

#[cfg(test)]
mod spin_tests {
    use super::{SPIN, SPIN_PAUSE, spin_angle};
    use std::time::Duration;

    #[test]
    fn the_icon_turns_once_then_rests() {
        assert!(spin_angle(Duration::ZERO).abs() < 1e-4);
        let turned = spin_angle(SPIN);
        assert!((turned - std::f32::consts::TAU).abs() < 1e-3);
        let resting = spin_angle(SPIN + SPIN_PAUSE / 2);
        assert!((resting - std::f32::consts::TAU).abs() < 1e-3);
    }
}
