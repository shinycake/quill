//! An incoming 1:1 call that rings while another call is active.

/// The second call's place in the swap flow. Only one second call is
/// tracked; further incoming calls while one is go to the busy queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SecondCall {
    #[default]
    None,
    /// The swap prompt is showing: answer (ending the current call) or
    /// decline.
    Prompt {
        call_id: i32,
        user_id: i64,
        is_video: bool,
    },
    /// "End current & answer" was chosen and the current call's
    /// `discardCall` went out. The driver sends `acceptCall` once the
    /// active call's terminal update lands (TDLib allows one active call).
    AcceptAfterHangup {
        call_id: i32,
        user_id: i64,
        is_video: bool,
    },
}

impl SecondCall {
    /// The second call's id, in either step.
    pub fn call_id(&self) -> Option<i32> {
        match *self {
            Self::None => None,
            Self::Prompt { call_id, .. } | Self::AcceptAfterHangup { call_id, .. } => Some(call_id),
        }
    }

    /// The prompt's `(call_id, user_id, is_video)` while it shows.
    pub fn prompt(&self) -> Option<(i32, i64, bool)> {
        match *self {
            Self::Prompt {
                call_id,
                user_id,
                is_video,
            } => Some((call_id, user_id, is_video)),
            _ => None,
        }
    }

    /// Close the prompt and return it; a queued accept is left alone.
    pub fn take_prompt(&mut self) -> Option<(i32, i64, bool)> {
        let prompt = self.prompt()?;
        *self = Self::None;
        Some(prompt)
    }

    /// The queued accept's `(call_id, user_id, is_video)`, taking it.
    pub fn take_queued_accept(&mut self) -> Option<(i32, i64, bool)> {
        let Self::AcceptAfterHangup {
            call_id,
            user_id,
            is_video,
        } = *self
        else {
            return None;
        };
        *self = Self::None;
        Some((call_id, user_id, is_video))
    }

    /// The hang-up the queued accept waited for failed: the current call
    /// goes on, so the swap prompt comes back.
    pub fn back_to_prompt(&mut self) {
        if let Self::AcceptAfterHangup {
            call_id,
            user_id,
            is_video,
        } = *self
        {
            *self = Self::Prompt {
                call_id,
                user_id,
                is_video,
            };
        }
    }
}
