//! Where Stop stands for a streaming bot reply.
use super::PendingBotMessage;

/// The stage of the Stop button of a pending bot message. Pressing Stop
/// again leaves `Failed`; the request's answer or loss leaves `Stopping`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotStopStage {
    /// Not stopped. `can_stop`: TDLib offers Stop for this draft
    /// (`updatePendingMessage.can_stop`).
    Running { can_stop: bool },
    /// Stop is in flight as this request.
    Stopping(crate::ids::RequestId),
    /// The last Stop was refused or failed; it may be pressed again.
    Failed,
    /// TDLib confirmed the stop.
    Stopped,
}

impl BotStopStage {
    /// The Stop button is offered: TDLib allows it, or a Stop is on its way
    /// or was refused and may be pressed again.
    pub fn can_stop(self) -> bool {
        match self {
            Self::Running { can_stop } => can_stop,
            Self::Stopping(_) | Self::Failed => true,
            Self::Stopped => false,
        }
    }
}

impl PendingBotMessage {
    /// The Stop button is offered for this reply.
    pub fn can_stop(&self) -> bool {
        self.stop.can_stop()
    }
}
