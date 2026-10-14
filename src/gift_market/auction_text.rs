//! The wording of the auction boxes and the chat-list bar (tdesktop
//! `boxes/star_gift_auction_box.cpp`, `lng_auction_*`).

use super::plural;
use super::{AuctionPhase, AuctionRound, AuctionState};
use crate::premium_hub::group_digits;

/// Seconds from `now` until `date`, never negative.
pub fn seconds_left(date: i32, now: i64) -> i64 {
    (i64::from(date) - now).max(0)
}

/// Whole minutes left, rounded up (tdesktop `MinutesLeftTillValue`).
pub fn minutes_left(date: i32, now: i64) -> i64 {
    (seconds_left(date, now) + 59) / 60
}

/// tdesktop `NiceCountdownText`: "1:05:09" or "5:09".
pub fn nice_countdown(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    let hours = minutes / 60;
    if hours > 0 {
        format!("{hours}:{:02}:{:02}", minutes % 60, seconds % 60)
    } else {
        format!("{minutes}:{:02}", seconds % 60)
    }
}

/// "★ 1,250" for the minimum-bid block; 1M and up in short form
/// (tdesktop `FormatCountToShort`).
pub fn short_stars(count: i64) -> String {
    if count >= 1_000_000 {
        let tenths = count / 100_000;
        if tenths % 10 == 0 || count >= 10_000_000 {
            format!("{}M", count / 1_000_000)
        } else {
            format!("{}.{}M", tenths / 10, tenths % 10)
        }
    } else {
        group_digits(count)
    }
}

/// The two lines of the info box's button (tdesktop
/// `SetAuctionButtonCountdownText`); `place` is the send box's variant.
pub fn join_button(state: &AuctionState, now: i64, place: bool) -> (String, String) {
    let preview = !state.started(now);
    let left = if preview {
        seconds_left(state.start_date(), now)
    } else {
        minutes_left(state.end_date(), now)
    };
    if state.finished(now) || (!preview && left <= 0) {
        return ("OK".into(), String::new());
    }
    if preview {
        return (
            "Place an Early Bid".into(),
            format!("starts in {}", nice_countdown(left)),
        );
    }
    let title = if place { "Place a Bid" } else { "Join Auction" };
    let (hours, minutes) = (left / 60, left % 60);
    let time = if hours > 0 {
        format!("{hours} h {minutes} m")
    } else {
        format!("{minutes} m")
    };
    (title.into(), format!("{time} left"))
}

/// The bid box title (`lng_auction_bid_title` / `_early`).
pub fn bid_title(started: bool) -> &'static str {
    if started {
        "Place a Bid"
    } else {
        "Place an Early Bid"
    }
}

/// "Top 5 bidders will win" (`lng_auction_bid_subtitle`).
pub fn bid_subtitle(per_round: i32) -> String {
    if per_round == 1 {
        "Top 1 bidder will win".into()
    } else {
        format!(
            "Top {} bidders will win",
            group_digits(i64::from(per_round))
        )
    }
}

/// The toast after a bid: title and text.
pub fn bid_done_toast(increased: bool, per_round: i32) -> (&'static str, String) {
    (
        if increased {
            "Your bid has been increased"
        } else {
            "Your bid has been placed"
        },
        format!(
            "If you fall below the top {}, your bid will roll over to the next round.",
            group_digits(i64::from(per_round))
        ),
    )
}

