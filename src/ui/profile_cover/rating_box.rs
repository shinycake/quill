//! The Stars rating box a click on the rating pill opens (tdesktop
//! `AboutRatingBox`, `ui/controls/stars_rating.cpp`): the level bar, what
//! the rating says about the person and what changes it.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::bold_marked;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{Key, keys, tr, tr_with, trn, trn_decimal, trn_with};
use quill::stars_rating::{PendingRating, StarsRating, bubble_count};
use std::cell::RefCell;
use std::rc::Rc;

/// One row of "What affects it": the icon, the title, and the text with an
/// Added or Deducted tag in place of tdesktop's `{emoji}` badge.
struct Effect {
    icon: IconName,
    title: Key,
    about: Key,
    added: bool,
}

const EFFECTS: [Effect; 3] = [
    Effect {
        icon: IconName::Gift,
        title: keys::STARS_TITLE_GIFTS_TELEGRAM,
        about: keys::STARS_ABOUT_GIFTS_TELEGRAM,
        added: true,
    },
    Effect {
        icon: IconName::Users,
        title: keys::STARS_TITLE_GIFTS_USERS,
        about: keys::STARS_ABOUT_GIFTS_USERS,
        added: true,
    },
    Effect {
        icon: IconName::Undo2,
        title: keys::STARS_TITLE_REFUNDS,
        about: keys::STARS_ABOUT_REFUNDS,
        added: false,
    },
];

/// What the open box shows.
struct RatingView {
    /// The person's first name; `None` on your own profile ("your").
    name: Option<String>,
    rating: StarsRating,
    /// Your own rating's pending change (TDLib sends it for you only).
    pending: Option<PendingRating>,
    /// The pending rating is on the bar ("Future Rating").
    preview: bool,
}

impl RatingView {
    fn shown(&self) -> StarsRating {
        match self.pending {
            Some(pending) if self.preview => pending.value,
            _ => self.rating,
        }
    }
}

impl QuillApp {
    /// The person and rating of the open box; `None` closes it.
    fn rating_box_target(&self) -> Option<RatingView> {
        let user_id = self.profile_cover.rating_box?;
        let session = self.session()?;
        let ratings = session.user_full_info(user_id)?.ratings;
        let is_self = session.my_user_id == Some(user_id);
        let name = (!is_self).then(|| {
            session
                .user(user_id)
                .map(|user| user.first_name.clone())
                .unwrap_or_default()
        });
        Some(RatingView {
            name,
            rating: ratings.rating?,
            pending: ratings.pending.filter(|_| is_self),
            preview: self.profile_cover.rating_preview,
        })
    }

    pub(super) fn close_rating_box(&mut self, cx: &mut Context<Self>) {
        self.profile_cover.rating_box = None;
        self.profile_cover.rating_preview = false;
        cx.notify();
    }

    pub(in crate::ui) fn build_rating_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StarsRating, |this, _, cx| {
                this.close_rating_box(cx)
            });
        let target = app.read(cx).rating_box_target();
        app.update(cx, |_, cx| {
            let body = target
                .map(|view| rating_body(&view, cx))
                .unwrap_or_else(|| div().into_any_element());
            let footer = div().flex().w_full().child(
                div()
                    .flex_1()
                    .debug_selector(|| "rating-box-understood".to_string())
                    .child(
                        Button::new("rating-box-understood")
                            .label(tr(keys::STARS_RATING_UNDERSTOOD))
                            .icon(IconName::Check)
                            .primary()
                            .w_full()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_rating_box(cx);
                                this.close_kit_dialog_if_done(DialogKind::StarsRating, window, cx);
                            })),
                    ),
            );
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
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

