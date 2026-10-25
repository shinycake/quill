//! The box's kit dialog: the title with the info button, the view it
//! shows (card, language picker, Cocoon) and the footer (tdesktop
//! `ComposeAiBox` and its `rebuildButtons`).

use super::pill_radius;
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell, dialog_title, scrollable_dialog_content};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::FooterAction;
use quill::lang::{keys, tr};
use std::cell::RefCell;
use std::rc::Rc;

/// The Increase Limit badge (tdesktop `CustomEmojiTextBadge(u"x50"_q)`).
const LIMIT_BADGE: &str = "x50";

const CLOSE_BUTTON_ROOM: f32 = 28.;

impl QuillApp {
    fn build_ai_compose_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AiCompose, |this, _, cx| {
                this.ai_compose_dismissed(cx);
            });
        app.update(cx, |this, cx| {
            let view = this
                .ai_compose
                .open
                .as_ref()
                .map_or(AiComposeView::Card, |open| open.view);
            let (title, body, footer) = match view {
                AiComposeView::Card => (
                    Some(this.ai_compose_title(cx)),
                    this.ai_compose_card_view(cx),
                    this.ai_compose_footer(cx),
                ),
                AiComposeView::Languages => (
                    Some(dialog_title(tr(keys::LANGUAGES)).into_any_element()),
                    this.ai_compose_languages_view(cx),
                    this.ai_compose_languages_footer(cx),
                ),
                AiComposeView::About => (
                    None,
                    this.ai_compose_about_view(cx),
                    this.ai_compose_about_footer(cx),
                ),
                AiComposeView::StyleEditor => (
                    Some(dialog_title(this.ai_style_editor_title()).into_any_element()),
                    this.ai_style_editor_view(cx),
                    this.ai_style_editor_footer(cx),
                ),
                AiComposeView::StyleIcon => (
                    Some(dialog_title(tr(keys::AI_COMPOSE_TONE_ICON_TITLE)).into_any_element()),
                    this.ai_style_icons_view(cx),
                    this.ai_style_icons_footer(cx),
                ),
            };
            let dialog = match title {
                Some(title) => dialog.title(title),
                None => dialog,
            };
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

    /// Escape, the backdrop or the close button. The language picker, the
    /// Cocoon view and the style editor go back to the card and the icon
    /// picker to the editor, as closing tdesktop's boxes on top of the AI
    /// box does; the card closes the box.
    fn ai_compose_dismissed(&mut self, cx: &mut Context<Self>) {
        match self.ai_compose.open.as_ref().map(|open| open.view) {
            Some(AiComposeView::Languages | AiComposeView::About) => {
                self.set_ai_compose_view(AiComposeView::Card, cx);
            }
            Some(AiComposeView::StyleEditor) => {
                if let Some(open) = self.ai_compose.open.as_mut() {
                    open.editor = None;
                }
                self.set_ai_compose_view(AiComposeView::Card, cx);
            }
            Some(AiComposeView::StyleIcon) => {
                self.set_ai_compose_view(AiComposeView::StyleEditor, cx);
            }
            _ => self.close_ai_compose(cx),
        }
    }

    /// "AI Editor", a sparkle while loading (tdesktop appends one to the
    /// title), and the info button left of the kit's close button.
    fn ai_compose_title(&self, cx: &mut Context<Self>) -> AnyElement {
        let loading = self
            .ai_compose
            .open
            .as_ref()
            .is_some_and(|open| open.ai.loading() && !open.ai.premium_flooded());
        let primary = cx.theme().primary;
        h_flex()
            .w_full()
            .items_center()
            .gap_1p5()
            .pr(px(CLOSE_BUTTON_ROOM))
            .child(dialog_title(tr(keys::AI_COMPOSE_TITLE)))
            .when(loading, |row| {
                row.child(
                    Icon::new(IconName::Sparkles)
                        .size(px(14.))
                        .text_color(primary),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("ai-compose-info")
                    .ghost()
                    .small()
                    .icon(IconName::Info)
                    .tooltip(tr(keys::SR_AI_COMPOSE_INFO))
                    .accessibility_label(tr(keys::SR_AI_COMPOSE_INFO))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_ai_compose_view(AiComposeView::About, cx);
                    })),
            )
            .into_any_element()
    }

    fn ai_compose_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(open) = self.ai_compose.open.as_ref() else {
            return div().into_any_element();
        };
        let loading = open.ai.loading() && !open.ai.premium_flooded();
        let footer = open.ai.footer();
        let main = match footer {
            FooterAction::IncreaseLimit => {
                let theme = cx.theme();
                let badge = Tag::custom(
                    theme.primary_foreground,
                    theme.primary,
                    theme.primary_foreground,
                )
                .small()
                .rounded_full()
                .child(LIMIT_BADGE);
                Button::new("ai-compose-increase")
                    .primary()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(tr(keys::AI_COMPOSE_INCREASE_LIMIT))
                            .child(badge),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_ai_compose(cx);
                        this.open_premium_at(
                            quill::premium_hub::PremiumFeature::TextComposition,
                            cx,
                        );
                        this.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
                    }))
            }
            FooterAction::SelectStyle => Button::new("ai-compose-select-style")
                .primary()
                .label(tr(keys::AI_COMPOSE_SELECT_STYLE))
                .disabled(true),
            FooterAction::Apply { style, enabled } => {
                let label = if style {
                    tr(keys::AI_COMPOSE_APPLY_STYLE)
                } else {
                    tr(keys::AI_COMPOSE_APPLY)
                };
                let spinner = Spinner::new().small().color(cx.theme().primary_foreground);
                Button::new("ai-compose-apply")
                    .primary()
                    .map(|button| {
                        if loading {
                            button.loading(true).child(spinner)
                        } else {
                            button.label(label).disabled(!enabled)
                        }
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_ai_compose(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
                    }))
            }
        };
        let send = matches!(footer, FooterAction::Apply { enabled: true, .. })
            && self.ai_compose_can_send();
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .debug_selector(|| "ai-compose-main-button".into())
                    .child(main.w_full().rounded(pill_radius())),
            )
            .when(send, |row| {
                row.child(
                    Button::new("ai-compose-send")
                        .primary()
                        .icon(IconName::Send)
                        .rounded(pill_radius())
                        .tooltip(tr(keys::SEND_BUTTON))
                        .accessibility_label(tr(keys::SEND_BUTTON))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_ai_compose(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
                        })),
                )
            })
            .into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    AiCompose => DialogSpec::new(
        7720,
        |app| app.ai_compose.open.is_some(),
        QuillApp::build_ai_compose_dialog,
    ),
}
