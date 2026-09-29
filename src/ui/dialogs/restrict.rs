use super::super::*;

/// Slice G1: restrict/ban dialog (`setChatMemberStatus`, schema
/// 1.8.67, line 13592). `banned_until_days`: 0 = forever; otherwise
/// the Unix timestamp sent is now + days * 86400.
pub struct RestrictDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) user_id: i64,
    pub(crate) ban: bool,
    pub(crate) banned_until_days: i32,
    pub(crate) permissions: ChatPermissions,
}

impl RestrictDialog {
    pub(crate) fn new(chat_id: ChatId, user_id: i64, ban: bool, current: ChatPermissions) -> Self {
        Self {
            chat_id,
            user_id,
            ban,
            banned_until_days: 0,
            permissions: current,
        }
    }
}
