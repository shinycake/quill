//! TDLib objects of the revenue and affiliate screens.

use super::*;

/// Answers and updates the payments domain hands to `state::earn`.
#[derive(Debug, Clone, PartialEq)]
pub enum EarnPayload {
    /// `chatRevenueStatistics`.
    TonStats(TonRevenueStats),
    /// `chatRevenueTransactions`.
    TonTransactions(TonTxPage),
    /// `starRevenueStatistics`.
    StarStats(StarRevenueStats),
    /// `foundAffiliatePrograms`.
    FoundPrograms(FoundPrograms),
    /// `connectedAffiliateProgram`.
    Connected(ConnectedProgram),
    /// `connectedAffiliatePrograms`.
    ConnectedList(ConnectedPrograms),
    /// `updateChatRevenueAmount`.
    UpdateTonAmounts { chat_id: i64, amounts: TonAmounts },
    /// `updateStarRevenueStatus`.
    UpdateStarStatus {
        owner: EarnPeer,
        status: StarRevenueStatus,
    },
}

/// Parse `type_name`, or `None` for a type this module does not own.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<EarnPayload> {
    Some(match type_name {
        "chatRevenueStatistics" => EarnPayload::TonStats(TonRevenueStats::parse(value)),
        "chatRevenueTransactions" => EarnPayload::TonTransactions(TonTxPage::parse(value)),
        "starRevenueStatistics" => EarnPayload::StarStats(StarRevenueStats::parse(value)),
        "foundAffiliatePrograms" => EarnPayload::FoundPrograms(FoundPrograms::parse(value)),
        "connectedAffiliateProgram" => EarnPayload::Connected(ConnectedProgram::parse(value)),
        "connectedAffiliatePrograms" => EarnPayload::ConnectedList(ConnectedPrograms::parse(value)),
        "updateChatRevenueAmount" => EarnPayload::UpdateTonAmounts {
            chat_id: int(value.get("chat_id")),
            amounts: TonAmounts::parse(value.get("revenue_amount")),
        },
        "updateStarRevenueStatus" => EarnPayload::UpdateStarStatus {
            owner: crate::premium_hub::sender_of(value.get("owner_id"))
                .map(EarnPeer::from_sender)?,
            status: StarRevenueStatus::parse(value.get("status")),
        },
        _ => return None,
    })
}

/// The affiliate switches of `getApplicationConfig`'s `jsonValueObject`
/// (tdesktop `AppConfig::starrefJoinAllowed` / `starrefSetupAllowed`,
/// both off unless the server says otherwise).
pub fn starref_flags(config: &Value) -> (bool, bool) {
    let flag = |key: &str| {
        config
            .get("members")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|m| m.get("key").and_then(Value::as_str) == Some(key))
            .and_then(|m| m.get("value"))
            .filter(|v| v.get("@type").and_then(Value::as_str) == Some("jsonValueBoolean"))
            .and_then(|v| v.get("value"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    (
        flag("starref_connect_allowed"),
        flag("starref_program_allowed"),
    )
}
