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
use std::cell::RefCell;
use std::rc::Rc;

/// One hub row: what it opens (`None` is Quick Replies, its own dialog),
/// icon, title and description, in tdesktop's fallback order.
const FEATURES: [(Option<View>, IconName, &str, &str); 8] = [
    (
        Some(View::Greeting),
        IconName::Hand,
        "Greeting Messages",
        "Create greetings that will be automatically sent to new customers.",
    ),
    (
        Some(View::Away),
        IconName::Moon,
        "Away Messages",
        "Define messages that are automatically sent when you are off.",
    ),
    (
        None,
        IconName::Zap,
        "Quick Replies",
        "Set up shortcuts with rich text and media to respond to messages faster.",
    ),
    (
        Some(View::Hours),
        IconName::Clock,
        "Opening Hours",
        "Show to your customers when you are open for business.",
    ),
    (
        Some(View::Location),
        IconName::MapPin,
        "Location",
        "Display the location of your business on your account.",
    ),
    (
        Some(View::ChatLinks),
        IconName::Link,
        "Links to Chat",
        "Create links that start a chat with you, suggesting the first message.",
    ),
    (
        Some(View::StartPage),
        IconName::MessageSquareHeart,
        "Custom Intro",
        "Customize the message people see before they start a chat with you.",
    ),
    (
        Some(View::Chatbots),
        IconName::Bot,
        "Chatbots",
        "Add any third party chatbots that will process customer interactions.",
    ),
];

const ADS_URL: &str = "https://ads.telegram.org";

fn title_of(view: View, include: bool, link_edit_is_new: bool) -> String {
    match view {
        View::Hub => "Telegram Business".into(),
        View::Greeting => "Greeting Message".into(),
        View::Away => "Away Message".into(),
        View::Hours => "Business Hours".into(),
        View::Day(day) => quill::business_hours_edit::WEEKDAYS[day.min(6)].into(),
        View::TimeZone => "Choose Time Zone".into(),
        View::Location => "Location".into(),
        View::Picker if include => "Include Chats".into(),
        View::Picker => "Exclude Chats".into(),
        View::StartPage => "Start Page".into(),
        View::StickerPick => "Choose Sticker".into(),
        View::ChatLinks => "Links to Chat".into(),
        View::LinkEdit if link_edit_is_new => "New Link".into(),
        View::LinkEdit => "Edit Link".into(),
        View::LinkQr => "Chat Link QR Code".into(),
        View::Chatbots => "Chat Automation".into(),
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
            .child(div().text_sm().font_semibold().child("Ads in Channels"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Do Not Hide Ads"))
                    .child(
                        Switch::new("business-ads")
                            .checked(on)
                            .accessibility_label("Do Not Hide Ads")
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
                    .child(
                        "As a Premium subscriber, you don't see any ads on Telegram, but you can \
                         turn them on, for example, to view your own ads that you launched on the",
                    )
                    .child(
                        Link::new("business-ads-platform")
                            .on_click(|_, _, _| {
                                quill::platform::open_external_url(ADS_URL);
                            })
                            .child("Telegram Ad Platform"),
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
                    "You have now unlocked these additional business features."
                } else {
                    "Turn your account to a business page with these additional features."
                }));
        let hover = cx.theme().secondary;
        let active = cx.theme().secondary_active;
        for (index, (view, icon, title, about)) in FEATURES.into_iter().enumerate() {
            body = body.child(
                div()
                    .id(("business-feature", index as u64))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(title)
                    .tab_index(0)
                    .hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(active))
                    .child(Icon::new(icon).size(px(20.)).text_color(cx.theme().primary))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_semibold().child(title))
                            .child(div().text_xs().text_color(muted).child(about)),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(16.))
                            .text_color(muted),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.business_open_feature(view, window, cx);
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
                    .child("Telegram Business is included in Telegram Premium."),
            );
        }
        let saving = self
            .session()
            .is_some_and(|s| !s.settings.business.saving.is_empty());
        body.when(saving, |this| {
            this.child(div().text_xs().text_color(muted).child("Saving…"))
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
            View::Hub => footer.child(Button::new("business-close").label("Close").on_click(
                cx.listener(|this, _, window, cx| {
                    if this.close_business_saving(true, window, cx) {
                        this.close_kit_dialog_if_done(DialogKind::Business, window, cx);
                    }
                }),
            )),
            View::Picker => footer.child(back("Cancel")).child(
                primary("business-picker-done", "Done").on_click(
                    cx.listener(|this, _, window, cx| this.finish_business_picker(window, cx)),
                ),
            ),
            View::Day(day) => {
                footer
                    .child(back("Cancel"))
                    .child(primary("business-day-save", "Save").on_click(cx.listener(
                        move |this, _, window, cx| this.save_business_day(day, window, cx),
                    )))
            }
            View::LinkEdit => {
                footer
                    .child(back("Cancel"))
                    .child(primary("business-link-save", "Save").on_click(
                        cx.listener(|this, _, window, cx| this.save_business_link(window, cx)),
                    ))
            }
            // The screens save when they are left; the time zone list,
            // sticker panel, links and QR code have nothing to save.
            _ => footer.child(back("Back")),
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
                View::LinkQr => this.business_link_qr_body(cx),
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
