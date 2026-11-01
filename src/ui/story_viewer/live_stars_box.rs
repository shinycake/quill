//! The live story's boxes: the Stars box (tdesktop `VideoStreamStarsBox` →
//! `PaidReactionsBox` in video stream mode), the comment delete box
//! (`ShowDeleteMessageConfirmation` with Report Spam, Delete All and Ban
//! User) and the confirm to leave another call for the live story
//! (`confirmLeaveCurrent`).
//!
//! The Stars box has three modes. Choosing Stars for a comment shows what
//! the level gives (pin time, characters, emoji) above "Highlight and
//! Pin"; sending Stars as a reaction shows the top senders above "React
//! with Stars"; on your own live story it only explains. The slider takes
//! the level's color, and the balance sits under the button.
#![deny(clippy::indexing_slicing)]

use super::live_state::{StarsBoxMode, level_color, pin_label};
use super::live_view::sender_avatar;
use super::*;
use crate::ui::os_open::open_link_in;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::slider::Slider;
use quill::lang::trn;
use quill::live_story::level_for_stars;
use quill::premium_hub::group_digits;
use quill::telegram::envelope::GroupCallMessageLevel;
use std::cell::RefCell;
use std::rc::Rc;

/// `lng_group_invite_subscription_about_url`, the Stars terms.
const STARS_TERMS_URL: &str = "https://telegram.org/tos/stars";

/// One of the three level facts over the comment Stars box.
fn info_block(title: String, subtext: String) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .py(px(8.))
        .rounded(px(8.))
        .bg(rgba(0xffffff0f))
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(div().text_xs().text_color(text_muted()).child(subtext))
}

/// What a comment with these Stars gets (tdesktop `VideoStreamStarsLevel`).
fn level_blocks(level: &GroupCallMessageLevel) -> Div {
    div()
        .flex()
        .gap(px(8.))
        .child(info_block(
            pin_label(level.pin_duration),
            tr(keys::PAID_COMMENT_PIN_ABOUT).to_string(),
        ))
        .child(info_block(
            level.max_text_length.to_string(),
            trn(keys::PAID_COMMENT_LIMIT_ABOUT, level.max_text_length),
        ))
        .child(info_block(
            level.max_custom_emoji_count.to_string(),
            trn(keys::PAID_COMMENT_EMOJI_ABOUT, level.max_custom_emoji_count),
        ))
}

