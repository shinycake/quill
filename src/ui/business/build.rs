//! The Telegram Business dialog: the hub (with "Ads in Channels") and the
//! frame of every screen.

use super::View;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::link::Link;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::premium_hub::BusinessFeature;
use std::cell::RefCell;
use std::rc::Rc;

/// The icon of a hub row.
fn icon_of(feature: BusinessFeature) -> IconName {
    match feature {
        BusinessFeature::GreetingMessage => IconName::Hand,
        BusinessFeature::AwayMessage => IconName::Moon,
        BusinessFeature::QuickReplies => IconName::Zap,
        BusinessFeature::OpeningHours => IconName::Clock,
        BusinessFeature::Location => IconName::MapPin,
        BusinessFeature::AccountLinks => IconName::Link,
        BusinessFeature::StartPage => IconName::MessageSquareHeart,
        BusinessFeature::Bots => IconName::Bot,
        BusinessFeature::ChatFolderTags
        | BusinessFeature::EmojiStatus
        | BusinessFeature::UpgradedStories => IconName::Tag,
    }
}

const ADS_URL: &str = "https://ads.telegram.org";

fn title_of(view: View, include: bool, link_edit_is_new: bool) -> String {
    match view {
        View::Hub => tr(keys::BUSINESS_TITLE).into(),
        View::Greeting => tr(keys::Q_OPS_GREETING_MESSAGE).into(),
        View::Away => tr(keys::Q_OPS_AWAY_MESSAGE).into(),
        View::Hours => tr(keys::Q_OPS_BUSINESS_HOURS).into(),
        View::Day(day) => quill::business_hours_edit::WEEKDAYS
            .get(day.min(6))
            .copied()
            .unwrap_or_default()
            .into(),
        View::TimeZone => tr(keys::Q_OPS_CHOOSE_TIME_ZONE).into(),
        View::Location => tr(keys::BUSINESS_SUBTITLE_LOCATION).into(),
        View::Picker if include => tr(keys::Q_OPS_INCLUDE_CHATS).into(),
        View::Picker => tr(keys::Q_OPS_EXCLUDE_CHATS).into(),
        View::StartPage => tr(keys::Q_OPS_START_PAGE).into(),
        View::StickerPick => tr(keys::Q_OPS_CHOOSE_STICKER).into(),
        View::ChatLinks => tr(keys::BUSINESS_SUBTITLE_CHAT_LINKS).into(),
        View::LinkEdit if link_edit_is_new => tr(keys::Q_OPS_NEW_LINK).into(),
        View::LinkEdit => tr(keys::Q_OPS_EDIT_LINK).into(),
        View::Chatbots => tr(keys::Q_OPS_CHAT_AUTOMATION).into(),
    }
}

