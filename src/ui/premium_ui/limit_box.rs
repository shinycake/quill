//! Premium limit boxes for pinned chats, captions, saved GIFs, joined
//! communities, public links and file size.
//!
//! tdesktop `boxes/premium_limits_box.cpp` (`SimpleLimitBox`): a line from
//! the free to the Premium value, the sentence, and "Increase Limit" next to
//! "Cancel" (an OK button for a Premium account). Folder limits keep the box
//! in `ui/folder_extras.rs`. "Increase Limit" opens the Premium page; Quill
//! never takes a payment.

use super::*;
use crate::ui::folder_share::finish_dialog;
use quill::composer::ComposerAttachment;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::promo::limit_pair;
use quill::premium_limits::{
    FileSizeCheck, LimitPair, PremiumLimitKind, check_file_size, gif_limit_reached, limit_box_text,
    line_ratio, shown_limit,
};

/// The blue-to-pink of the Premium line (tdesktop `premiumButtonBg`).
const PREMIUM_FROM: u32 = 0x6b93ff;
const PREMIUM_TO: u32 = 0xe46ace;

/// A number on the line, with its unit where it has one.
fn value_text(kind: PremiumLimitKind, value: i64) -> String {
    if kind == PremiumLimitKind::FileSize {
        tr_with(keys::Q_MONEY_GB, &[("value", &value.to_string())])
    } else {
        value.to_string()
    }
}

/// The limit now in force for this account: TDLib's option where it keeps
/// one, else the free or Premium value.
fn limit_in_force(session: &Session, kind: PremiumLimitKind, pair: LimitPair) -> i64 {
    let option = match kind {
        PremiumLimitKind::Pins => i64::from(session.chat_list.pinned_chat_count_max),
        PremiumLimitKind::PinsArchive => {
            i64::from(session.chat_list.pinned_archived_chat_count_max)
        }
        PremiumLimitKind::Caption => i64::from(session.messages.message_caption_length_max),
        _ => 0,
    };
    if option > 0 {
        option
    } else {
        shown_limit(pair, session.my_is_premium())
    }
}

/// What a box shows, resolved from the session.
struct LimitView {
    title: String,
    message: String,
    offer_upgrade: bool,
    show_line: bool,
    pair: LimitPair,
    ratio: f32,
}

fn view_of(session: &Session, kind: PremiumLimitKind, extra: i64) -> LimitView {
    let premium = session.my_is_premium();
    let possible = session.premium_possible();
    let pair = limit_pair(session.payments.hub.premium.as_ref(), kind);
    // A file over even the Premium limit: no upsell, the Premium size shown.
    let too_large = kind == PremiumLimitKind::FileSize && extra == 1;
    let (shown, is_premium, possible) = if too_large {
        (pair.premium_value, true, false)
    } else if kind == PremiumLimitKind::FileSize {
        (shown_limit(pair, premium), premium, possible)
    } else {
        (limit_in_force(session, kind, pair), premium, possible)
    };
    let text = limit_box_text(kind, shown, pair.premium_value, is_premium, possible);
    let mut message = text.message;
    if kind == PremiumLimitKind::Caption && extra > 0 {
        message.push_str(&format!(
            " Please make the caption shorter by {extra} character{}.",
            if extra == 1 { "" } else { "s" }
        ));
    }
    let current = (shown
        + if kind == PremiumLimitKind::Caption {
            extra
        } else {
            0
        })
    .clamp(
        pair.default_value,
        pair.premium_value.max(pair.default_value),
    );
    LimitView {
        title: text.title,
        message,
        offer_upgrade: text.offer_upgrade,
        show_line: possible,
        pair,
        ratio: line_ratio(kind, pair, current),
    }
}

/// The line from the free value to the Premium one.
fn limit_line(kind: PremiumLimitKind, view: &LimitView, cx: &App) -> impl IntoElement {
    let cell = || div().flex().items_center().justify_between().gap_2().px_3();
    div()
        .id("premium-limit-line")
        .flex()
        .w_full()
        .h(px(32.))
        .rounded_md()
        .overflow_hidden()
        .text_sm()
        .font_semibold()
        .child(
            cell()
                .w(relative(view.ratio))
                .bg(cx.theme().muted)
                .child(tr(keys::PREMIUM_FREE))
                .child(value_text(kind, view.pair.default_value)),
        )
        .child(
            cell()
                .flex_1()
                .text_color(rgb(0xffffff))
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(rgb(PREMIUM_FROM), 0.),
                    linear_color_stop(rgb(PREMIUM_TO), 1.),
                ))
                .child(tr(keys::AI_COMPOSE_TONE_SAVED_LIMIT_LINK))
                .child(value_text(kind, view.pair.premium_value)),
        )
}

