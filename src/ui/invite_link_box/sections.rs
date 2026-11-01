//! The link box's sections, top to bottom as tdesktop lays them out: the
//! monthly fee, the time limit, the member limit, admin approval and the
//! link name.
#![deny(clippy::indexing_slicing)]

use super::{CustomBox, InviteLinkBox};
use crate::ui::app::QuillApp;
use crate::ui::os_open::open_link_in;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::input::Input;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::invite_link_form::{
    DAY, Expire, ExpireOption, HOUR, InviteLinkForm, STARS_USD_RATE_DEFAULT, SubscriptionToggle,
    Usage, UsageOption, expire_options, stars_amount, stars_usd, usage_options,
};
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::now_unix;
use quill::premium_hub::group_digits;

/// `lng_group_invite_subscription_about_url`.
const STARS_TERMS_URL: &str = "https://telegram.org/tos/stars";

fn heading(text: &str, cx: &App) -> Div {
    div()
        .pt_2()
        .text_sm()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(text.to_string())
}

fn about(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// tdesktop `FormatExpireDate` and the preset labels.
pub(super) fn expire_label(option: ExpireOption) -> String {
    match option {
        ExpireOption::Custom => tr(keys::GROUP_INVITE_EXPIRE_CUSTOM).to_string(),
        ExpireOption::Value(Expire::Never) => tr(keys::GROUP_INVITE_EXPIRE_NEVER).to_string(),
        ExpireOption::Value(Expire::After(secs)) if secs < DAY => trn(keys::HOURS, secs / HOUR),
        ExpireOption::Value(Expire::After(secs)) if secs < 7 * DAY => trn(keys::DAYS, secs / DAY),
        ExpireOption::Value(Expire::After(secs)) => trn(keys::WEEKS, secs / (7 * DAY)),
        ExpireOption::Value(Expire::At(date)) => crate::ui::giveaway_info_ui::date_time_text(date),
    }
}

pub(super) fn usage_label(option: UsageOption) -> String {
    match option {
        UsageOption::Custom => tr(keys::GROUP_INVITE_USAGE_CUSTOM).to_string(),
        UsageOption::Value(Usage::Any) => tr(keys::GROUP_INVITE_USAGE_ANY).to_string(),
        UsageOption::Value(Usage::Limit(count)) => group_digits(i64::from(count)),
    }
}

/// tdesktop `RequestApprovalAbout` (the guard bot line aside: TDLib does
/// not name one).
fn approval_about(form: &InviteLinkForm) -> &'static str {
    let group = form.facts.group;
    let key = match (form.approval_locked(), form.request_approval, group) {
        (true, _, true) => keys::GROUP_INVITE_ABOUT_APPROVE_DISABLED,
        (true, _, false) => keys::GROUP_INVITE_ABOUT_APPROVE_DISABLED_CHANNEL,
        (false, true, true) => keys::GROUP_INVITE_ABOUT_APPROVE,
        (false, true, false) => keys::GROUP_INVITE_ABOUT_APPROVE_CHANNEL,
        (false, false, true) => keys::GROUP_INVITE_ABOUT_NO_APPROVE,
        (false, false, false) => keys::GROUP_INVITE_ABOUT_NO_APPROVE_CHANNEL,
    };
    tr(key)
}

impl QuillApp {
    pub(super) fn invite_link_box_body(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link_box = self.invite_link_box()?;
        let form = &link_box.form;
        let mut body = div().id("invite-link-box").flex().flex_col().gap_2();
        if form.offers_subscription() {
            body = body.child(self.invite_link_subscription(link_box, cx));
        }
        if form.shows_limits() {
            body = body.child(invite_link_expire(form, cx));
        }
        if form.shows_usage() {
            body = body.child(invite_link_usage(form, cx));
        }
        if form.shows_approval() {
            body = body.child(invite_link_approval(form, cx));
        }
        body = body
            .child(div().pt_2().child(Input::new(&link_box.name_input)))
            .child(about(tr(keys::GROUP_INVITE_LABEL_ABOUT), cx));
        Some(body.into_any_element())
    }

