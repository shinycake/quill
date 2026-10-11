//! "About These Ads" box from the sponsored message's Ad menu (Telegram
//! Desktop `AboutBox` in `menu/menu_sponsored.cpp`, strings
//! `lng_sponsored_revenued_*`). It explains what sponsored messages are,
//! lists the advertiser info TDLib sent (`sponsor.info`, `additional_info`;
//! tdesktop's "Advertiser info" submenu) and links to the ad platform.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::telegram::envelope::SponsoredMessage;
use std::cell::RefCell;
use std::rc::Rc;

/// The ad platform page the footer's "Learn more" opens.
pub(super) const ADS_PLATFORM_URL: &str = "https://ads.telegram.org";
const PREMIUM_URL: &str = "https://t.me/PremiumBot";

/// What the open box shows about the ad it was opened from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SponsoredAbout {
    /// Non-empty advertiser lines, in the order tdesktop lists them.
    pub(super) advertiser: Vec<String>,
}

impl SponsoredAbout {
    pub(super) fn for_message(message: &SponsoredMessage) -> Self {
        Self {
            advertiser: advertiser_lines(&message.sponsor.info, &message.additional_info),
        }
    }
}

/// `sponsor.info` first, then `additional_info`; trimmed, empty lines and
/// exact duplicates dropped (TDLib often repeats the sponsor name).
pub(super) fn advertiser_lines(sponsor_info: &str, additional_info: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in [sponsor_info, additional_info] {
        let line = raw.trim();
        if !line.is_empty() && !lines.iter().any(|l| l == line) {
            lines.push(line.to_string());
        }
    }
    lines
}

impl QuillApp {
    pub(super) fn open_sponsored_about(
        &mut self,
        message: &SponsoredMessage,
        cx: &mut Context<Self>,
    ) {
        self.message_ui.sponsored_about = Some(SponsoredAbout::for_message(message));
        cx.notify();
    }

    pub(super) fn close_sponsored_about(&mut self, cx: &mut Context<Self>) {
        self.message_ui.sponsored_about = None;
        cx.notify();
    }

    pub(super) fn build_sponsored_about_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SponsoredAbout, |this, _, cx| {
                this.close_sponsored_about(cx);
            });
        app.update(cx, |this, cx| {
            let Some(about) = this.message_ui.sponsored_about.clone() else {
                return dialog.on_close(on_close);
            };
            let muted = cx.theme().muted_foreground;
            let border = cx.theme().border;
            let point = move |title: &'static str, text: &'static str| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().font_semibold().child(title))
                    .child(div().text_sm().text_color(muted).child(text))
            };
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().text_color(muted).child(
                    "Telegram Ads are very different from ads on other platforms. \
                     Ads such as this one:",
                ))
                .child(point(
                    "Respect Your Privacy",
                    "Ads on Telegram do not use your personal information and are \
                     based on the channel in which you see them.",
                ))
                .child(point(
                    "Help the Channel Creator",
                    "50% of the revenue from Telegram Ads goes to the owner of the \
                     channel where they are displayed.",
                ))
                .child(
                    point(
                        "Can Be Removed",
                        "You can turn off ads by subscribing to Telegram Premium.",
                    )
                    .child(
                        div().flex().child(
                            Button::new("sponsored-about-premium")
                                .label("Telegram Premium")
                                .link()
                                .on_click(|_, _, cx| cx.open_url(PREMIUM_URL)),
                        ),
                    ),
                )
                .child(
                    point(
                        "Can I Launch an Ad?",
                        "Anyone can create an ad to display in this channel, with \
                         minimal budgets. Check out the Telegram Ad Platform for details.",
                    )
                    .child(
                        div().flex().child(
                            Button::new("sponsored-about-learn-more")
                                .label("Learn more")
                                .link()
                                .on_click(|_, _, cx| cx.open_url(ADS_PLATFORM_URL)),
                        ),
                    ),
                );
            if !about.advertiser.is_empty() {
                let copy_text = about.advertiser.join("\n");
                let mut info = div()
                    .id("sponsored-about-advertiser")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .child(div().text_sm().font_semibold().child("Advertiser info"));
                for line in &about.advertiser {
                    info = info.child(div().text_sm().text_color(muted).child(line.clone()));
                }
                body = body.child(
                    info.child(
                        div().flex().child(
                            Button::new("sponsored-about-copy")
                                .label("Copy")
                                .ghost()
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_text.clone(),
                                    ));
                                }),
                        ),
                    ),
                );
            }
            let body = body
                .child(
                    div().flex().justify_end().child(
                        Button::new("sponsored-about-ok")
                            .label("OK")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_sponsored_about(cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::SponsoredAbout,
                                    window,
                                    cx,
                                );
                            })),
                    ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("About These Ads"))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "About These Ads" from a sponsored message's Ad menu.
    SponsoredAbout => DialogSpec::new(
        7455,
        |app| app.message_ui.sponsored_about.is_some(),
        QuillApp::build_sponsored_about_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::advertiser_lines;

    #[test]
    fn lines_are_trimmed_and_ordered() {
        assert_eq!(
            advertiser_lines("  Example Ads ", "Ad by Example"),
            vec!["Example Ads", "Ad by Example"]
        );
    }

    #[test]
    fn empty_and_duplicate_lines_are_dropped() {
        assert_eq!(advertiser_lines("Curated", ""), vec!["Curated"]);
        assert_eq!(advertiser_lines("Same", " Same "), vec!["Same"]);
        assert!(advertiser_lines("  ", "").is_empty());
    }
}
