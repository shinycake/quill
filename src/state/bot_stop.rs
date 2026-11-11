//! Where Stop stands for a streaming bot reply.

/// The stage of the Stop button of a pending bot message. "Stopping" is the
/// request in flight (the bar reads it from the request registry); sending
/// Stop again leaves `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotStopStage {
    /// Not stopped: Stop may be pressed.
    Running,
    /// The last Stop was refused or failed; it may be pressed again.
    Failed,
    /// TDLib confirmed the stop.
    Stopped,
}