/// The rows of the auction info table: label and value.
pub fn info_rows(
    state: &AuctionState,
    now: i64,
    date: impl Fn(i32) -> String,
) -> Vec<(String, String)> {
    let preview = !state.started(now);
    let finished = state.finished(now);
    let mut rows = vec![
        (
            if preview {
                "Starts"
            } else if finished {
                "First Sale"
            } else {
                "Started"
            }
            .to_string(),
            date(state.start_date()),
        ),
        (
            if finished { "Last Sale" } else { "Ends" }.to_string(),
            date(state.end_date()),
        ),
    ];
    match &state.phase {
        AuctionPhase::Active(active) if preview => {
            rows.push(("Quantity".into(), group_digits(i64::from(state.total()))));
            rows.push((
                "Rounds".into(),
                group_digits(i64::from(active.total_rounds)),
            ));
            for (index, round) in active.rounds.iter().enumerate() {
                let next = active
                    .rounds
                    .get(index + 1)
                    .map_or(active.total_rounds.saturating_add(1), |next| next.number);
                rows.push(round_row(round, next));
            }
        }
        AuctionPhase::Active(active) => {
            rows.push((
                "Current Round".into(),
                format!(
                    "{} of {}",
                    group_digits(i64::from(active.round)),
                    group_digits(i64::from(active.total_rounds))
                ),
            ));
            rows.push((
                "Availability".into(),
                format!(
                    "{} of {} left",
                    group_digits(i64::from(active.left)),
                    group_digits(i64::from(state.total()))
                ),
            ));
        }
        AuctionPhase::Finished(finished) => {
            if finished.average_price > 0 {
                rows.push((
                    "Average Price".into(),
                    format!("★{}", group_digits(finished.average_price)),
                ));
            }
        }
    }
    rows
}

/// "Round 1" / "Rounds 2-5" and "10 minutes each" (+ late-bid extension).
fn round_row(round: &AuctionRound, next_number: i32) -> (String, String) {
    let exact = next_number == round.number.saturating_add(1);
    let label = if exact {
        format!("Round {}", round.number)
    } else {
        format!("Rounds {}-{}", round.number, next_number.saturating_sub(1))
    };
    let duration = duration_text(round.duration, !exact);
    let value = if round.top_winner_count > 0 && round.extend_time > 0 {
        format!(
            "{duration} + {} for late bids in top {}",
            duration_text(round.extend_time, false),
            round.top_winner_count
        )
    } else {
        duration
    };
    (label, value)
}

/// "10 minutes", or "10 minutes each" for a range of rounds.
fn duration_text(seconds: i32, each: bool) -> String {
    let (count, one, other) = if seconds % 3600 == 0 {
        (seconds / 3600, "hour", "hours")
    } else if seconds % 60 == 0 {
        (seconds / 60, "minute", "minutes")
    } else {
        (seconds, "second", "seconds")
    };
    let text = plural(i64::from(count), one, other);
    if each { format!("{text} each") } else { text }
}

/// "{amount} is the average sale price for {gift} gifts."
pub fn average_tooltip(average: i64, title: &str) -> String {
    format!(
        "★{} is the average sale price for {title} gifts.",
        group_digits(average)
    )
}

/// "3 items bought" (`lng_auction_bought`), without the emoji and arrow.
pub fn bought_link(count: i32) -> String {
    plural(i64::from(count), "item bought", "items bought")
}

/// "3 Items Bought" (`lng_auction_bought_title`).
pub fn bought_title(count: i32) -> String {
    plural(i64::from(count), "Item Bought", "Items Bought")
}

/// "View 12 Variants" (`lng_auction_preview_variants`).
pub fn variants_link(count: i32) -> String {
    format!("View {}", plural(i64::from(count), "Variant", "Variants"))
}

/// The about box's three feature rows: title and text.
pub fn about_features(per_round: i32, rounds: i32) -> [(String, String); 3] {
    let per = i64::from(per_round);
    [
        (
            plural_words(per, "Top", "Bidder", "Bidders"),
            format!(
                "{} dropped in {} to the {} by bid amount.",
                plural(per, "gift is", "gifts are"),
                plural(i64::from(rounds), "round", "rounds"),
                plural_words(per, "top", "bidder", "bidders")
            ),
        ),
        (
            "Bid Carryover".into(),
            format!(
                "If your bid leaves the top {}, it will automatically join the next round.",
                group_digits(per)
            ),
        ),
        (
            "Missed Bidders".into(),
            "If your bid doesn't win after the final round, your Stars will be fully refunded."
                .into(),
        ),
    ]
}

