//! Giveaways: the answers of `getGiveawayInfo`, the payment options for a
//! new giveaway, prepaid giveaways in `chatBoostStatus`, and the summary
//! of a giveaway message (tdesktop `boxes/gift_premium_box.cpp`
//! `GiveawayInfoBox` and `info/channel_statistics/boosts/`).
use super::*;
use serde_json::Value;

/// A redeemable gift code. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct SecretCode(pub String);

impl std::fmt::Debug for SecretCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretCode(..)")
    }
}

/// `GiveawayPrize` (schema `td_api.tl:2529`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveawayPrize {
    /// `giveawayPrizePremium`.
    Premium { month_count: i32 },
    /// `giveawayPrizeStars`.
    Stars { star_count: i64 },
}

impl GiveawayPrize {
    pub(crate) fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::Premium { month_count: 0 };
        };
        match value.get("@type").and_then(Value::as_str) {
            Some("giveawayPrizeStars") => Self::Stars {
                star_count: int53_or_zero(value.get("star_count")),
            },
            _ => Self::Premium {
                month_count: json_i32(value.get("month_count"), 0),
            },
        }
    }

    pub fn is_stars(&self) -> bool {
        matches!(self, Self::Stars { .. })
    }
}

/// The part of a giveaway message the "About this giveaway" box needs
/// (`messageGiveaway` or `messageGiveawayWinners`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiveawaySummary {
    /// The chat the giveaway boosts.
    pub boosted_chat_id: i64,
    /// Message to ask `getGiveawayInfo` about; 0 means the message the
    /// card sits on (a `messageGiveawayWinners` names the original one).
    pub info_message_id: i64,
    /// Other chats users must have joined.
    pub additional_chat_count: i32,
    /// When winners are (or were) selected.
    pub selection_date: i32,
    pub only_new_members: bool,
    pub has_public_winners: bool,
    pub prize_description: String,
    pub winner_count: i32,
    pub unclaimed_count: i32,
    pub prize: GiveawayPrize,
    /// The card belongs to the winners announcement.
    pub finished: bool,
    pub was_refunded: bool,
}

impl GiveawaySummary {
    /// `messageGiveaway`.
    pub(crate) fn parse_start(value: &Value) -> Self {
        let params = value.get("parameters").unwrap_or(&Value::Null);
        Self {
            boosted_chat_id: int53_or_zero(params.get("boosted_chat_id")),
            info_message_id: 0,
            additional_chat_count: params
                .get("additional_chat_ids")
                .and_then(Value::as_array)
                .map_or(0, |ids| ids.len().min(i32::MAX as usize) as i32),
            selection_date: json_i32(params.get("winners_selection_date"), 0),
            only_new_members: json_bool(params.get("only_new_members"), false),
            has_public_winners: json_bool(params.get("has_public_winners"), false),
            prize_description: json_field_str(params, "prize_description"),
            winner_count: json_i32(value.get("winner_count"), 0),
            unclaimed_count: 0,
            prize: GiveawayPrize::parse(value.get("prize")),
            finished: false,
            was_refunded: false,
        }
    }

    /// `messageGiveawayWinners`.
    pub(crate) fn parse_results(value: &Value) -> Self {
        Self {
            boosted_chat_id: int53_or_zero(value.get("boosted_chat_id")),
            info_message_id: int53_or_zero(value.get("giveaway_message_id")),
            additional_chat_count: json_i32(value.get("additional_chat_count"), 0),
            selection_date: json_i32(value.get("actual_winners_selection_date"), 0),
            only_new_members: json_bool(value.get("only_new_members"), false),
            has_public_winners: true,
            prize_description: json_field_str(value, "prize_description"),
            winner_count: json_i32(value.get("winner_count"), 0),
            unclaimed_count: json_i32(value.get("unclaimed_prize_count"), 0),
            prize: GiveawayPrize::parse(value.get("prize")),
            finished: true,
            was_refunded: json_bool(value.get("was_refunded"), false),
        }
    }

    /// Winners plus prizes nobody claimed: how many prizes the admins
    /// bought (tdesktop `winnersCount + unclaimedCount`).
    pub fn quantity(&self) -> i32 {
        self.winner_count.saturating_add(self.unclaimed_count)
    }
}

/// `GiveawayParticipantStatus` (schema `td_api.tl:2490`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiveawayParticipantStatus {
    Eligible,
    Participating,
    /// The user was in the chat before the giveaway began.
    AlreadyWasMember {
        joined_chat_date: i32,
    },
    /// The user administers one of the chats.
    Administrator {
        chat_id: i64,
    },
    DisallowedCountry {
        country_code: String,
    },
}