impl QuillApp {
    pub(super) fn build_live_stars_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LiveStoryStars, |this, _, cx| {
                this.close_live_stars_box(cx);
            });
        app.update(cx, |this, cx| {
            let (Some(session), Some(boxed)) =
                (this.session(), this.stories.live.stars_box.as_ref())
            else {
                return dialog.on_close(on_close);
            };
            let mode = boxed.mode;
            let stars = boxed.stars;
            let levels = session.live_levels();
            let level = level_for_stars(&levels, stars);
            let name = this
                .stories
                .live
                .watching
                .and_then(|(chat_id, _)| session.chats.get(&chat_id.0))
                .map(|chat| chat.title.clone())
                .unwrap_or_default();
            let title = match mode {
                StarsBoxMode::Admin => tr(keys::PAID_ADMIN_TITLE),
                StarsBoxMode::Comment => tr(keys::PAID_COMMENT_TITLE),
                StarsBoxMode::Reaction => tr(keys::PAID_REACTION_TITLE),
            };
            let about = match mode {
                StarsBoxMode::Admin => tr(keys::PAID_ADMIN_ABOUT).to_string(),
                StarsBoxMode::Comment => tr_with(keys::PAID_COMMENT_ABOUT, &[("name", &name)]),
                StarsBoxMode::Reaction => tr_with(keys::PAID_REACTION_ABOUT, &[("name", &name)]),
            };
            let mut body = div().flex().flex_col().gap(px(12.));
            if mode == StarsBoxMode::Comment {
                body = body.child(level_blocks(&level));
            }
            if mode != StarsBoxMode::Admin {
                body = body
                    .child(
                        div().flex().justify_center().child(
                            div()
                                .px(px(12.))
                                .py(px(2.))
                                .rounded_full()
                                .bg(level_color(level.first_color, 1.0))
                                .text_color(rgb(0xffffff))
                                .text_size(px(20.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("\u{2605} {}", group_digits(stars))),
                        ),
                    )
                    .child(Slider::new(&boxed.slider).bg(level_color(level.first_color, 1.0)));
            }
            body = body
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_center()
                        .child(title),
                )
                .child(div().text_sm().text_center().child(about));
            if mode != StarsBoxMode::Comment
                && let Some(donors) = session
                    .stories
                    .live
                    .as_ref()
                    .and_then(|l| l.donors.as_ref())
                && !donors.top_donors.is_empty()
            {
                body = body.child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(text_muted())
                        .child(tr(keys::PAID_REACT_TOP_TITLE)),
                );
                let row = donors.top_donors.iter().enumerate().map(|(ix, donor)| {
                    let label = match donor.sender {
                        Some(sender) if !donor.is_anonymous => session.sender_label(sender),
                        _ => tr(keys::PAID_REACT_ANONYMOUS).to_string(),
                    };
                    let avatar = match donor.sender {
                        Some(sender) if !donor.is_anonymous => sender_avatar(session, sender, 48.),
                        _ => crate::ui::message_text::kit_avatar_element(&label, None, px(48.)),
                    };
                    div()
                        .id(("live-story-donor", ix as u64))
                        .w(px(72.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .child(avatar)
                        .child(
                            div()
                                .px(px(6.))
                                .rounded_full()
                                .bg(level_color(
                                    level_for_stars(&levels, donor.star_count).first_color,
                                    1.0,
                                ))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child(format!("\u{2605}{}", group_digits(donor.star_count))),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_center()
                                .truncate()
                                .child(label),
                        )
                });
                body = body.child(div().flex().justify_center().gap(px(8.)).children(row));
            }
            let divider: AnyElement = if mode == StarsBoxMode::Admin {
                div()
                    .text_xs()
                    .text_color(text_muted())
                    .child(tr(keys::PAID_REACT_ADMIN_CANT))
                    .into_any_element()
            } else {
                let link = tr(keys::PAID_REACT_AGREE_LINK);
                let text = tr_with(keys::PAID_REACT_AGREE, &[("link", link)]);
                let start = text.find(link).unwrap_or(text.len());
                let range = start..start.saturating_add(link.len()).min(text.len());
                InteractiveText::new(
                    "live-story-stars-terms",
                    StyledText::new(text).with_highlights(vec![(
                        range.clone(),
                        HighlightStyle {
                            color: Some(cx.theme().link),
                            ..Default::default()
                        },
                    )]),
                )
                .on_click(vec![range], {
                    let app = app.downgrade();
                    move |_, _, cx| open_link_in(&app, STARS_TERMS_URL, cx)
                })
                .into_any_element()
            };
            body = body.child(div().text_xs().text_color(text_muted()).child(divider));
            let button_label = match mode {
                StarsBoxMode::Admin => tr(keys::BOX_OK).to_string(),
                StarsBoxMode::Comment => tr_with(
                    keys::PAID_COMMENT_BUTTON,
                    &[("stars", &format!("\u{2605}{}", group_digits(stars)))],
                ),
                StarsBoxMode::Reaction => tr_with(
                    keys::PAID_REACTION_BUTTON,
                    &[("stars", &format!("\u{2605}{}", group_digits(stars)))],
                ),
            };
            body = body.child(
                Button::new("live-story-stars-submit")
                    .label(button_label)
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_live_stars_box(window, cx);
                    })),
            );
            if let Some(balance) = session.payments.hub.balance {
                body = body.child(
                    div()
                        .flex()
                        .justify_center()
                        .gap(px(4.))
                        .text_xs()
                        .text_color(text_muted())
                        .child(format!("\u{2605} {}", group_digits(balance.stars)))
                        .child(tr(keys::CREDITS_BALANCE_ME)),
                );
            }
            dialog
                .overlay(true)
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    pub(super) fn build_live_delete_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LiveStoryDelete, |this, _, cx| {
                this.stories.live.delete = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(delete) = this.stories.live.delete.clone() else {
                return dialog.on_close(on_close);
            };
            // Moderation choices are for your own live story and someone
            // else's comment (tdesktop `canModerate`).
            let moderate = this.live_story_admin()
                && this
                    .session()
                    .and_then(|s| s.live_story_sender())
                    .is_none_or(|me| me != delete.sender);
            let check =
                |id: &'static str,
                 label: String,
                 on: bool,
                 flip: fn(&mut super::live_state::LiveDelete, bool)| {
                    Checkbox::new(id)
                        .label(label)
                        .checked(on)
                        .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                            if let Some(delete) = this.stories.live.delete.as_mut() {
                                flip(delete, on);
                            }
                            cx.notify();
                        }))
                };
            let mut body = div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(div().text_sm().child(tr(keys::SELECTED_DELETE_SURE_THIS)));
            if moderate {
                body = body
                    .child(check(
                        "live-story-delete-spam",
                        tr(keys::REPORT_SPAM).to_string(),
                        delete.report_spam,
                        |d, on| d.report_spam = on,
                    ))
                    .child(check(
                        "live-story-delete-all",
                        tr_with(keys::DELETE_ALL_FROM_USER, &[("user", &delete.name)]),
                        delete.delete_all,
                        |d, on| d.delete_all = on,
                    ))
                    .child(check(
                        "live-story-delete-ban",
                        tr(keys::BAN_USER).to_string(),
                        delete.ban,
                        |d, on| d.ban = on,
                    ));
            }
            body = body.child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("live-story-delete-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stories.live.delete = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("live-story-delete-confirm")
                            .label(tr(keys::BOX_DELETE))
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete_live_comment(cx);
                            })),
                    ),
            );
            dialog
                .overlay(true)
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The live story's Stars box.
    LiveStoryStars => DialogSpec::new(
        3852,
        |app| app.stories.live.stars_box.is_some(),
        QuillApp::build_live_stars_dialog,
    )
    .closes(|app, _, cx| app.close_live_stars_box(cx)),
    /// Deleting a live story comment.
    LiveStoryDelete => DialogSpec::new(
        3853,
        |app| app.stories.live.delete.is_some(),
        QuillApp::build_live_delete_dialog,
    ),
}
