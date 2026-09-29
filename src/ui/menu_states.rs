use super::*;

/// M1: right-click context menu state — the target message plus the
/// window position where the menu opens (`MouseDownEvent.position` is in
/// window coordinates, so the panel renders absolute at that point).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageMenuState {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub position: Point<Pixels>,
}

/// Slice CL1: right-click chat-row context menu target + window
/// position (same pattern as `MessageMenuState`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatMenuState {
    pub chat_id: ChatId,
    pub position: Point<Pixels>,
}