fn rating_body(view: &RatingView, cx: &mut Context<QuillApp>) -> AnyElement {
    let theme = cx.theme();
    let (muted, danger, accent) = (theme.muted_foreground, theme.danger, theme.primary);
    let shown = view.shown();
    let bar = shown.bar();
    let negative = shown.is_negative();
    let name = view.name.as_deref().map(super::unmarked);
    let negative_text = match &name {
        None if shown.rating < 0 => trn_decimal(keys::STARS_RATING_NEGATIVE_YOUR, -shown.rating),
        _ => tr_with(
            keys::STARS_RATING_NEGATIVE,
            &[("name", name.as_deref().unwrap_or_default())],
        ),
    };
    let about = match &name {
        Some(name) => tr_with(keys::STARS_RATING_ABOUT, &[("name", name)]),
        None => tr(keys::STARS_RATING_ABOUT_YOUR).to_string(),
    };
    let title = if view.preview && view.pending.is_some() {
        keys::STARS_RATING_FUTURE
    } else {
        keys::STARS_RATING_TITLE
    };
    let level = |level: Option<i32>| level.map(|level| trn(keys::BOOST_LEVEL, level));
    let scale = bar.stars.max(bar.next_stars);
    let count = (!(negative && bar.stars == 0)).then(|| {
        let mut text = bubble_count(bar.stars, scale);
        if bar.next_stars > 0 {
            text.push('/');
            text.push_str(&bubble_count(bar.next_stars, scale));
        }
        text
    });
    div()
        .id("rating-box")
        .flex()
        .flex_col()
        .gap_3()
        .pr_6()
        .child(
            div()
                .flex()
                .justify_center()
                .items_center()
                .gap_1()
                .text_color(if negative { danger } else { accent })
                .font_semibold()
                .child(Icon::new(if negative {
                    IconName::TriangleAlert
                } else {
                    IconName::StarFill
                }))
                .children(count),
        )
        .child(
            Progress::new("rating-box-bar")
                .value(bar.ratio * 100.)
                .color(if negative { danger } else { accent })
                .accessibility_label(tr(keys::STARS_RATING_TITLE)),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .text_xs()
                .text_color(muted)
                .child(div().children(level(bar.left_level)))
                .child(
                    div().child(
                        level(bar.right_level)
                            .unwrap_or_else(|| tr(keys::STARS_RATING_NEGATIVE_LABEL).to_string()),
                    ),
                ),
        )
        .when(negative, |this| {
            this.child(
                div()
                    .text_color(danger)
                    .text_center()
                    .child(bold_marked(&negative_text)),
            )
        })
        .child(
            div()
                .pt_2()
                .text_center()
                .text_lg()
                .font_semibold()
                .child(tr(title)),
        )
        .children(
            view.pending
                .map(|pending| pending_row(pending, view.rating, view.preview, cx)),
        )
        .child(
            div()
                .text_center()
                .text_color(muted)
                .child(bold_marked(&about)),
        )
        .children(EFFECTS.iter().map(|effect| effect_row(effect, cx)))
        .into_any_element()
}

/// "The rating will update in N days. M points are pending." and the
/// Preview / Back toggle (tdesktop `aboutPending`).
fn pending_row(
    pending: PendingRating,
    current: StarsRating,
    preview: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let points = pending.points_over(current);
    let when = trn(
        keys::STARS_RATING_UPDATES,
        pending.days_left(quill::local_time::now_unix()),
    );
    let grouped = quill::premium_hub::group_digits(points);
    let text = trn_with(
        keys::STARS_RATING_PENDING,
        points,
        &[("count", &grouped), ("when", &when), ("link", "")],
    );
    let toggle = if preview {
        keys::STARS_RATING_PENDING_BACK
    } else {
        keys::STARS_RATING_PENDING_PREVIEW
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .text_sm()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .children(
            text.lines()
                .map(|line| div().child(line.trim().to_string())),
        )
        .child(
            div()
                .debug_selector(|| "rating-box-preview".to_string())
                .child(
                    Button::new("rating-box-preview")
                        .label(tr_with(toggle, &[("arrow", "›")]))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.profile_cover.rating_preview = !this.profile_cover.rating_preview;
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
}

fn effect_row(effect: &Effect, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let (tag, label) = if effect.added {
        (Tag::success(), keys::STARS_RATING_ADDED)
    } else {
        (Tag::danger(), keys::STARS_RATING_DEDUCTED)
    };
    let about = tr(effect.about).replace("{emoji}", "");
    div()
        .flex()
        .items_start()
        .gap_3()
        .child(
            div()
                .pt_0p5()
                .text_color(cx.theme().primary)
                .child(Icon::new(effect.icon)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .child(div().text_sm().font_semibold().child(tr(effect.title)))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_1p5()
                        .text_sm()
                        .text_color(muted)
                        .child(
                            div()
                                .flex_none()
                                .child(tag.xsmall().child(tr(label).to_uppercase())),
                        )
                        .child(div().flex_1().min_w_0().child(about.trim().to_string())),
                ),
        )
        .into_any_element()
}
