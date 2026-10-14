//! Gift auctions: the state TDLib reports and the rules of the bid box
//! (tdesktop `data/components/gift_auctions.cpp`,
//! `boxes/star_gift_auction_box.cpp`).

use super::{array, flag, int, int32, text};
use crate::gift_manage::AvailableGift;
use crate::premium_hub::upgraded::{GiftAttribute, GiftBackdrop};
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

/// tdesktop `kMaxShownBid`: the slider reaches at least this far.
pub const MAX_SHOWN_BID: i64 = 30_000;
/// The largest custom bid (`EditCustomBid`).
pub const MAX_CUSTOM_BID: i64 = 1_000_000_000;
/// Rows under "Top winners".
pub const TOP_PLACES: usize = 3;

/// `gift.auction_info` (`giftAuction`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftAuctionInfo {
    /// The slug in `t.me/auction/<id>` and the `getGiftAuctionState` key.
    pub id: String,
    pub gifts_per_round: i32,
    pub start_date: i32,
}

impl GiftAuctionInfo {
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| !v.is_null())?;
        let id = text(value, "id");
        if id.is_empty() {
            return None;
        }
        Some(Self {
            id,
            gifts_per_round: int32(value.get("gifts_per_round")).max(0),
            start_date: int32(value.get("start_date")),
        })
    }

    /// `https://t.me/auction/<id>` (the About / Copy Link / Share menu).
    pub fn link(&self) -> String {
        format!("https://t.me/auction/{}", self.id)
    }
}

/// `auctionBid`: one level of the sparse bid list, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BidLevel {
    pub stars: i64,
    pub date: i32,
    pub position: i32,
}

/// `userAuctionBid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyBid {
    pub stars: i64,
    pub date: i32,
    /// The least the next bid may be.
    pub next_bid: i64,
    /// Who receives the gift if the bid wins.
    pub owner: Option<MessageSender>,
    pub was_returned: bool,
}

/// `auctionRound`: the rules from round `number` on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionRound {
    pub number: i32,
    pub duration: i32,
    pub extend_time: i32,
    pub top_winner_count: i32,
}

/// `auctionStateActive`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActiveAuction {
    pub start_date: i32,
    pub end_date: i32,
    pub min_bid: i64,
    pub levels: Vec<BidLevel>,
    /// At most three users with the biggest bids, in `levels` order.
    pub top_bidders: Vec<i64>,
    pub rounds: Vec<AuctionRound>,
    pub round_end_date: i32,
    /// 1-based.
    pub round: i32,
    pub total_rounds: i32,
    pub distributed: i32,
    pub left: i32,
    pub acquired: i32,
    pub my: Option<MyBid>,
}

/// `auctionStateFinished`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FinishedAuction {
    pub start_date: i32,
    pub end_date: i32,
    pub average_price: i64,
    pub acquired: i32,
    pub telegram_listed: i32,
    pub fragment_listed: i32,
    pub fragment_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuctionPhase {
    Active(ActiveAuction),
    Finished(FinishedAuction),
}

/// `giftAuctionState`: the gift and its auction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuctionState {
    pub gift: AvailableGift,
    pub phase: AuctionPhase,
}

impl AuctionState {
    pub fn parse(value: &Value) -> Option<Self> {
        let gift =
            crate::gift_manage::parse_available_gift(&json!({ "gift": value.get("gift")? }))?;
        let state = value.get("state")?;
        let phase = match state.get("@type").and_then(Value::as_str)? {
            "auctionStateActive" => AuctionPhase::Active(parse_active(state)),
            "auctionStateFinished" => AuctionPhase::Finished(FinishedAuction {
                start_date: int32(state.get("start_date")),
                end_date: int32(state.get("end_date")),
                average_price: int(state.get("average_price")).max(0),
                acquired: int32(state.get("acquired_item_count")).max(0),
                telegram_listed: int32(state.get("telegram_listed_item_count")).max(0),
                fragment_listed: int32(state.get("fragment_listed_item_count")).max(0),
                fragment_url: text(state, "fragment_url"),
            }),
            _ => return None,
        };
        Some(Self { gift, phase })
    }

