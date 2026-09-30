//! message reactions UI.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::DEFAULT_EMOJI_REACTIONS;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_reactions(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(RequestPurpose::AddMessageReaction, Some(ChatId(11)));
    let jsons = [
        format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    pub(super) fn open_reaction_picker(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let can_react = self.session().is_some_and(|session| {
            session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))
                .is_some_and(quill::state::HistoryMessage::can_react)
        });
        if !can_react {
            return;
        }
        self.pending_react = Some((chat_id, message_id));
        self.status_note = "react".into();
        cx.notify();
    }

    pub(super) fn close_reaction_picker(&mut self, cx: &mut Context<Self>) {
        self.pending_react = None;
        self.status_note = "reaction picker closed".into();
        cx.notify();
    }

    pub(super) fn toggle_emoji_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: String,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_message_reaction(chat_id, message_id, &emoji);
            self.status_note = match result {
                Ok(_) => "updating reaction…".into(),
                Err(_) => "could not update reaction".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_reaction_toggle(chat_id, message_id, &emoji);
            self.status_note = "reaction updated".into();
            cx.notify();
        }
    }

    pub(super) fn reaction_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.pending_react;
        let (chat_id, message_id) = target.unwrap_or((ChatId(0), MessageId(0)));
        let chosen: Vec<String> = self
            .session()
            .and_then(|session| {
                session
                    .histories
                    .get(&chat_id.0)
                    .and_then(|history| history.messages.get(&message_id.0))
                    .map(|message| {
                        DEFAULT_EMOJI_REACTIONS
                            .iter()
                            .filter(|emoji| message.chosen_emoji(emoji))
                            .map(|emoji| (*emoji).to_string())
                            .collect()
                    })
            })
            .unwrap_or_default();
        let mut row = div().id("reaction-emoji-row").flex().flex_wrap().gap_1();
        for emoji in DEFAULT_EMOJI_REACTIONS {
            let own = chosen.iter().any(|picked| picked == *emoji);
            let picked = (*emoji).to_string();
            row = row.child(
                Button::new(format!("react-pick-{picked}"))
                    .label(if own {
                        format!("{picked} · yours")
                    } else {
                        picked.clone()
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_emoji_reaction(chat_id, message_id, picked.clone(), cx);
                    })),
            );
        }
        div()
            .id("reaction-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("React"))
                    .child(
                        Button::new("close-reaction-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_reaction_picker(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Tap an emoji to react. Tap yours again to remove."),
            )
            .child(row)
    }
}
