//! Message widgets' requests to TDLib: a query's answer or failure and a
//! public data poll land in `bots.widget_answers` for the UI to hand to
//! the widget's runtime.
use crate::message_widget::{QueryFailure, QueryOutcome, WidgetAnswer, WidgetKey, poll_period};
use crate::state::*;
use crate::telegram::envelope::BotsPayload;

/// The widget message a pending widget request was made for.
fn widget_key(pending: &PendingRequest) -> Option<WidgetKey> {
    Some(WidgetKey {
        chat_id: pending.chat_id?,
        message_id: pending.around_message_id?,
    })
}

impl Session {
    /// `widgetQueryAnswer` / `widgetPublicData` for our own request.
    pub(crate) fn apply_widget_answer(
        &mut self,
        payload: BotsPayload,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else { return };
        let Some(key) = widget_key(pending) else {
            return;
        };
        let answer = match (payload, pending.purpose) {
            (
                BotsPayload::WidgetQueryAnswer { data },
                RequestPurpose::Bots(BotsPurpose::GetWidgetQueryAnswer { token }),
            ) => WidgetAnswer::Query {
                key,
                token,
                outcome: QueryOutcome::Answered(data),
            },
            (
                BotsPayload::WidgetPublicData {
                    data,
                    poll_period: period,
                },
                RequestPurpose::Bots(BotsPurpose::GetWidgetPublicData),
            ) => WidgetAnswer::PublicData {
                key,
                data: Some((data, poll_period(period))),
            },
            _ => return,
        };
        self.bots.widget_answers.push(answer);
    }

    /// A widget request failed: the widget hears why (a query) or polls
    /// again a period later (public data). True when it was one.
    pub(crate) fn apply_widget_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let Some(pending) = pending else { return false };
        let answer = match pending.purpose {
            RequestPurpose::Bots(BotsPurpose::GetWidgetQueryAnswer { token }) => {
                widget_key(pending).map(|key| WidgetAnswer::Query {
                    key,
                    token,
                    outcome: QueryOutcome::Failed(QueryFailure::of(err)),
                })
            }
            RequestPurpose::Bots(BotsPurpose::GetWidgetPublicData) => {
                widget_key(pending).map(|key| WidgetAnswer::PublicData { key, data: None })
            }
            _ => return false,
        };
        self.bots.widget_answers.extend(answer);
        true
    }

    /// A deleted message that was a widget: the UI drops its storage
    /// (Telegram Desktop's `DeleteMessageStorage`).
    pub(crate) fn forget_deleted_widget(&mut self, chat_id: ChatId, message_id: MessageId) {
        let was_widget = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| {
                matches!(
                    message.content,
                    crate::telegram::envelope::MessageContent::Widget(_)
                )
            });
        if was_widget {
            self.bots.widget_answers.push(WidgetAnswer::Deleted {
                key: WidgetKey {
                    chat_id,
                    message_id,
                },
            });
        }
    }
}