    fn invite_link_subscription(&self, link_box: &InviteLinkBox, cx: &mut Context<Self>) -> Div {
        let form = &link_box.form;
        let mut block = div().flex().flex_col().gap_2().child(
            Switch::new("invite-link-subscription")
                .label(tr(keys::GROUP_INVITE_SUBSCRIPTION))
                .checked(form.subscription)
                .on_click(cx.listener(|this, _, _, cx| {
                    let mut locked = false;
                    this.edit_invite_link_form(cx, |form| {
                        locked = form.toggle_subscription() == SubscriptionToggle::Locked;
                    });
                    if locked {
                        this.connection
                            .set_note(StatusNote::info(tr(keys::GROUP_INVITE_SUBSCRIPTION_TOAST)));
                    }
                })),
        );
        if form.subscription {
            let rate = self
                .session()
                .and_then(|s| s.groups.stars_usd_rate)
                .unwrap_or(STARS_USD_RATE_DEFAULT);
            let stars = stars_amount(
                &link_box.stars_input.read(cx).value(),
                self.invite_link_stars_max(),
            );
            let price = stars_usd(stars, rate)
                .map(|cost| tr_with(keys::GROUP_INVITE_SUBSCRIPTION_PRICE, &[("cost", &cost)]));
            block = block.child(
                Input::new(&link_box.stars_input)
                    .prefix(crate::ui::earn::star_icon(14.))
                    .disabled(form.locked_stars.is_some())
                    .suffix(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .children(price),
                    ),
            );
        }
        let link = tr_with(
            keys::GROUP_INVITE_SUBSCRIPTION_ABOUT_LINK,
            &[("emoji", "›")],
        );
        let text = tr_with(keys::GROUP_INVITE_SUBSCRIPTION_ABOUT, &[("link", &link)]);
        let start = text.find(&link).unwrap_or(text.len());
        let range = start..start.saturating_add(link.len()).min(text.len());
        block.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    InteractiveText::new(
                        "invite-link-subscription-about",
                        StyledText::new(text).with_highlights(vec![(
                            range.clone(),
                            HighlightStyle {
                                color: Some(cx.theme().link),
                                ..Default::default()
                            },
                        )]),
                    )
                    .on_click(vec![range], {
                        let app = cx.weak_entity();
                        move |_, _, cx| open_link_in(&app, STARS_TERMS_URL, cx)
                    }),
                ),
        )
    }
}

fn invite_link_expire(form: &InviteLinkForm, cx: &mut Context<QuillApp>) -> Div {
    let options = expire_options(form.expire, now_unix());
    let selected = options
        .iter()
        .position(|option| *option == ExpireOption::Value(form.expire));
    let radios: Vec<Radio> = options
        .iter()
        .map(|option| Radio::new(0).label(expire_label(*option)))
        .collect();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(heading(tr(keys::GROUP_INVITE_EXPIRE_TITLE), cx))
        .child(
            RadioGroup::vertical("invite-link-expire")
                .children(radios)
                .selected_index(selected)
                .on_click(cx.listener(move |this, index: &usize, window, cx| {
                    match options.get(*index) {
                        Some(ExpireOption::Value(expire)) => {
                            let expire = *expire;
                            this.edit_invite_link_form(cx, |form| form.expire = expire);
                        }
                        Some(ExpireOption::Custom) => this.open_invite_link_custom_date(window, cx),
                        None => {}
                    }
                })),
        )
        .child(about(tr(keys::GROUP_INVITE_EXPIRE_ABOUT), cx))
}

fn invite_link_usage(form: &InviteLinkForm, cx: &mut Context<QuillApp>) -> Div {
    let options = usage_options(form.usage);
    let selected = options
        .iter()
        .position(|option| *option == UsageOption::Value(form.usage));
    let radios: Vec<Radio> = options
        .iter()
        .map(|option| Radio::new(0).label(usage_label(*option)))
        .collect();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(heading(tr(keys::GROUP_INVITE_USAGE_TITLE), cx))
        .child(
            RadioGroup::vertical("invite-link-usage")
                .children(radios)
                .selected_index(selected)
                .on_click(cx.listener(move |this, index: &usize, window, cx| {
                    match options.get(*index) {
                        Some(UsageOption::Value(usage)) => {
                            let usage = *usage;
                            this.edit_invite_link_form(cx, |form| form.usage = usage);
                        }
                        Some(UsageOption::Custom) => this.open_invite_link_custom_limit(window, cx),
                        None => {}
                    }
                })),
        )
        .child(about(tr(keys::GROUP_INVITE_USAGE_ABOUT), cx))
}

fn invite_link_approval(form: &InviteLinkForm, cx: &mut Context<QuillApp>) -> Div {
    let label = if form.facts.group {
        tr(keys::GROUP_INVITE_REQUEST_APPROVE)
    } else {
        tr(keys::GROUP_INVITE_REQUEST_APPROVE_CHANNEL)
    };
    div()
        .flex()
        .flex_col()
        .gap_2()
        .pt_2()
        .child(
            Switch::new("invite-link-approval")
                .label(label)
                .checked(form.request_approval)
                .disabled(form.approval_locked())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.edit_invite_link_form(cx, InviteLinkForm::toggle_approval);
                })),
        )
        .child(about(approval_about(form), cx))
}

impl CustomBox {
    pub(super) fn is_date(&self) -> bool {
        matches!(self, Self::Date(_))
    }
}
