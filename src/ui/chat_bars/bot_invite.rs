//! The bar of a chat with a bot someone invited the user to (TDLib 1.8.71
//! `chatActionBarBotInvite`, tdesktop `ContactStatus` `Type::BotInvite`):
//! "Block bot" and "Report Spam" over "{user} invited you to this bot
//! (owned by {owner})". Report reports the inviter: `reportChat` with no
//! option and no messages, as TDLib documents for this bar.
use super::*;

impl QuillApp {
    pub(super) fn bot_invite_bar(
        &self,
        chat_id: ChatId,
        bot_id: i64,
        inviter_user_id: i64,
        owner_user_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let inviter = self.contact_display_name(inviter_user_id).to_string();
        let about = if owner_user_id != 0 && owner_user_id != inviter_user_id {
            let owner = self.contact_display_name(owner_user_id).to_string();
            tr_with(
                keys::NEW_CONTACT_BOT_INVITED_OWNER,
                &[("user", &inviter), ("owner", &owner)],
            )
        } else {
            tr_with(keys::ACTION_BOT_INVITED_YOU, &[("user", &inviter)])
        };
        let block = Self::bar_button(
            "action-bar-block".into(),
            tr(keys::NEW_CONTACT_BLOCK_BOT),
            true,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_block_bar_dialog(chat_id, bot_id, None, cx);
        }));
        let report = Self::bar_button("action-bar-report".into(), tr(keys::REPORT_SPAM), true)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.confirm_report_bot_inviter(chat_id, inviter_user_id, window, cx);
            }));
        self.bar_shell("chat-action-bar", cx)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(block)
                            .child(report),
                    )
                    .child(
                        div()
                            .debug_selector(|| "bot-invite-about".into())
                            .text_xs()
                            .text_color(text_muted())
                            .text_center()
                            .child(about),
                    ),
            )
            .child(
                Button::new("action-bar-close")
                    .icon(gpui_kit::assets::IconName::X)
                    .ghost()
                    .small()
                    .tooltip(tr(keys::Q_CHAT_HIDE))
                    .accessibility_label(tr(keys::Q_CHAT_HIDE))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.bar_dismiss(chat_id, cx);
                    })),
            )
            .into_any_element()
    }

    /// tdesktop `showBotInviteReportBox`: confirm, then report the inviter.
    /// The box is the window's: OK reports from the chat that window shows.
    fn confirm_report_bot_inviter(
        &mut self,
        chat_id: ChatId,
        inviter_user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inviter = self.contact_display_name(inviter_user_id).to_string();
        let text = tr_with(keys::REPORT_SPAM_SURE_BOT_INVITER, &[("user", &inviter)]);
        let scoped = self.scoped_app(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let scoped = scoped.clone();
            alert
                .title(tr(keys::REPORT_SPAM))
                .description(text.clone())
                .ok_text(tr(keys::REPORT_SPAM_OK))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = scoped.update(cx, |this, cx| {
                        this.start_bar_work(
                            chat_id,
                            BarKind::Report,
                            |driver| BarWork::start_report(driver, chat_id, false),
                            cx,
                        );
                    });
                    true
                })
        });
    }
}
