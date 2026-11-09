//! Parses the TDLib 1.8.70 community answers.
use super::CommunitiesPayload;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The communities domain's TDLib types; `Ok(None)` leaves `type_name`
/// to the other domains.
pub(crate) fn parse_communities_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "communities" => CommunitiesPayload::Communities {
            community_ids: parse_community_ids(value),
        },
        "communityAddChatRequests" => {
            CommunitiesPayload::AddChatRequests(parse_community_add_chat_requests(value))
        }
        "communityMemberChats" => {
            CommunitiesPayload::MemberChats(parse_community_member_chats(value))
        }
        _ => return Ok(None),
    };
    Ok(Some(EnvelopePayload::Communities(payload)))
}