impl QuillApp {
    /// tdesktop's "Ads in Channels" section, for Premium accounts.
    fn business_ads_section(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let on = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|me| s.user_full_info(me)))
            .is_some_and(|info| info.extras.sponsored_enabled);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::BUSINESS_SUBTITLE_SPONSORED)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(tr(keys::BUSINESS_BUTTON_SPONSORED)))
                    .child(
                        Switch::new("business-ads")
                            .checked(on)
                            .accessibility_label(tr(keys::BUSINESS_BUTTON_SPONSORED))
                            .on_click(cx.listener(|this, &next: &bool, _, cx| {
                                match this.live.as_mut() {
                                    Some(live) => {
                                        let _ = live.driver.set_sponsored_enabled(next);
                                    }
                                    None => this.business_error(Self::DEMO_LINE, cx),
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_0p5()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_AS_A_PREMIUM_SUBSCRIBER_YOU))
                    .child(
                        Link::new("business-ads-platform")
                            .on_click(|_, _, _| {
                                quill::platform::open_external_url(ADS_URL);
                            })
                            .child(tr(keys::Q_OPS_TELEGRAM_AD_PLATFORM)),
                    ),
            )
    }

    fn business_hub_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let premium = self.business_premium();
        let mut body =
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_sm().text_color(muted).child(if premium {
                    tr(keys::BUSINESS_UNLOCKED)
                } else {
                    tr(keys::BUSINESS_ABOUT)
                }));
        let hover = cx.theme().secondary;
        let active = cx.theme().secondary_active;
        // The server's order (`getBusinessFeatures`), else tdesktop's.
        let features = self
            .session()
            .and_then(|s| s.payments.hub.business_features.clone())
            .unwrap_or_else(|| BusinessFeature::FALLBACK_ORDER.to_vec());
        for feature in features {
            let (title, about) = feature.text_keys();
            body = body.child(
                div()
                    .id(SharedString::from(feature.td_type()))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr(title))
                    .tab_index(0)
                    .hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(active))
                    .child(
                        Icon::new(icon_of(feature))
                            .size(px(20.))
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_semibold().child(tr(title)))
                            .child(div().text_xs().text_color(muted).child(tr(about))),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(16.))
                            .text_color(muted),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.business_open_feature(feature, window, cx);
                    })),
            );
        }
        if premium {
            body = body.child(self.business_ads_section(cx));
        } else {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_TELEGRAM_BUSINESS_IS_INCLUDED_IN)),
            );
        }
        let saving = self
            .session()
            .is_some_and(|s| !s.settings.business.saving.is_empty());
        body.when(saving, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_SAVING)),
            )
        })
    }

    fn business_footer(&mut self, view: View, cx: &mut Context<Self>) -> Div {
        let back = |label: &'static str| {
            Button::new("business-back")
                .label(label)
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| this.business_back(window, cx)))
        };
        let primary =
            |id: &'static str, label: &'static str| Button::new(id).label(label).primary();
        let footer = div().flex().justify_end().gap_2();
        match view {
            View::Hub => footer.child(
                Button::new("business-close")
                    .label(tr(keys::CLOSE))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.close_business_saving(true, window, cx) {
                            this.close_kit_dialog_if_done(DialogKind::Business, window, cx);
                        }
                    })),
            ),
            View::Picker => footer.child(back(tr(keys::CANCEL))).child(
                primary("business-picker-done", tr(keys::BOX_DONE)).on_click(
                    cx.listener(|this, _, window, cx| this.finish_business_picker(window, cx)),
                ),
            ),
            View::Day(day) => footer.child(back(tr(keys::CANCEL))).child(
                primary("business-day-save", tr(keys::SETTINGS_SAVE)).on_click(
                    cx.listener(move |this, _, window, cx| this.save_business_day(day, window, cx)),
                ),
            ),
            View::LinkEdit => footer.child(back(tr(keys::CANCEL))).child(
                primary("business-link-save", tr(keys::SETTINGS_SAVE)).on_click(
                    cx.listener(|this, _, window, cx| this.save_business_link(window, cx)),
                ),
            ),
            // The screens save when they are left; the time zone list,
            // sticker panel, links and QR code have nothing to save.
            _ => footer.child(back(tr(keys::Q_OPS_BACK))),
        }
    }

    pub(in crate::ui) fn build_business_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        // Esc, the close button and the backdrop save the open screen too
        // (the kit already closed the dialog, so a problem is reported in
        // the status line).
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Business, |this, window, cx| {
                this.close_business_saving(false, window, cx);
            });
        app.update(cx, |this, cx| {
            let Some((view, include, new_link, local_error, note)) =
                this.dialogs.business_dialog.as_ref().map(|d| {
                    (
                        d.view,
                        d.picker.as_ref().is_some_and(|p| p.include),
                        d.link_edit.as_ref().is_none_or(|e| e.link.is_none()),
                        d.local_error.clone(),
                        d.note.clone(),
                    )
                })
            else {
                return dialog.on_close(on_close);
            };
            let mut body = match view {
                View::Hub => this.business_hub_body(cx),
                View::Greeting => this.business_greeting_body(cx),
                View::Away => this.business_away_body(cx),
                View::Hours => this.business_hours_body(cx),
                View::Day(_) => this.business_day_body(cx),
                View::TimeZone => this.business_zone_body(cx),
                View::Location => this.business_location_body(cx),
                View::Picker => this.business_picker_body(cx),
                View::StartPage => this.business_start_page_body(cx),
                View::StickerPick => this.business_sticker_pick_body(cx),
                View::ChatLinks => this.business_chat_links_body(cx),
                View::LinkEdit => this.business_link_edit_body(cx),
                View::Chatbots => this.business_chatbots_body(cx),
            };
            if let Some(note) = note {
                body = body.child(
                    div()
                        .id("business-note")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(note),
                );
            }
            let server_error = this
                .session()
                .and_then(|s| s.settings.business.error.clone());
            if let Some(line) = local_error.or(server_error) {
                body = body.child(
                    div()
                        .id("business-error")
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(line),
                );
            }
            let footer = this.business_footer(view, cx);
            let content = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .child(footer)
                .into_any_element();
            let content = Rc::new(RefCell::new(Some(content)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title_of(
                    view, include, new_link,
                )))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |slot, _, _| {
                        let body = content
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        slot.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}
