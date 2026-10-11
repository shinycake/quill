//! The words of the "About this giveaway" box (tdesktop
//! `boxes/gift_premium_box.cpp` `GiveawayInfoBox`, strings `lng_prizes_*`).
use crate::deep_link_types::gift_duration;
use crate::telegram::envelope::{
    GiveawayInfoData, GiveawayParticipantStatus, GiveawayPrize, GiveawaySummary,
};

/// What the wording needs from the app.
pub struct GiveawayTextContext<'a> {
    /// Name of the boosted channel or group.
    pub first_channel: String,
    /// The boosted chat is a supergroup.
    pub group: bool,
    /// Name of an administered chat, by id.
    pub chat_title: &'a dyn Fn(i64) -> String,
    /// "October 6" (tdesktop `langDayOfMonthFull`).
    pub day: &'a dyn Fn(i32) -> String,
    /// "October 6 at 14:05" (tdesktop `langDateTime`).
    pub date_time: &'a dyn Fn(i32) -> String,
}

/// The box, as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutText {
    pub title: &'static str,
    /// The highlighted line about the current user's prize.
    pub result: Option<String>,
    /// Whether `result` announces a win.
    pub won: bool,
    pub paragraphs: Vec<String>,
    /// Shown in a box of its own when the prizes were refunded.
    pub refund_notice: Option<String>,
}

fn count_of(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn stars(n: i64) -> String {
    count_of(n, "Star", "Stars")
}

fn months_phrase(months: i32) -> String {
    format!("for {}", gift_duration(i64::from(months), 0))
}

/// The text for `summary` (the giveaway card) and `info` (the answer to
/// `getGiveawayInfo`).
pub fn about_text(
    summary: &GiveawaySummary,
    info: &GiveawayInfoData,
    ctx: &GiveawayTextContext<'_>,
) -> AboutText {
    let finished = info.is_finished();
    let (refunded, winner_count, activated, gift_code, won_stars) = match info {
        GiveawayInfoData::Completed {
            was_refunded,
            winner_count,
            activation_count,
            gift_code,
            won_star_count,
            ..
        } => (
            *was_refunded,
            *winner_count,
            *activation_count,
            !gift_code.0.is_empty(),
            *won_star_count,
        ),
        GiveawayInfoData::Ongoing { .. } => (false, 0, 0, false, 0),
    };
    let ch = &ctx.first_channel;
    let group = ctx.group;
    let many = summary.additional_chat_count > 0;
    let quantity = i64::from(summary.quantity());
    let count = if winner_count > 0 {
        i64::from(winner_count)
    } else {
        quantity
    };

    let (result, won) = if gift_code {
        (
            Some("You won a prize in this giveaway \u{1F3C6}".to_string()),
            true,
        )
    } else if won_stars > 0 {
        (
            Some(format!(
                "You won {} in this giveaway \u{1F3C6}",
                stars(won_stars)
            )),
            true,
        )
    } else if matches!(info, GiveawayInfoData::Completed { .. }) && !refunded {
        (
            Some("You didn't win a prize in this giveaway.".to_string()),
            false,
        )
    } else {
        (None, false)
    };

    let audience = if group {
        "its members"
    } else {
        "its followers"
    };
    let admins = match summary.prize {
        GiveawayPrize::Stars { star_count } => {
            format!(
                "the admins of {ch}, who acquired {} for {audience}",
                stars(star_count)
            )
        }
        GiveawayPrize::Premium { month_count } => format!(
            "the admins of {ch}, who acquired {} {} {} for {audience}",
            count_of(quantity, "Telegram Premium", "Telegram Premium"),
            if quantity == 1 {
                "subscription"
            } else {
                "subscriptions"
            },
            months_phrase(month_count),
        ),
    };
    let mut intro = if finished {
        format!("This giveaway was sponsored by {admins}.")
    } else {
        format!("This giveaway is sponsored by {admins}.")
    };

    let who = if group { "member" } else { "subscriber" };
    let also = if many {
        " and other listed groups and channels"
    } else {
        ""
    };
    let winners = if summary.only_new_members {
        let start = (ctx.date_time)(match info {
            GiveawayInfoData::Ongoing { creation_date, .. }
            | GiveawayInfoData::Completed { creation_date, .. } => *creation_date,
        });
        format!(
            "{} that joined {ch}{also} after {start}",
            count_of(count, "random user", "random users")
        )
    } else {
        format!(
            "{} of {ch}{also}",
            count_of(count, &format!("random {who}"), &format!("random {who}s"))
        )
    };

    let mut paragraphs = Vec::new();
    if !summary.prize_description.is_empty() {
        let plural = count != 1;
        intro.push_str(&format!(
            "\n\n{ch} also included {count} {} in the {}. Admins of the {} are responsible for delivering {} {}.",
            summary.prize_description,
            if plural { "prizes" } else { "prize" },
            if group { "group" } else { "channel" },
            if plural { "these" } else { "this" },
            if plural { "prizes" } else { "prize" },
        ));
    }
    let until = (ctx.day)(summary.selection_date);
    intro.push_str(&format!(
        "\n\nOn {until}, Telegram {} {winners}.",
        if finished {
            "automatically selected"
        } else {
            "will automatically select"
        }
    ));
    if activated > 0 {
        intro.push(' ');
        intro.push_str(&if activated == 1 {
            "1 winner already used their gift link.".to_string()
        } else {
            format!("{activated} of the winners already used their gift links.")
        });
    }
    paragraphs.push(intro);

    if !gift_code && !finished && !refunded {
        {
            let GiveawayInfoData::Ongoing { status, .. } = info else {
                unreachable!("an unfinished giveaway is ongoing");
            };
            let line = match status {
                GiveawayParticipantStatus::Administrator { chat_id } => {
                    let name = (ctx.chat_title)(*chat_id);
                    if group {
                        format!("You are not eligible to participate in this giveaway because you are an admin of a participating group ({name}).")
                    } else {
                        format!("You are not eligible to participate in this giveaway because you are an admin of a participating channel ({name}).")
                    }
                }
                GiveawayParticipantStatus::AlreadyWasMember { joined_chat_date } => {
                    let date = (ctx.date_time)(*joined_chat_date);
                    format!(
                        "You are not eligible to participate in this giveaway because you joined this {} on {date}, which is before the giveaway started.",
                        if group { "group" } else { "channel" }
                    )
                }
                GiveawayParticipantStatus::DisallowedCountry { .. } => {
                    "You are not eligible to participate in this giveaway because your country is not included in the terms of the giveaway.".to_string()
                }
                GiveawayParticipantStatus::Participating => {
                    if many {
                        format!("You are participating in this giveaway because you joined the channel {ch} (and other listed groups and channels).")
                    } else {
                        format!("You are participating in this giveaway because you joined the channel {ch}.")
                    }
                }
                GiveawayParticipantStatus::Eligible => {
                    if many {
                        format!("To take part in this giveaway please join channel {ch} and other listed groups and channels before {until}.")
                    } else {
                        format!("To take part in this giveaway please join the channel {ch} before {until}.")
                    }
                }
            };
            paragraphs.push(line);
        }
    }

    AboutText {
        title: if finished {
            "Giveaway ended"
        } else {
            "About this giveaway"
        },
        result,
        won,
        paragraphs,
        refund_notice: refunded.then(|| {
            if group {
                "The group canceled the prizes by refunding the payment for them.".to_string()
            } else {
                "The channel canceled the prizes by refunding the payment for them.".to_string()
            }
        }),
    }
}
