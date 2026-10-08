use serde_json::Value;

/// Batch 8: `ChatActionBar` (TDLib 1.8.67, `schema/td_api.tl:3667-3690`) —
/// the strip tdesktop shows above a chat's history (`ContactStatus`).
/// `chatActionBarReportUnrelatedLocation` is not in this schema; an unknown
/// constructor parses to `None`, so a newer TDLib never shows a wrong bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatActionBar {
    /// Group or channel that can be reported as spam.
    ReportSpam { can_unarchive: bool },
    /// Recently created group; members can be invited.
    InviteMembers,
    /// Private or secret chat with a stranger: add, block or report.
    ReportAddBlock { can_unarchive: bool },
    /// Private or secret chat; the other user can be added as a contact.
    AddContact,
    /// Mutual contact; the viewer's phone number can be shared.
    SharePhoneNumber,
    /// Private chat with an admin of a chat the viewer asked to join.
    JoinRequest {
        title: String,
        is_channel: bool,
        request_date: i32,
    },
}

impl ChatActionBar {
    /// Whether the bar's close button may send `removeChatActionBar`. The
    /// join-request notice has none in tdesktop (it opens an explanation).
    pub fn is_dismissible(&self) -> bool {
        !matches!(self, ChatActionBar::JoinRequest { .. })
    }

    /// `can_unarchive` of the spam/block variants: the chat was archived
    /// automatically and the bar offers "Unarchive".
    pub fn can_unarchive(&self) -> bool {
        matches!(
            self,
            ChatActionBar::ReportSpam {
                can_unarchive: true
            } | ChatActionBar::ReportAddBlock {
                can_unarchive: true
            }
        )
    }
}

/// Parse `chat.action_bar` / `updateChatActionBar.action_bar`. Null,
/// absent or an unknown constructor is `None` (no bar).
pub(crate) fn parse_chat_action_bar(value: Option<&Value>) -> Option<ChatActionBar> {
    let value = value?;
    let can_unarchive = value
        .get("can_unarchive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match value.get("@type").and_then(Value::as_str)? {
        "chatActionBarReportSpam" => Some(ChatActionBar::ReportSpam { can_unarchive }),
        "chatActionBarInviteMembers" => Some(ChatActionBar::InviteMembers),
        "chatActionBarReportAddBlock" => Some(ChatActionBar::ReportAddBlock { can_unarchive }),
        "chatActionBarAddContact" => Some(ChatActionBar::AddContact),
        "chatActionBarSharePhoneNumber" => Some(ChatActionBar::SharePhoneNumber),
        "chatActionBarJoinRequest" => Some(ChatActionBar::JoinRequest {
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            is_channel: value
                .get("is_channel")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            request_date: value
                .get("request_date")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        _ => None,
    }
}