    pub fn gift_id(&self) -> i64 {
        self.gift.id
    }

    pub fn auction_id(&self) -> Option<&str> {
        self.gift.auction.as_ref().map(|a| a.id.as_str())
    }

    /// Gifts each round hands out (the "top N").
    pub fn per_round(&self) -> i32 {
        self.gift
            .auction
            .as_ref()
            .map_or(0, |a| a.gifts_per_round)
            .max(1)
    }

    /// The whole supply (`overall_limits.total_count`).
    pub fn total(&self) -> i32 {
        self.gift.supply.map_or(0, |(_, total)| total)
    }

    pub fn active(&self) -> Option<&ActiveAuction> {
        match &self.phase {
            AuctionPhase::Active(active) => Some(active),
            AuctionPhase::Finished(_) => None,
        }
    }

    pub fn start_date(&self) -> i32 {
        match &self.phase {
            AuctionPhase::Active(a) => a.start_date,
            AuctionPhase::Finished(f) => f.start_date,
        }
    }

    pub fn end_date(&self) -> i32 {
        match &self.phase {
            AuctionPhase::Active(a) => a.end_date,
            AuctionPhase::Finished(f) => f.end_date,
        }
    }

    /// Finished by state, or past its end date.
    pub fn finished(&self, now: i64) -> bool {
        match &self.phase {
            AuctionPhase::Finished(_) => true,
            AuctionPhase::Active(a) => i64::from(a.end_date) <= now,
        }
    }

    pub fn started(&self, now: i64) -> bool {
        i64::from(self.start_date()) <= now
    }

    pub fn my(&self) -> Option<&MyBid> {
        self.active().and_then(|a| a.my.as_ref())
    }

    /// The user's bid in Stars; 0 without one.
    pub fn my_stars(&self) -> i64 {
        self.my().map_or(0, |my| my.stars)
    }

    /// Gifts the user won (`acquired_item_count`).
    pub fn acquired(&self) -> i32 {
        match &self.phase {
            AuctionPhase::Active(a) => a.acquired,
            AuctionPhase::Finished(f) => f.acquired,
        }
    }

    /// The least bid the user may place: their next bid, or the auction's
    /// minimum, and at least 1.
    pub fn min_bid_for_me(&self) -> i64 {
        let Some(active) = self.active() else {
            return 1;
        };
        let next = active.my.as_ref().map_or(0, |my| my.next_bid);
        (if next > 0 { next } else { active.min_bid }).max(1)
    }

    /// When the current round (or the whole auction) ends.
    pub fn next_round_at(&self) -> i32 {
        match &self.phase {
            AuctionPhase::Active(a) if a.round_end_date > 0 => a.round_end_date,
            _ => self.end_date(),
        }
    }

    /// tdesktop `MyAuctionPosition`: the place the user's bid holds.
    pub fn my_position(&self) -> i32 {
        let Some(active) = self.active() else {
            return 0;
        };
        let (stars, date) = active.my.as_ref().map_or((0, 0), |my| (my.stars, my.date));
        place_for(&active.levels, stars, date)
    }

    /// Whether the user's bid wins this round.
    pub fn is_winning(&self) -> bool {
        self.my_stars() > 0 && self.my_position() <= self.per_round()
    }

    /// Bounds of the bid slider (tdesktop's `BidSliderValues`).
    pub fn slider(&self) -> BidSlider {
        let mine = self.my_stars();
        let min = self.min_bid_for_me();
        let last = self
            .active()
            .and_then(|a| a.levels.first())
            .map_or(0, |level| level.stars);
        let mut max = (min + 1).max(MAX_SHOWN_BID).max(round_ratio(mine, 6, 5));
        // `max < last * 1.05`, in integers.
        if max.saturating_mul(100) < last.saturating_mul(105) {
            max = round_ratio(last, 6, 5);
        }
        BidSlider { min, mine, max }
    }

