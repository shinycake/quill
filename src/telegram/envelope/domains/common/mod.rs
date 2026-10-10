//! TDLib updates and answers for options, connection state and scalar answers shared by many requests.
mod parse;

use crate::telegram::envelope::*;
use crate::text::TextEntity;
pub(crate) use parse::parse_common_payload;
use serde_json::Value;

/// Payloads for options, connection state and scalar answers shared by many requests; wrapped as
/// [`EnvelopePayload::Common`].
#[derive(Debug, Clone, PartialEq)]
pub enum CommonPayload {
    AccountExport(Value),
    /// MED4: `updateOption` (TDLib 1.8.67, `schema/td_api.tl:10926`).
    /// Only the options Quill reads are kept; everything else is still a
    /// parsed-but-ignored update (never an error).
    UpdateOption {
        name: String,
        value: OptionValue,
    },
    UpdateConnectionState(ConnectionState),
    /// `updateDiceEmojis` (schema line 11431).
    UpdateDiceEmojis {
        emojis: Vec<String>,
    },
    /// `updateFreezeState` (schema line 11347).
    UpdateFreezeState(FreezeStateUpdate),
    /// `updateSpeechRecognitionTrial` (schema line 11425).
    UpdateSpeechRecognitionTrial(SpeechTrialUpdate),
    /// `updateAgeVerificationParameters` (schema line 11351); `None` when
    /// verification is not needed.
    UpdateAgeVerificationParameters {
        parameters: Option<AgeVerificationParams>,
    },
    /// Phase C3a: `text` (schema 1.8.67, line 10071) — the
    /// `joinVideoChat` answer (join payload for tgcalls).
    Text {
        text: String,
    },
    /// Phase C3a: `httpUrl` (schema 1.8.67, line 7458) — the
    /// `getVideoChatInviteLink` answer.
    HttpUrl {
        url: String,
    },
    /// Slice msg-richtext-ai-tools: `fixedText` (TDLib 1.8.67,
    /// `schema/td_api.tl:157`) — the `fixTextWithAi` answer. `text` is
    /// the fixed text the composer applies (`diffText` is not parsed —
    /// nothing renders it; parsing what you never use is slop).
    FixedText {
        text: String,
    },
    /// Slice msg-richtext-ai-tools: bare `formattedText` (TDLib 1.8.67,
    /// `schema/td_api.tl:3046`) — the `composeTextWithAi`,
    /// `translateText` and `translateMessageText` answer. The composer
    /// applies `text` (entities are dropped: the draft is plain text,
    /// documented in the driver); translations keep `entities`.
    FormattedText {
        text: String,
        entities: Vec<TextEntity>,
    },
    /// `count` — the answer to `getChatMessageCount` (schema 1.8.67,
    /// line 10068).
    Count {
        count: i32,
    },
    /// Batch 4: `updateServiceNotification` — a server popup.
    UpdateServiceNotification {
        kind: String,
        text: String,
    },
    /// `parity:proxy-settings`: `seconds` — `pingProxy` answer.
    Seconds {
        seconds: f64,
    },
}
