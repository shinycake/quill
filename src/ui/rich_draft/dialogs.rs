//! The rich draft's boxes: "discard this message?" (tdesktop
//! `initDiscardRichDraftButton`'s confirm box), "Remove Formatting"
//! (`Iv::Editor::OfferRichMessagePremiumChoice`) and the Premium toast
//! (`ShowRichMessagesPremiumToast`).

use crate::ui::ai_compose::marked_link;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::premium_hub::PremiumFeature;
use std::cell::RefCell;
use std::rc::Rc;

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 320.;
/// tdesktop `ivEditorPremiumChoiceIconCircle`.
const ICON_CIRCLE: f32 = 64.;

impl QuillApp {
    fn close_rich_discard_confirm(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.rich_draft.discard_confirm = false;
        cx.notify();
    }

    fn close_rich_premium_choice(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.rich_draft.premium_choice = None;
        cx.notify();
    }

    /// tdesktop `ShowRichMessagesPremiumToast`: a star, the requirement and
    /// a "Telegram Premium" link to the feature's Premium page.
    pub(in crate::ui) fn show_rich_premium_toast(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let marked = marked_link(
            tr(keys::ARTICLE_PREMIUM_REQUIRED),
            "link",
            tr(keys::ARTICLE_PREMIUM_REQUIRED_LINK),
        );
        let toast = Self::marked_toast("rich-premium-toast", marked, cx, |this, _, cx| {
            this.open_premium_at(PremiumFeature::RichMessages, cx);
        })
        .id::<RichPremiumToast>()
        .icon(IconName::Star);
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(toast, cx);
        });
    }

    pub(in crate::ui) fn build_rich_discard_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::RichDraftDiscard, |this, _, cx| {
                this.close_rich_discard_confirm(cx);
            });
        app.update(cx, |_, cx| {
            let body = div()
                .text_sm()
                .child(tr(keys::IV_EDITOR_DISCARD_DRAFT_SURE))
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("rich-discard-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_rich_discard_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::RichDraftDiscard, window, cx);
                        })),
                )
                .child(
                    Button::new("rich-discard-confirm")
                        .label(tr(keys::RECORD_LOCK_DISCARD))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_rich_discard_confirm(cx);
                            this.discard_rich_draft(cx);
                            this.close_kit_dialog_if_done(DialogKind::RichDraftDiscard, window, cx);
                        })),
                );
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .w(px(BOX_WIDTH))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }

    pub(in crate::ui) fn build_rich_premium_choice_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::RichPremiumChoice, |this, _, cx| {
                this.close_rich_premium_choice(cx);
            });
        app.update(cx, |_, cx| {
            let theme = cx.theme();
            let about = marked_link(
                tr(keys::ARTICLE_PREMIUM_CHOICE_ABOUT),
                "link",
                tr(keys::ARTICLE_PREMIUM_CHOICE_ABOUT_LINK),
            );
            let bold = HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            };
            let highlights: Vec<_> = about
                .bold
                .iter()
                .chain(about.link.iter())
                .map(|range| (range.clone(), bold))
                .collect();
            let icon = div()
                .size(px(ICON_CIRCLE))
                .rounded_full()
                .bg(theme.primary)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::FileText)
                        .size(px(28.))
                        .text_color(theme.primary_foreground),
                );
            let full = |button: Button| button.w_full().rounded(px(999.));
            let body = div()
                .id("rich-premium-choice")
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .pt_2()
                .child(icon)
                .child(
                    div()
                        .text_lg()
                        .font_semibold()
                        .child(tr(keys::ARTICLE_PREMIUM_CHOICE_TITLE)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_center()
                        .child(StyledText::new(about.text).with_highlights(highlights)),
                )
                .into_any_element();
            let footer = div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(full(
                    Button::new("rich-choice-subscribe")
                        .label(tr(keys::POSTS_SUBSCRIBE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_rich_premium_choice(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::RichPremiumChoice,
                                window,
                                cx,
                            );
                            this.open_premium_at(PremiumFeature::RichMessages, cx);
                        })),
                ))
                .child(full(
                    Button::new("rich-choice-plain")
                        .label(tr(keys::ARTICLE_PREMIUM_CHOICE_PLAIN))
                        .outline()
                        .on_click(cx.listener(|this, _, window, cx| {
                            let plain = this.composer_ui.rich_draft.premium_choice.take();
                            this.close_kit_dialog_if_done(
                                DialogKind::RichPremiumChoice,
                                window,
                                cx,
                            );
                            if let Some(plain) = plain {
                                this.send_rich_draft_plain(plain, window, cx);
                            }
                            cx.notify();
                        })),
                ))
                .child(full(
                    Button::new("rich-choice-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_rich_premium_choice(cx);
                            this.close_kit_dialog_if_done(
                                DialogKind::RichPremiumChoice,
                                window,
                                cx,
                            );
                        })),
                ));
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .w(px(BOX_WIDTH))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

/// Identifies the Premium toast so a second one replaces it.
struct RichPremiumToast;

crate::ui::shell::register_dialogs! {
    /// tdesktop's "discard this message?" box for a rich draft.
    RichDraftDiscard => DialogSpec::new(
        7626,
        |app| app.composer_ui.rich_draft.discard_confirm,
        QuillApp::build_rich_discard_dialog,
    ),
    /// tdesktop's "Remove Formatting" box for a non-Premium rich send.
    RichPremiumChoice => DialogSpec::new(
        7627,
        |app| app.composer_ui.rich_draft.premium_choice.is_some(),
        QuillApp::build_rich_premium_choice_dialog,
    ),
}
