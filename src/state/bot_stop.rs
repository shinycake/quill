//! Where Stop stands for a streaming bot reply.

/// The stage of the Stop button of a pending bot message. Pressing Stop
/// again leaves `Failed`; the request's answer or loss leaves `Stopping`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotStopStage {
    /// Not stopped: Stop may be pressed.
    Running,
    /// Stop is in flight as this request.
    Stopping(crate::ids::RequestId),
    /// The last Stop was refused or failed; it may be pressed again.
    Failed,
    /// TDLib confirmed the stop.
    Stopped,
}