/// The send box's about text for an auction gift
/// (`lng_auction_about_top_short`), without the "Learn more" link.
pub fn send_about(per_round: i32) -> String {
    let per = i64::from(per_round);
    format!(
        "{} dropped to the {} by bid amount.",
        plural(per, "gift is", "gifts are"),
        plural_words(per, "top", "bidder", "bidders")
    )
}

/// "Top 5 Bidders": a lead word, the count, then the noun.
fn plural_words(count: i64, lead: &str, one: &str, other: &str) -> String {
    format!("{lead} {}", plural(count, one, other))
}

/// The chat-list bar: title, text, whether the text is red, and the
/// button (countdown for one auction, "View" for several).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuctionBar {
    pub title: String,
    pub text: String,
    pub outbid: bool,
    pub button: String,
}

/// tdesktop `ActiveAuctionsTitle` / `ActiveAuctionsState` /
/// `ActiveAuctionsButton`; `None` without active auctions.
pub fn auction_bar(list: &[AuctionState], now: i64) -> Option<AuctionBar> {
    let first = list.first()?;
    let winning = |state: &AuctionState| {
        let position = state.my_position();
        (position <= state.per_round()).then_some(position)
    };
    if list.len() == 1 {
        let (text, outbid) = match winning(first) {
            Some(1) => ("You're winning (1st place).".to_string(), false),
            Some(n) => (format!("You're winning ({n}th place)."), false),
            None => ("You've been outbid.".to_string(), true),
        };
        return Some(AuctionBar {
            title: "Active Auction".into(),
            text,
            outbid,
            button: nice_countdown(seconds_left(first.next_round_at(), now)),
        });
    }
    let outbid = list.iter().filter(|state| winning(state).is_none()).count();
    let text = if outbid == list.len() {
        "You've been outbid in all of them.".to_string()
    } else if outbid > 0 {
        format!("You've been outbid in {outbid} of them.")
    } else {
        "You're winning in all of them.".to_string()
    };
    Some(AuctionBar {
        title: plural(list.len() as i64, "Active Auction", "Active Auctions"),
        text,
        outbid: outbid > 0,
        button: "View".into(),
    })
}

/// One row of the "Active Auctions" list: "Round 2 of 10" and "Your bid
/// ★500 is ranked #3" / "Your bid ★500 is outbid".
pub fn active_row(state: &AuctionState) -> (String, String, bool) {
    let (round, total) = state
        .active()
        .map_or((0, 0), |active| (active.round, active.total_rounds));
    let position = state.my_position();
    let outbid = position > state.per_round();
    let stars = format!("★{}", group_digits(state.my_stars()));
    let text = if outbid {
        format!("Your bid {stars} is outbid")
    } else {
        format!("Your bid {stars} is ranked #{position}")
    };
    (format!("Round {round} of {total}"), text, outbid)
}

/// The tile's badge and price line for an auction gift (tdesktop
/// `lng_gift_stars_auction*`): badge "auction" or "soon", line "Join" or
/// "View".
pub fn auction_tile(start_date: i32, sold_out: bool, now: i64) -> (&'static str, &'static str) {
    let upcoming = i64::from(start_date) > now;
    let badge = if upcoming { "soon" } else { "auction" };
    let line = if sold_out || upcoming { "View" } else { "Join" };
    (badge, line)
}

/// The link preview button (tdesktop `history_view_web_page.cpp`).
pub fn preview_button(start_date: i32, end_date: i32, now: i64) -> &'static str {
    if i64::from(end_date) <= now {
        "View Results"
    } else if i64::from(start_date) > now {
        "View"
    } else {
        "Join"
    }
}

/// The "Change Recipient" text.
pub fn change_recipient_text(from: Option<&str>, to: Option<&str>) -> String {
    let already = match from {
        None => "You've already placed a bid on this gift for yourself.".to_string(),
        Some(name) => format!("You've already placed a bid on this gift for {name}."),
    };
    let change = match to {
        None => "Do you want to raise your bid and change the recipient to yourself?".to_string(),
        Some(name) => format!("Do you want to raise your bid and change the recipient to {name}?"),
    };
    format!("{already} {change}")
}
