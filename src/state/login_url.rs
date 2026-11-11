//! The login-URL button press: its context and where it stands.
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::LoginUrlInfo;

/// B1: context of an in-flight login-URL button press. Kept while
/// `getLoginUrlInfo` (and then `getLoginUrl`, after consent) is in flight,
/// so an error degrades the button to a plain URL-button press (schema
/// 1.8.67 doc on `getLoginUrl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUrlRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub button_id: i64,
    /// The button's raw URL — the plain-URL fallback.
    pub raw_url: String,
    /// Pressed by a step the agent UI driver injected: what the answer
    /// opens runs as part of that step (`guard::with_injecting`).
    pub from_agent: bool,
}

/// Where a login-URL button press stands: one stage, moved only by the
/// answer (or error) of the request it waits for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoginUrlStage {
    #[default]
    Idle,
    /// `getLoginUrlInfo` (then `getLoginUrl`, after consent) is out under
    /// `request`; an error degrades the button to its raw URL.
    Asking {
        request: RequestId,
        press: LoginUrlRequest,
    },
    /// The answer waits for the UI, which takes it once.
    Answered {
        info: LoginUrlInfo,
        /// The press it answers; `None` once its context is spent (a
        /// refused press has only the fallback URL left).
        press: Option<LoginUrlRequest>,
    },
}

impl LoginUrlStage {
    /// A press is out or its answer is not taken yet: a second one waits.
    pub fn is_busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// A press goes out under `request`.
    pub fn ask(&mut self, request: RequestId, press: LoginUrlRequest) {
        *self = Self::Asking { request, press };
    }

    /// The press under `request` was refused (or could not be sent): the
    /// button degrades to its raw URL. Another request's error changes
    /// nothing.
    pub fn refused(&mut self, request: Option<RequestId>) {
        if let Self::Asking {
            request: asked,
            press,
        } = self
            && Some(*asked) == request
        {
            let fallback_url = std::mem::take(&mut press.raw_url);
            *self = Self::Answered {
                info: LoginUrlInfo::Failed { fallback_url },
                press: None,
            };
        }
    }

    /// `request` answered with `info`. An answer of any other request is
    /// not this press's.
    pub fn answered(&mut self, request: Option<RequestId>, info: LoginUrlInfo) {
        if let Self::Asking {
            request: asked,
            press,
        } = self
            && Some(*asked) == request
        {
            *self = Self::Answered {
                info,
                press: Some(press.clone()),
            };
        }
    }

    /// The answer, once.
    pub fn take_answer(&mut self) -> Option<(LoginUrlInfo, Option<LoginUrlRequest>)> {
        match std::mem::take(self) {
            Self::Answered { info, press } => Some((info, press)),
            other => {
                *self = other;
                None
            }
        }
    }

    /// An answer nobody took is dropped; a press still out keeps its
    /// context for the answer to come.
    pub fn forget_answer(&mut self) {
        if matches!(self, Self::Answered { .. }) {
            *self = Self::Idle;
        }
    }
}