    /// The row the user's bid would hold at `chosen` Stars and the top
    /// three places (tdesktop `AddBidPlaces`).
    pub fn bid_rows(&self, chosen: i64, my_user_id: Option<i64>) -> BidRows {
        let per_round = self.per_round();
        let Some(active) = self.active() else {
            return BidRows {
                kind: BidKind::Setting,
                position: 1,
                top: Vec::new(),
            };
        };
        let mine = self.my_stars();
        let my_date = active.my.as_ref().map_or(0, |my| my.date);
        let setting = chosen > mine;
        let me = TopBid {
            user_id: my_user_id.unwrap_or(0),
            stars: chosen,
            is_me: true,
        };
        // tdesktop `AddBidPlaces`: the others in level order, skipping the
        // user's own entry, with the user inserted at the chosen place.
        let mut top: Vec<TopBid> = Vec::new();
        let push_top = |top: &mut Vec<TopBid>, index: usize| -> bool {
            let (Some(&user_id), Some(level)) =
                (active.top_bidders.get(index), active.levels.get(index))
            else {
                return false;
            };
            if top.len() >= TOP_PLACES {
                return false;
            }
            if Some(user_id) != my_user_id {
                top.push(TopBid {
                    user_id,
                    stars: level.stars,
                    is_me: false,
                });
            }
            true
        };
        let mut position = None;
        for (index, level) in active.levels.iter().enumerate() {
            if level.stars < chosen || (!setting && level.stars == chosen && level.date >= my_date)
            {
                top.push(me);
                for rest in index..active.levels.len() {
                    if !push_top(&mut top, rest) {
                        break;
                    }
                }
                position = Some(level.position);
                break;
            }
            push_top(&mut top, index);
        }
        let position = position.unwrap_or_else(|| {
            top.push(me);
            active
                .levels
                .last()
                .map_or(0, |level| level.position)
                .saturating_add(1)
        });
        top.truncate(TOP_PLACES);
        let kind = if setting {
            BidKind::Setting
        } else if position <= per_round {
            BidKind::Winning
        } else {
            BidKind::Losing
        };
        BidRows {
            kind,
            position,
            top,
        }
    }

    /// The bubble's second line over the slider (tdesktop's `subtext`).
    pub fn bubble_subtext(&self, chosen: i64) -> String {
        let mine = self.my_stars();
        if chosen == mine {
            "your bid".into()
        } else if chosen == self.slider().max {
            "click to bid more".into()
        } else if mine > 0 && chosen > mine {
            format!(
                "+{}",
                crate::premium_hub::group_digits(chosen.saturating_sub(mine))
            )
        } else {
            String::new()
        }
    }

    /// What pressing the bid button does at `chosen` Stars.
    pub fn bid_action(&self, chosen: i64) -> BidAction {
        let mine = self.my_stars();
        if chosen <= mine {
            BidAction::Close
        } else if mine > 0 {
            BidAction::Increase(chosen)
        } else {
            BidAction::Place(chosen)
        }
    }

    /// Which box a tap on the auction opens (tdesktop
    /// `ChooseAndShowAuctionBox`). `recipient` is who the gift would go
    /// to now.
    pub fn choose_box(
        &self,
        recipient: &MessageSender,
        about_shown: bool,
        now: i64,
    ) -> AuctionBoxChoice {
        let finished = self.finished(now);
        let has_bid = self.my_stars() > 0;
        let same_owner = self
            .my()
            .and_then(|my| my.owner.as_ref())
            .is_none_or(|owner| owner == recipient);
        if has_bid && !finished && same_owner {
            AuctionBoxChoice::Bid
        } else if has_bid && !finished {
            AuctionBoxChoice::ChangeRecipient
        } else if !self.started(now) || finished || about_shown {
            AuctionBoxChoice::Info
        } else {
            AuctionBoxChoice::About
        }
    }
}