/// `GiveawayInfo` (schema `td_api.tl:2509`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiveawayInfoData {
    /// `giveawayInfoOngoing`.
    Ongoing {
        creation_date: i32,
        status: GiveawayParticipantStatus,
        /// The giveaway ended and the results are being prepared.
        is_ended: bool,
    },
    /// `giveawayInfoCompleted`.
    Completed {
        creation_date: i32,
        actual_winners_selection_date: i32,
        was_refunded: bool,
        is_winner: bool,
        winner_count: i32,
        activation_count: i32,
        /// Empty unless the current user won a Premium code.
        gift_code: SecretCode,
        /// 0 unless the current user won Stars.
        won_star_count: i64,
    },
}

impl GiveawayInfoData {
    pub(crate) fn parse(value: &Value) -> Self {
        if value.get("@type").and_then(Value::as_str) == Some("giveawayInfoCompleted") {
            return Self::Completed {
                creation_date: json_i32(value.get("creation_date"), 0),
                actual_winners_selection_date: json_i32(
                    value.get("actual_winners_selection_date"),
                    0,
                ),
                was_refunded: json_bool(value.get("was_refunded"), false),
                is_winner: json_bool(value.get("is_winner"), false),
                winner_count: json_i32(value.get("winner_count"), 0),
                activation_count: json_i32(value.get("activation_count"), 0),
                gift_code: SecretCode(json_field_str(value, "gift_code")),
                won_star_count: int53_or_zero(value.get("won_star_count")),
            };
        }
        let status = value.get("status").unwrap_or(&Value::Null);
        let status = match status.get("@type").and_then(Value::as_str) {
            Some("giveawayParticipantStatusParticipating") => {
                GiveawayParticipantStatus::Participating
            }
            Some("giveawayParticipantStatusAlreadyWasMember") => {
                GiveawayParticipantStatus::AlreadyWasMember {
                    joined_chat_date: json_i32(status.get("joined_chat_date"), 0),
                }
            }
            Some("giveawayParticipantStatusAdministrator") => {
                GiveawayParticipantStatus::Administrator {
                    chat_id: int53_or_zero(status.get("chat_id")),
                }
            }
            Some("giveawayParticipantStatusDisallowedCountry") => {
                GiveawayParticipantStatus::DisallowedCountry {
                    country_code: json_field_str(status, "user_country_code"),
                }
            }
            _ => GiveawayParticipantStatus::Eligible,
        };
        Self::Ongoing {
            creation_date: json_i32(value.get("creation_date"), 0),
            status,
            is_ended: json_bool(value.get("is_ended"), false),
        }
    }

    /// The giveaway is over or being closed out (tdesktop `Finished` and
    /// `Refunded` states).
    pub fn is_finished(&self) -> bool {
        match self {
            Self::Ongoing { is_ended, .. } => *is_ended,
            Self::Completed { .. } => true,
        }
    }
}

/// `premiumGiveawayPaymentOption` (schema `td_api.tl:1387`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumGiveawayOption {
    pub currency: String,
    /// In the smallest units of the currency.
    pub amount: i64,
    pub winner_count: i32,
    pub month_count: i32,
}

/// `starGiveawayPaymentOption` (schema `td_api.tl:1429`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarGiveawayOption {
    pub currency: String,
    pub amount: i64,
    pub star_count: i64,
    pub yearly_boost_count: i32,
    /// Allowed numbers of winners.
    pub winner_counts: Vec<i32>,
    pub is_default: bool,
    pub is_additional: bool,
}

pub(crate) fn parse_premium_giveaway_options(value: &Value) -> Vec<PremiumGiveawayOption> {
    value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| PremiumGiveawayOption {
            currency: json_field_str(option, "currency"),
            amount: int53_or_zero(option.get("amount")),
            winner_count: json_i32(option.get("winner_count"), 0),
            month_count: json_i32(option.get("month_count"), 0),
        })
        .collect()
}

pub(crate) fn parse_star_giveaway_options(value: &Value) -> Vec<StarGiveawayOption> {
    value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| StarGiveawayOption {
            currency: json_field_str(option, "currency"),
            amount: int53_or_zero(option.get("amount")),
            star_count: int53_or_zero(option.get("star_count")),
            yearly_boost_count: json_i32(option.get("yearly_boost_count"), 0),
            winner_counts: option
                .get("winner_options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|w| json_i32(w.get("winner_count"), 0))
                .collect(),
            is_default: json_bool(option.get("is_default"), false),
            is_additional: json_bool(option.get("is_additional"), false),
        })
        .collect()
}

/// `prepaidGiveaway` (schema `td_api.tl:7265`): a giveaway already paid
/// for and waiting to be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPrepaidGiveaway {
    pub id: i64,
    pub winner_count: i32,
    pub prize: GiveawayPrize,
    /// Boosts the chat gets when the giveaway runs.
    pub boost_count: i32,
    pub payment_date: i32,
}