impl QuillApp {
    /// Open the limit box for `kind` and ask for the Premium values if they
    /// are not loaded yet. `extra` is how far over the limit the action
    /// went (caption characters) or, for file size, 1 when even Premium
    /// would not take the file.
    pub(in crate::ui) fn show_premium_limit(
        &mut self,
        kind: PremiumLimitKind,
        extra: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(hub) = self.hub_mut() {
            hub.limit_box = Some((kind, extra));
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_premium();
        }
        cx.notify();
    }

    fn dismiss_premium_limit(&mut self) {
        if let Some(hub) = self.hub_mut() {
            hub.limit_box = None;
        }
    }

    /// Refuse attachments over the upload limit with the file size box.
    /// True when one was refused (nothing is sent).
    pub(in crate::ui) fn deny_oversized_attachments(
        &mut self,
        attachments: &[ComposerAttachment],
        cx: &mut Context<Self>,
    ) -> bool {
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        let worst = attachments
            .iter()
            .filter_map(|att| std::fs::metadata(&att.path).ok())
            .map(|meta| check_file_size(meta.len(), premium))
            .fold(FileSizeCheck::Fits, |worst, next| match (worst, next) {
                (FileSizeCheck::TooLarge, _) | (_, FileSizeCheck::TooLarge) => {
                    FileSizeCheck::TooLarge
                }
                (FileSizeCheck::NeedsPremium, _) | (_, FileSizeCheck::NeedsPremium) => {
                    FileSizeCheck::NeedsPremium
                }
                _ => FileSizeCheck::Fits,
            });
        match worst {
            FileSizeCheck::Fits => false,
            FileSizeCheck::NeedsPremium => {
                self.show_premium_limit(PremiumLimitKind::FileSize, 0, cx);
                true
            }
            FileSizeCheck::TooLarge => {
                self.show_premium_limit(PremiumLimitKind::FileSize, 1, cx);
                true
            }
        }
    }

    /// After "Add to GIFs" on a full list: TDLib replaced the oldest GIF,
    /// and tdesktop says so with the limit box.
    pub(in crate::ui) fn note_gif_replaced(&mut self, cx: &mut Context<Self>) {
        let full = self.session().is_some_and(|s| {
            let pair = limit_pair(s.payments.hub.premium.as_ref(), PremiumLimitKind::SavedGifs);
            s.stickers.gifs.loaded
                && gif_limit_reached(
                    s.stickers.gifs.animations.len(),
                    shown_limit(pair, s.my_is_premium()),
                )
        });
        if full {
            self.show_premium_limit(PremiumLimitKind::SavedGifs, 0, cx);
        }
    }

    pub(in crate::ui) fn build_premium_limit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PremiumLimit, |this, _, cx| {
                this.dismiss_premium_limit();
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some((kind, extra)) = this.session().and_then(|s| s.payments.hub.limit_box) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::CAPTION_LIMIT_TITLE,
                    )))
                    .on_close(on_close);
            };
            let Some(view) = this.session().map(|s| view_of(s, kind, extra)) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::CAPTION_LIMIT_TITLE,
                    )))
                    .on_close(on_close);
            };
            let mut body = div().flex().flex_col().gap_3();
            if view.show_line {
                body = body.child(limit_line(kind, &view, cx));
            }
            body = body.child(
                div()
                    .id("premium-limit-text")
                    .text_sm()
                    .child(view.message.clone()),
            );
            if kind == PremiumLimitKind::PublicLinks {
                body = body.child(this.public_links_list(cx));
            }
            let mut footer = div().flex().justify_end().gap_2();
            if view.offer_upgrade {
                footer = footer
                    .child(
                        Button::new("premium-limit-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_premium_limit();
                                cx.notify();
                                this.close_kit_dialog_if_done(DialogKind::PremiumLimit, window, cx);
                            })),
                    )
                    .child(
                        Button::new("premium-limit-increase")
                            .label(tr(keys::AI_COMPOSE_INCREASE_LIMIT))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_premium_limit();
                                this.open_premium(cx);
                                this.close_kit_dialog_if_done(DialogKind::PremiumLimit, window, cx);
                            })),
                    );
            } else {
                footer = footer.child(
                    Button::new("premium-limit-ok")
                        .label(tr(keys::BOX_OK))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_premium_limit();
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::PremiumLimit, window, cx);
                        })),
                );
            }
            finish_dialog(
                dialog,
                view.title,
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A Premium limit box (pins, caption, GIFs, communities, links, file size).
    PremiumLimit => DialogSpec::new(
        1310,
        |app| app.session().is_some_and(|s| s.payments.hub.limit_box.is_some()),
        QuillApp::build_premium_limit_dialog,
    )
    .closes(|app, _, cx| { app.dismiss_premium_limit(); cx.notify(); }),
}
