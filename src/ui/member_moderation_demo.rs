//! `ready-member-moderation` screenshot demo: the member list with its
//! menu buttons, the restrict-until picker, the remove confirmation, the
//! admin delete box and the ownership dialogs. Everything is injected
//! through the real reducers; no live Telegram. English-only text.
//! `QUILL_DEMO_MODERATION=members|restrict|ban|remove|delete|leave|pick|confirm|blocked`.

use super::app::QuillApp;
use super::dialogs::{GroupConfirmAction, MemberDialog, OwnershipStage};
use super::groups::apply_ready_group_manage;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::moderation::RestrictUntil;
use quill::state::{OwnerLookup, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{CanTransferOwnershipResult, MessageActions};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const CHAT: i64 = 61;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// The viewer owns the demo group (instead of being an admin) and a
/// member posted something spammy.
fn apply_owner_and_message(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHAT},"is_forum":false,"status":{{"@type":"chatMemberStatusCreator","is_member":true}}}}}}"#
        ),
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":7001,"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":5}},"is_outgoing":false,"date":1790000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Buy cheap followers here! t.me/cheapfollowers","entities":[]}}}}}}}}"#
        ),
    );
}

register_demos![
    // Member moderation (`QUILL_DEMO_MODERATION=members|restrict|ban|
    // remove|delete|leave|pick|confirm|blocked`; injected data, no live
    // Telegram).
    DemoSpec::chats(
        "ready-member-moderation",
        "screenshot demo — member moderation (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_member_moderation),
];

impl QuillApp {
    fn demo_setup_member_moderation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_MODERATION").unwrap_or_else(|_| "members".into());
        let chat = ChatId(CHAT);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_sink, &self.demo_seq);
            if matches!(
                mode.as_str(),
                "leave" | "pick" | "confirm" | "blocked" | "delete"
            ) {
                apply_owner_and_message(session, &self.demo_sink, &self.demo_seq);
            }
            session.ownership.can_transfer = Some(match mode.as_str() {
                "blocked" => CanTransferOwnershipResult::PasswordTooFresh {
                    retry_after: 3 * 86_400,
                },
                _ => CanTransferOwnershipResult::Ok,
            });
            session
                .ownership
                .owner_after_leaving
                .insert(CHAT, OwnerLookup::Loaded(1));
        }
        match mode.as_str() {
            "members" => {
                self.member_dialog = Some(MemberDialog::new(window, cx, chat, false));
            }
            "restrict" | "ban" => {
                self.open_restrict_dialog(chat, 5, mode == "ban", window, cx);
                let until = if mode == "ban" {
                    RestrictUntil::Week
                } else {
                    RestrictUntil::Custom(0)
                };
                self.pick_restrict_until(until, cx);
            }
            "remove" => {
                self.open_group_confirm(chat, GroupConfirmAction::RemoveMember { user_id: 5 }, cx)
            }
            "leave" => {
                self.open_owner_leave(chat, window, cx);
            }
            "pick" => self.open_transfer_ownership(chat, window, cx),
            "confirm" | "blocked" => {
                self.open_transfer_ownership(chat, window, cx);
                if let Some(dialog) = self.ownership_dialog.as_mut() {
                    dialog.stage = OwnershipStage::Confirm {
                        user_id: 5,
                        leave_after: false,
                    };
                }
            }
            "delete" => {
                let message = self.session().and_then(|s| {
                    s.histories
                        .get(&CHAT)
                        .and_then(|h| h.messages.get(&7001))
                        .cloned()
                });
                if let Some(message) = message {
                    let properties = MessageActions {
                        can_be_deleted_for_all_users: true,
                        can_report_supergroup_spam: true,
                        ..MessageActions::default()
                    };
                    let offer = self.moderation_offer(chat, &message, Some(properties));
                    if let Some(confirm) = quill::composer::DeleteConfirm::for_message(
                        chat,
                        MessageId(7001),
                        false,
                        false,
                    ) {
                        // The kit dialog layer exists once the window has
                        // rendered; open the alert a moment after setup.
                        cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1200))
                                .await;
                            let _ = this.update_in(cx, |this, window, cx| {
                                this.open_delete_dialog_with(confirm, offer, window, cx);
                            });
                        })
                        .detach();
                    }
                }
            }
            _ => {}
        }
        self.status_note = format!("screenshot demo — member moderation: {mode}");
        cx.notify();
    }
}
