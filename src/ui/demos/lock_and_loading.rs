//! Screenshot demos for round 15 of the state-machine review (area C): a
//! second call ringing in under the lock, and a Business screen opened
//! before the account's full info arrived.
use crate::ui::app::QuillApp;
use crate::ui::business::View;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::quick_replies::{QuickRepliesUpdate, QuickReplyShortcut};
use quill::state::{ActiveCall, SecondCall};
use quill::telegram::envelope::CallState;

register_demos![
    DemoSpec::chat_list("ready-lock-swap-call").setup(QuillApp::demo_lock_swap_call),
    DemoSpec::chat_list("ready-business-loading").setup(QuillApp::demo_business_loading),
];

impl QuillApp {
    /// `ready-lock-swap-call`: Quill locked during a call while a second
    /// call rings in; the lock screen shows its Decline and End & Answer.
    fn demo_lock_swap_call(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.calls.summary = None;
            session.calls.active_call = Some(ActiveCall::fresh(
                5,
                12,
                true,
                false,
                CallState::Ready,
                None,
            ));
            session.calls.second_call = SecondCall::Prompt {
                call_id: 6,
                user_id: 12,
                is_video: false,
            };
        }
        self.account.passcode.enabled = true;
        self.lock_by_passcode(cx);
        cx.notify();
    }

    /// `ready-business-loading`: the Greeting screen opened before the own
    /// account's full info arrived. `QUILL_DEMO_BUSINESS_INFO=landed`: the
    /// info lands and the next frame fills the screen in with the saved
    /// settings (`business_info_upkeep`).
    fn demo_business_loading(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let landed = std::env::var("QUILL_DEMO_BUSINESS_INFO").is_ok_and(|v| v == "landed");
        if let Some(session) = self.demo_session.as_mut() {
            session.payments.premium_option = Some(true);
            session.my_user_id = Some(777);
            session.apply_quick_replies(QuickRepliesUpdate::Shortcut(QuickReplyShortcut {
                id: 2,
                name: "hello".into(),
                message_count: 1,
            }));
        }
        self.open_business(window, cx);
        self.business_go(View::Greeting, window, cx);
        if landed && let Some(session) = self.demo_session.as_mut() {
            session.users_state.user_full_infos.insert(
                777,
                quill::state::UserFullInfoData {
                    extras: quill::telegram::envelope::UserProfileExtras {
                        business: Some(quill::business_info::BusinessInfo {
                            greeting: Some(quill::business_settings::GreetingSettings {
                                shortcut_id: 2,
                                recipients: quill::business_settings::BusinessRecipients::everyone(
                                ),
                                inactivity_days: 7,
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
        }
        cx.notify();
    }
}