fn parse_active(state: &Value) -> ActiveAuction {
    let my = state
        .get("user_bid")
        .filter(|v| !v.is_null())
        .map(|bid| MyBid {
            stars: int(bid.get("star_count")).max(0),
            date: int32(bid.get("bid_date")),
            next_bid: int(bid.get("next_bid_star_count")).max(0),
            owner: crate::premium_hub::sender_of(bid.get("owner_id")),
            was_returned: flag(bid, "was_returned"),
        })
        .filter(|my| my.stars > 0 && !my.was_returned);
    ActiveAuction {
        start_date: int32(state.get("start_date")),
        end_date: int32(state.get("end_date")),
        min_bid: int(state.get("min_bid")).max(0),
        levels: array(state, "bid_levels")
            .map(|level| BidLevel {
                stars: int(level.get("star_count")).max(0),
                date: int32(level.get("bid_date")),
                position: int32(level.get("position")).max(0),
            })
            .collect(),
        top_bidders: array(state, "top_bidder_user_ids")
            .map(|id| int(Some(id)))
            .collect(),
        rounds: array(state, "rounds")
            .map(|round| AuctionRound {
                number: int32(round.get("number")).max(1),
                duration: int32(round.get("duration")).max(0),
                extend_time: int32(round.get("extend_time")).max(0),
                top_winner_count: int32(round.get("top_winner_count")).max(0),
            })
            .collect(),
        round_end_date: int32(state.get("current_round_end_date")),
        round: int32(state.get("current_round_number")).max(1),
        total_rounds: int32(state.get("total_round_count")).max(1),
        distributed: int32(state.get("distributed_item_count")).max(0),
        left: int32(state.get("left_item_count")).max(0),
        acquired: int32(state.get("acquired_item_count")).max(0),
        my,
    }
}

/// The place a bid of `stars` made at `date` takes among `levels`: the
/// first level it beats (a tie goes to the earlier bid), or after the last.
fn place_for(levels: &[BidLevel], stars: i64, date: i32) -> i32 {
    for level in levels {
        if level.stars < stars || (level.stars == stars && level.date >= date) {
            return level.position;
        }
    }
    levels
        .last()
        .map_or(0, |level| level.position)
        .saturating_add(1)
}

/// `round(value * num / den)` without floats.
fn round_ratio(value: i64, num: i64, den: i64) -> i64 {
    value
        .saturating_mul(num)
        .saturating_add(den / 2)
        .checked_div(den)
        .unwrap_or(0)
}

/// The bid slider's range: `mine` is the user's current bid (0 for none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BidSlider {
    pub min: i64,
    pub mine: i64,
    pub max: i64,
}

impl BidSlider {
    /// Where the slider starts: the user's bid, or the minimum.
    pub fn initial(self) -> i64 {
        if self.mine > 0 { self.mine } else { self.min }
    }

    /// The amount at slider position `step` of [`BID_SLIDER_STEPS`]: a
    /// cubic curve, so the low end (where most bids are) gets most of the
    /// track, like tdesktop's paid-reaction slider.
    pub fn amount_at(self, step: f32) -> i64 {
        let t = f64::from(step.clamp(0., BID_SLIDER_STEPS)) / f64::from(BID_SLIDER_STEPS);
        let span = self.max.saturating_sub(self.min).max(0) as f64;
        let amount = self.min as f64 + span * t * t * t;
        (amount.round() as i64).clamp(self.min, self.max.max(self.min))
    }

    /// The slider position closest to `amount` (the inverse of
    /// [`Self::amount_at`]).
    pub fn step_of(self, amount: i64) -> f32 {
        let span = self.max.saturating_sub(self.min);
        if span <= 0 {
            return 0.;
        }
        let t = (amount.saturating_sub(self.min).clamp(0, span) as f64 / span as f64).cbrt();
        (t * f64::from(BID_SLIDER_STEPS)).round() as f32
    }
}

/// Positions on the bid slider's track.
pub const BID_SLIDER_STEPS: f32 = 1000.;

/// How the user's row reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidKind {
    /// Choosing a new bid: "Your bid will be".
    Setting,
    /// "You're winning" (green).
    Winning,
    /// "You've been outbid" (red).
    Losing,
}

impl BidKind {
    pub fn title(self, started: bool) -> &'static str {
        match (self, started) {
            (_, false) | (Self::Setting, _) => "Your bid will be",
            (Self::Winning, true) => "You're winning",
            (Self::Losing, true) => "You've been outbid",
        }
    }
}

