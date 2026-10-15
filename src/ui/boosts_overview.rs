//! The top of the boosts page: level progress, boost statistics, the
//! level feature table and, in supergroups, "Boosts needed to bypass
//! restrictions" (tdesktop `info/channel_statistics/boosts/`,
//! `ui/boxes/boost_box.cpp` and `edit_peer_permissions_box.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{UNRESTRICT_BOOST_CHOICES, UNRESTRICT_BOOST_MAX};

/// "Boost the group N times" help under the unrestrict buttons
/// (`lng_rights_boosts_about` and `_about_on`).
pub(crate) fn unrestrict_about(count: i32) -> String {
    if count > 0 {
        tr(keys::RIGHTS_BOOSTS_ABOUT_ON).into()
    } else {
        tr(keys::RIGHTS_BOOSTS_ABOUT).into()
    }
}

/// Clamp to what TDLib accepts.
pub(crate) fn unrestrict_clamped(count: i32) -> i32 {
    count.clamp(0, UNRESTRICT_BOOST_MAX)
}

impl QuillApp {
    pub(super) fn open_boost_extras(&mut self, chat_id: ChatId) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_boost_features(chat_id);
            let _ = live.driver.refresh_group_admin(chat_id);
        }
    }

    fn set_unrestrict_boosts(&mut self, chat_id: ChatId, count: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .set_unrestrict_boost_count(chat_id, unrestrict_clamped(count))
                .is_err()
        {
            self.connection.status_note = tr(keys::Q_MONEY_COULD_NOT_SAVE_THE_SETTING).into();
        }
        cx.notify();
    }

    /// Progress, statistics, feature table and the unrestrict setting.
    pub(super) fn boosts_overview(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let accent = theme.primary;
        let track = theme.muted;
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let group = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| !chat.kind.is_channel());
        let mut body = div().flex().flex_col().gap_1();
        if let Some(status) = session.groups.chat_boost_details.get(&chat_id.0) {
            let next = status.level + 1;
            body = body
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_xs()
                        .text_color(muted)
                        .child(tr_with(
                            keys::Q_MONEY_LEVEL_2,
                            &[("level", &status.level.to_string())],
                        ))
                        .child(if status.next_level_boost_count > 0 {
                            tr_with(keys::Q_MONEY_LEVEL_3, &[("next", &next.to_string())])
                        } else {
                            tr(keys::Q_MONEY_MAXIMUM_LEVEL).to_string()
                        }),
                )
                .child(
                    div().h(px(8.)).w_full().rounded_full().bg(track).child(
                        div()
                            .h_full()
                            .w(relative(status.level_fraction() as f32))
                            .rounded_full()
                            .bg(accent),
                    ),
                );
            let mut stats = vec![
                (tr(keys::BOOSTS_LEVEL), status.level.to_string()),
                (tr(keys::BOOSTS_EXISTING), status.boost_count.to_string()),
            ];
            if status.premium_member_count > 0 || status.premium_member_percentage > 0.0 {
                stats.push((
                    if group {
                        tr(keys::BOOSTS_PREMIUM_MEMBERS)
                    } else {
                        tr(keys::BOOSTS_PREMIUM_AUDIENCE)
                    },
                    status.premium_audience_text(),
                ));
            }
            if let Some(left) = status.boosts_to_next_level() {
                stats.push((tr(keys::BOOSTS_NEXT_LEVEL), left.to_string()));
            }
            for (label, value) in stats {
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(div().text_color(muted).child(label))
                        .child(value),
                );
            }
        }
        if let Some(features) = session.groups.chat_boost_features.get(&chat_id.0) {
            let table = features.table(group);
            if !table.is_empty() {
                body = body.child(
                    div()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_WHAT_EACH_LEVEL_UNLOCKS)),
                );
                for (level, lines) in table {
                    body = body.child(
                        div()
                            .flex()
                            .gap_2()
                            .text_sm()
                            .child(div().w(px(56.)).text_color(muted).child(tr_with(
                                keys::Q_MONEY_LEVEL_2,
                                &[("level", &level.to_string())],
                            )))
                            .child(div().flex_1().child(lines.join(", "))),
                    );
                }
            }
        }
        if session.chat_can_edit_unrestrict_boosts(chat_id) {
            let current = session.chat_unrestrict_boost_count(chat_id);
            let mut choices = div().flex().flex_wrap().items_center().gap_1().child({
                let b = Button::new("unrestrict-off")
                    .label(tr(keys::Q_MONEY_OFF))
                    .small();
                let b = if current != 0 { b.ghost() } else { b };
                b.on_click(cx.listener(move |this, _, _, cx| {
                    this.set_unrestrict_boosts(chat_id, 0, cx);
                }))
            });
            for value in UNRESTRICT_BOOST_CHOICES {
                choices = choices.child({
                    let b = Button::new(("unrestrict-count", value as u64))
                        .label(value.to_string())
                        .small();
                    let b = if current != value { b.ghost() } else { b };
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.set_unrestrict_boosts(chat_id, value, cx);
                    }))
                });
            }
            body = body
                .child(
                    div()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::RIGHTS_BOOSTS_NO_RESTRICT)),
                )
                .child(choices)
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(unrestrict_about(current)),
                );
        }
        body.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{unrestrict_about, unrestrict_clamped};

    #[test]
    fn clamp_matches_tdlib_range() {
        assert_eq!(unrestrict_clamped(-3), 0);
        assert_eq!(unrestrict_clamped(5), 5);
        assert_eq!(unrestrict_clamped(99), 8);
    }

    #[test]
    fn about_text_follows_the_setting() {
        assert!(unrestrict_about(0).starts_with("Turn this on"));
        assert!(unrestrict_about(2).starts_with("Choose how many"));
    }
}