pub(crate) fn parse_prepaid_giveaways(value: Option<&Value>) -> Vec<ParsedPrepaidGiveaway> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(ParsedPrepaidGiveaway {
                id: int64(item.get("id"))?,
                winner_count: json_i32(item.get("winner_count"), 0),
                prize: GiveawayPrize::parse(item.get("prize")),
                boost_count: json_i32(item.get("boost_count"), 0),
                payment_date: json_i32(item.get("payment_date"), 0),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ongoing_info_reads_every_status() {
        let info = GiveawayInfoData::parse(&json!({
            "@type": "giveawayInfoOngoing", "creation_date": 100, "is_ended": false,
            "status": {"@type": "giveawayParticipantStatusAlreadyWasMember", "joined_chat_date": 50}
        }));
        assert_eq!(
            info,
            GiveawayInfoData::Ongoing {
                creation_date: 100,
                status: GiveawayParticipantStatus::AlreadyWasMember {
                    joined_chat_date: 50
                },
                is_ended: false,
            }
        );
        let admin = GiveawayInfoData::parse(&json!({
            "@type": "giveawayInfoOngoing", "creation_date": 1,
            "status": {"@type": "giveawayParticipantStatusAdministrator", "chat_id": "-1001"}
        }));
        assert!(matches!(
            admin,
            GiveawayInfoData::Ongoing {
                status: GiveawayParticipantStatus::Administrator { chat_id: -1001 },
                ..
            }
        ));
        let country = GiveawayInfoData::parse(&json!({
            "@type": "giveawayInfoOngoing",
            "status": {"@type": "giveawayParticipantStatusDisallowedCountry", "user_country_code": "DE"}
        }));
        assert!(matches!(
            country,
            GiveawayInfoData::Ongoing {
                status: GiveawayParticipantStatus::DisallowedCountry { .. },
                ..
            }
        ));
        let eligible = GiveawayInfoData::parse(&json!({"@type": "giveawayInfoOngoing"}));
        assert!(!eligible.is_finished());
    }

    #[test]
    fn completed_info_keeps_the_code_out_of_debug() {
        let info = GiveawayInfoData::parse(&json!({
            "@type": "giveawayInfoCompleted", "creation_date": 10,
            "actual_winners_selection_date": 20, "was_refunded": false, "is_winner": true,
            "winner_count": 5, "activation_count": 2, "gift_code": "SECRETSLUG", "won_star_count": 0
        }));
        let GiveawayInfoData::Completed {
            gift_code,
            winner_count,
            activation_count,
            ..
        } = &info
        else {
            panic!("completed");
        };
        assert_eq!(gift_code.0, "SECRETSLUG");
        assert_eq!((*winner_count, *activation_count), (5, 2));
        assert!(!format!("{info:?}").contains("SECRETSLUG"));
        assert!(info.is_finished());
    }

    #[test]
    fn message_summaries_read_start_and_results() {
        let start = GiveawaySummary::parse_start(&json!({
            "winner_count": 3,
            "prize": {"@type": "giveawayPrizePremium", "month_count": 6},
            "parameters": {"boosted_chat_id": -1002, "additional_chat_ids": [-1, -2],
                "winners_selection_date": 999, "only_new_members": true,
                "has_public_winners": false, "prize_description": "A mug"}
        }));
        assert_eq!(start.boosted_chat_id, -1002);
        assert_eq!(start.additional_chat_count, 2);
        assert_eq!(start.prize, GiveawayPrize::Premium { month_count: 6 });
        assert!(start.only_new_members && !start.finished);
        let results = GiveawaySummary::parse_results(&json!({
            "boosted_chat_id": -1002, "giveaway_message_id": 77, "additional_chat_count": 1,
            "actual_winners_selection_date": 1200, "winner_count": 4, "unclaimed_prize_count": 1,
            "prize": {"@type": "giveawayPrizeStars", "star_count": 500}
        }));
        assert_eq!(results.info_message_id, 77);
        assert_eq!(results.quantity(), 5);
        assert!(results.prize.is_stars() && results.finished);
    }

    #[test]
    fn payment_options_and_prepaid_giveaways_parse() {
        let premium = parse_premium_giveaway_options(&json!({"options": [
            {"currency": "USD", "amount": 1500, "winner_count": 5, "month_count": 3}]}));
        assert_eq!(premium[0].amount, 1500);
        assert_eq!(premium[0].winner_count, 5);
        let stars = parse_star_giveaway_options(&json!({"options": [
            {"currency": "USD", "amount": 999, "star_count": 1000, "yearly_boost_count": 6,
             "winner_options": [{"winner_count": 5, "won_star_count": 200}, {"winner_count": 10}],
             "is_default": true}]}));
        assert_eq!(stars[0].winner_counts, vec![5, 10]);
        assert!(stars[0].is_default && !stars[0].is_additional);
        let prepaid = parse_prepaid_giveaways(Some(&json!([
            {"id": "123456789012", "winner_count": 7,
             "prize": {"@type": "giveawayPrizePremium", "month_count": 12}, "boost_count": 28,
             "payment_date": 5},
            {"winner_count": 1}
        ])));
        assert_eq!(prepaid.len(), 1);
        assert_eq!(prepaid[0].id, 123456789012);
        assert_eq!(prepaid[0].boost_count, 28);
    }
}