/// A row under "Top winners": another bidder, or the user's own bid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopBid {
    pub user_id: i64,
    pub stars: i64,
    pub is_me: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidRows {
    pub kind: BidKind,
    pub position: i32,
    pub top: Vec<TopBid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidAction {
    /// Nothing to raise: the button reads "OK" and closes.
    Close,
    Place(i64),
    /// Raise the bid to this total.
    Increase(i64),
}

impl BidAction {
    /// "Place a ★N Bid" / "Add ★Δ to Your Bid" / "OK". `mine` is the
    /// current bid.
    pub fn label(self, mine: i64) -> String {
        let stars = |n: i64| format!("★{}", crate::premium_hub::group_digits(n));
        match self {
            Self::Close => "OK".into(),
            Self::Place(n) => format!("Place a {} Bid", stars(n)),
            Self::Increase(n) => format!("Add {} to Your Bid", stars(n.saturating_sub(mine))),
        }
    }
}

/// The box `AuctionState::choose_box` picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuctionBoxChoice {
    Bid,
    ChangeRecipient,
    Info,
    About,
}

/// One `giftAuctionAcquiredGift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquiredGift {
    pub receiver: Option<MessageSender>,
    pub date: i32,
    pub stars: i64,
    pub round: i32,
    pub position: i32,
    pub number: i32,
    pub text: String,
}

pub fn parse_acquired_gifts(value: &Value) -> Vec<AcquiredGift> {
    let mut gifts: Vec<AcquiredGift> = array(value, "gifts")
        .map(|gift| AcquiredGift {
            receiver: crate::premium_hub::sender_of(gift.get("receiver_id")),
            date: int32(gift.get("date")),
            stars: int(gift.get("star_count")).max(0),
            round: int32(gift.get("auction_round_number")),
            position: int32(gift.get("auction_round_position")),
            number: int32(gift.get("unique_gift_number")),
            text: gift
                .get("text")
                .map(|t| text(t, "text"))
                .unwrap_or_default(),
        })
        .collect();
    // tdesktop lists them by round.
    gifts.sort_by_key(|gift| gift.round);
    gifts
}

/// `giftUpgradePreview`: what an upgraded gift of a collection can look
/// like (the auction cover cycle and the craftable models).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpgradePreview {
    pub models: Vec<GiftAttribute>,
    pub symbols: Vec<GiftAttribute>,
    pub backdrops: Vec<GiftBackdrop>,
}

impl UpgradePreview {
    pub fn parse(value: &Value) -> Self {
        Self {
            models: array(value, "models").map(GiftAttribute::parse).collect(),
            symbols: array(value, "symbols").map(GiftAttribute::parse).collect(),
            backdrops: array(value, "backdrops").map(GiftBackdrop::parse).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty() || self.symbols.is_empty() || self.backdrops.is_empty()
    }

    /// The cover for `step` of the 1 s cycle: model, symbol and backdrop
    /// walk through their lists at different strides so the mix changes
    /// every step.
    pub fn cover(&self, step: u64) -> Option<(&GiftAttribute, &GiftAttribute, &GiftBackdrop)> {
        let pick = |len: usize, stride: u64| -> Option<usize> {
            let len = u64::try_from(len).ok().filter(|len| *len > 0)?;
            usize::try_from(step.wrapping_mul(stride) % len).ok()
        };
        Some((
            self.models.get(pick(self.models.len(), 1)?)?,
            self.symbols.get(pick(self.symbols.len(), 3)?)?,
            self.backdrops.get(pick(self.backdrops.len(), 7)?)?,
        ))
    }

    /// Models only crafting produces (`is_crafted`), for "View all
    /// craftable models".
    pub fn craftable_models(&self) -> impl Iterator<Item = &GiftAttribute> {
        self.models.iter().filter(|model| model.rarity.is_special())
    }

    pub fn files(&self) -> impl Iterator<Item = &crate::telegram::envelope::ParsedFile> {
        self.models
            .iter()
            .chain(self.symbols.iter())
            .flat_map(|attribute| attribute.files.iter())
    }
}
