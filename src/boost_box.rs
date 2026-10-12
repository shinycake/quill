//! The boost box (`parity:deeplink-boost`).
//!
//! tdesktop `SessionNavigation::resolveBoostState` opens `Ui::BoostBox` for a
//! `t.me/<channel>?boost` or `t.me/boost/<channel>` link: the channel's level,
//! how many boosts it needs, what the next level unlocks and one button.
//! The button runs `applyBoost`: it reads the viewer's boost slots and
//! sorts them (`ParseForChannelBoostSlots`). A free slot boosts at once. With
//! none free, the viewer sees one of the boxes below instead
//! (`ReassignBoostsBox`, `BoostBoxAlready`, `PremiumForBoostsBox`,
//! `GiftedNoBoostsBox`). Nothing is boosted before the click, and moving a
//! boost from another chat needs a second click on Reassign or Replace.
//!
//! This module holds the pure parts: the texts, the slot sorting, the
//! decision and the box state. `connect::groups::boost_flow` sends the
//! requests and `ui::boost_box_ui` draws it.
//!
//! Left out on purpose: the "gift Premium to get more boosts" lines of
//! `GiftForBoostsBox` and the multi-boost button (both need the
//! `boosts_per_sent_gift` app config, which Quill does not read), the
//! fireworks, and the "lifting" variant that group restriction prompts use.

use crate::local_time::{civil_local, day_label};
use crate::telegram::envelope::{ParsedBoostSlot, ParsedBoostStatus};

/// Level, boosts and the viewer's own boosts (`Ui::BoostCounters`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoostCounters {
    pub level: i32,
    pub boosts: i32,
    pub this_level_boosts: i32,
    /// Boosts the next level needs; 0 at the top level.
    pub next_level_boosts: i32,
    pub mine: i32,
}

impl From<&ParsedBoostStatus> for BoostCounters {
    fn from(status: &ParsedBoostStatus) -> Self {
        Self {
            level: status.level,
            boosts: status.boost_count,
            this_level_boosts: status.current_level_boost_count,
            next_level_boosts: status.next_level_boost_count,
            mine: status.my_boost_count,
        }
    }
}

/// The main button: its label and whether pressing it only closes the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoostButton {
    pub label: &'static str,
    pub closes: bool,
}

fn plural(count: i32, one: &str, other: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { other })
}

impl BoostCounters {
    /// Level the next boosts work towards: the one after the current level
    /// unless this is the top.
    pub fn target_level(&self) -> i32 {
        self.level + i32::from(self.next_level_boosts > 0)
    }

    fn boosts_left(&self) -> i32 {
        (self.next_level_boosts - self.boosts).max(0)
    }

    /// Share of the way from this level to the next, 0.0 to 1.0.
    pub fn fraction(&self) -> f32 {
        let span = self.next_level_boosts - self.this_level_boosts;
        if self.next_level_boosts <= 0 || span <= 0 {
            return 1.0;
        }
        ((self.boosts - self.this_level_boosts) as f32 / span as f32).clamp(0.0, 1.0)
    }

    /// `lng_boost_channel_you_title` and its siblings.
    pub fn title(&self, name: &str, group: bool) -> String {
        if self.mine > 0 {
            format!("You boosted {name}!")
        } else if self.next_level_boosts == 0 {
            "Maximum level reached".into()
        } else if self.level > 0 {
            if group {
                "Help upgrade group".into()
            } else {
                "Help upgrade channel".into()
            }
        } else if group {
            "Enable stories for group".into()
        } else {
            "Enable stories for this channel".into()
        }
    }

    /// The "x3" next to the title when the viewer boosts more than once.
    pub fn repeated(&self) -> Option<String> {
        (self.mine > 1).then(|| format!("x{}", self.mine))
    }

    /// The paragraph under the title.
    pub fn text(&self, name: &str, group: bool) -> String {
        let left = self.boosts_left();
        let what = if group { "group" } else { "channel" };
        let needs = || {
            format!(
                "{name} needs {} to unlock new features.",
                plural(left, "more boost", "more boosts")
            )
        };
        if self.mine == 0 && self.next_level_boosts != 0 {
            return needs();
        }
        if left > 0 {
            needs()
        } else if self.level == 0 {
            format!("This {what} reached Level 1 and can now post stories.")
        } else {
            let stories = self.target_level();
            format!(
                "This {what} reached Level {} and can now post {} per day.",
                self.level,
                plural(stories, "story", "stories")
            )
        }
    }

    /// `lng_boost_channel_button` and its siblings. Without the gift config
    /// there is no second boost from the same account, so a viewer who has
    /// boosted gets "OK".
    pub fn button(&self, group: bool) -> BoostButton {
        if self.next_level_boosts == 0 || self.mine > 0 {
            BoostButton {
                label: "OK",
                closes: true,
            }
        } else {
            BoostButton {
                label: if group {
                    "Boost Group"
                } else {
                    "Boost Channel"
                },
                closes: false,
            }
        }
    }
}

/// The viewer's slots, sorted for one chat (`ForChannelBoostSlots`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotChoice {
    /// Slots that boost nothing and are not cooling down.
    pub free: Vec<i32>,
    /// Slots that already boost this chat.
    pub already: Vec<i32>,
    /// Slots that boost another chat, or are cooling down.
    pub other: Vec<ParsedBoostSlot>,
}

/// `ParseForChannelBoostSlots`.
pub fn classify(chat_id: i64, slots: &[ParsedBoostSlot], now: i64) -> SlotChoice {
    let mut choice = SlotChoice::default();
    for slot in slots {
        if slot.chat_id == 0 && i64::from(slot.cooldown_until_date) <= now {
            choice.free.push(slot.slot_id);
        } else if slot.chat_id == chat_id {
            choice.already.push(slot.slot_id);
        } else {
            choice.other.push(*slot);
        }
    }
    choice
}

/// A box that only informs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoostNotice {
    /// Every slot already boosts this chat.
    Already,
    /// Boosting needs Telegram Premium.
    PremiumNeeded,
    /// Premium came as a gift, which cannot boost.
    GiftedNoBoosts,
    /// The only slot moved less than a day ago.
    Flood { seconds_left: i64, group: bool },
}

impl BoostNotice {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Already => "Already Boosted!",
            Self::PremiumNeeded => "Premium needed",
            Self::GiftedNoBoosts => "Can't boost with gifted Premium!",
            Self::Flood { .. } => "Can't boost too often!",
        }
    }

    pub fn text(&self, group: bool) -> String {
        let what = if group { "group" } else { "channel" };
        match self {
            Self::Already => format!("You are already boosting this {what}."),
            Self::PremiumNeeded => format!(
                "Only Telegram Premium subscribers can boost {what}s. Do you want to subscribe to Telegram Premium?"
            ),
            Self::GiftedNoBoosts => format!(
                "Because your Telegram Premium subscription was gifted to you, you can't use it to boost {what}s."
            ),
            Self::Flood {
                seconds_left,
                group,
            } => {
                let what = if *group { "group" } else { "channel" };
                format!(
                    "You can only change the {what} you boost once a day. You will be able to boost in {}.",
                    left_text(*seconds_left)
                )
            }
        }
    }

    /// Premium offers a way forward; the rest only say OK.
    pub fn offers_premium(&self) -> bool {
        matches!(self, Self::PremiumNeeded)
    }
}

/// `ReassignBoostFloodBox`: days, hours, minutes or seconds.
pub fn left_text(seconds: i64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3_600, seconds / 60);
    let unit = |count: i64, one: &str, other: &str| {
        format!("{count} {}", if count == 1 { one } else { other })
    };
    if days > 1 {
        unit(days, "day", "days")
    } else if hours > 1 {
        unit(hours, "hour", "hours")
    } else if minutes > 1 {
        unit(minutes, "minute", "minutes")
    } else {
        unit(seconds, "second", "seconds")
    }
}

/// What the Boost button does once the slots are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoostDecision {
    /// Boost with this free slot.
    Boost(i32),
    /// Move boosts here from other chats; the viewer picks.
    Reassign(Vec<ParsedBoostSlot>),
    Notice(BoostNotice),
}

/// `SessionNavigation::applyBoost` without the gift config. `is_group`
/// says whether a chat id is a group (for the flood text).
pub fn decide(
    choice: &SlotChoice,
    premium: bool,
    now: i64,
    is_group: impl Fn(i64) -> bool,
) -> BoostDecision {
    if let Some(slot) = choice.free.first() {
        return BoostDecision::Boost(*slot);
    }
    if choice.other.is_empty() {
        return BoostDecision::Notice(if !choice.already.is_empty() {
            BoostNotice::Already
        } else if !premium {
            BoostNotice::PremiumNeeded
        } else {
            BoostNotice::GiftedNoBoosts
        });
    }
    if let [only] = choice.other.as_slice()
        && i64::from(only.cooldown_until_date) > now
    {
        return BoostDecision::Notice(BoostNotice::Flood {
            seconds_left: i64::from(only.cooldown_until_date) - now,
            group: is_group(only.chat_id),
        });
    }
    BoostDecision::Reassign(sorted_for_list(&choice.other, now))
}

/// The reassign list order: the ones that can move first, longest ready
/// first; cooling-down ones last, soonest first.
pub fn sorted_for_list(slots: &[ParsedBoostSlot], now: i64) -> Vec<ParsedBoostSlot> {
    let mut list = slots.to_vec();
    list.sort_by_key(|slot| {
        let cooldown = i64::from(slot.cooldown_until_date);
        if cooldown > now { cooldown } else { -cooldown }
    });
    list
}

/// A row of the reassign list is cooling down.
pub fn is_waiting(slot: &ParsedBoostSlot, now: i64) -> bool {
    i64::from(slot.cooldown_until_date) > now
}

/// "available in 1:02:03" or "available in 12:05"
/// (`lng_boost_available_in`).
pub fn available_in(slot: &ParsedBoostSlot, now: i64) -> String {
    let seconds = (i64::from(slot.cooldown_until_date) - now).max(0);
    let (hours, minutes) = (seconds / 3_600, seconds / 60);
    if hours > 0 {
        format!(
            "available in {hours}:{:02}:{:02}",
            minutes % 60,
            seconds % 60
        )
    } else {
        format!("available in {minutes}:{:02}", seconds % 60)
    }
}

/// "boost expires on October 6" (`lng_boosts_list_status`).
pub fn expires_on(slot: &ParsedBoostSlot, now: i64) -> String {
    let at = civil_local(i64::from(slot.expiration_date));
    format!("boost expires on {}", day_label(&at, &civil_local(now)))
}

/// A row's status line.
pub fn row_status(slot: &ParsedBoostSlot, now: i64) -> String {
    if is_waiting(slot, now) {
        available_in(slot, now)
    } else {
        expires_on(slot, now)
    }
}

/// Tapping a row that is cooling down
/// (`lng_boost_available_in_toast` without the gift part).
pub const WAITING_NOTE: &str = "Wait until the boost is available.";

/// "You currently boost A. Do you want to boost B instead?"
pub fn replace_question(from: &str, to: &str) -> String {
    format!("You currently boost {from}. Do you want to boost {to} instead?")
}

/// How many distinct groups and channels the picked slots come from.
pub fn sources(
    slots: &[ParsedBoostSlot],
    selected: &[i32],
    is_group: impl Fn(i64) -> bool,
) -> (usize, usize) {
    let mut seen: Vec<i64> = Vec::new();
    let (mut groups, mut channels) = (0, 0);
    for id in selected {
        let Some(slot) = slots.iter().find(|slot| slot.slot_id == *id) else {
            continue;
        };
        if seen.contains(&slot.chat_id) {
            continue;
        }
        seen.push(slot.chat_id);
        if is_group(slot.chat_id) {
            groups += 1;
        } else {
            channels += 1;
        }
    }
    (groups, channels)
}

/// `lng_boost_reassign_done`.
pub fn reassign_done_text(count: usize, groups: usize, channels: usize) -> String {
    let from = if groups == 0 {
        format!("{channels} channel{}", if channels == 1 { "" } else { "s" })
    } else if channels == 0 {
        format!("{groups} group{}", if groups == 1 { "" } else { "s" })
    } else {
        format!("{} groups and channels", groups + channels)
    };
    if count == 1 {
        format!("1 boost is reassigned from {from}.")
    } else {
        format!("{count} boosts are reassigned from {from}.")
    }
}

/// Where the open box stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoostPhase {
    /// Counters, features and the button.
    Main,
    /// A request is out (slots, or `boostChat`).
    Working,
    /// Pick the boosts to move here.
    Reassign {
        slots: Vec<ParsedBoostSlot>,
        selected: Vec<i32>,
    },
    Notice(BoostNotice),
}

/// A reassignment sent and not answered yet: how many boosts and from how
/// many groups and channels, for the toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReassignSummary {
    pub count: usize,
    pub groups: usize,
    pub channels: usize,
}

/// The open boost box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoostBoxState {
    pub chat_id: i64,
    pub group: bool,
    pub phase: BoostPhase,
    /// A line under the counters: the last result or error.
    pub note: Option<String>,
    pub reassigning: Option<ReassignSummary>,
}

impl BoostBoxState {
    pub fn new(chat_id: i64, group: bool) -> Self {
        Self {
            chat_id,
            group,
            phase: BoostPhase::Main,
            note: None,
            reassigning: None,
        }
    }

    /// Show what the Boost button decided. `Boost` keeps the box working
    /// until `boostChat` answers.
    pub fn apply_decision(&mut self, decision: BoostDecision) {
        self.phase = match decision {
            BoostDecision::Boost(_) => BoostPhase::Working,
            BoostDecision::Reassign(slots) => BoostPhase::Reassign {
                slots,
                selected: Vec::new(),
            },
            BoostDecision::Notice(notice) => BoostPhase::Notice(notice),
        };
    }

    /// Check or uncheck a row. A row that is cooling down stays unchecked
    /// and the note says why.
    pub fn toggle_slot(&mut self, slot_id: i32, now: i64) {
        let BoostPhase::Reassign { slots, selected } = &mut self.phase else {
            return;
        };
        let Some(slot) = slots.iter().find(|slot| slot.slot_id == slot_id) else {
            return;
        };
        if is_waiting(slot, now) {
            self.note = Some(WAITING_NOTE.into());
        } else if let Some(at) = selected.iter().position(|id| *id == slot_id) {
            selected.remove(at);
        } else {
            selected.push(slot_id);
            self.note = None;
        }
    }

    /// The slots the Reassign button would move: the checked ones, or the
    /// only slot of the single-slot confirmation.
    pub fn reassign_selection(&self, now: i64) -> Vec<i32> {
        match &self.phase {
            BoostPhase::Reassign { slots, selected } => match slots.as_slice() {
                [only] if !is_waiting(only, now) => vec![only.slot_id],
                _ => selected
                    .iter()
                    .copied()
                    .filter(|id| {
                        slots
                            .iter()
                            .find(|slot| slot.slot_id == *id)
                            .is_some_and(|slot| !is_waiting(slot, now))
                    })
                    .collect(),
            },
            _ => Vec::new(),
        }
    }
}

impl crate::state::Session {
    /// A supergroup that is not a broadcast channel.
    pub fn chat_is_group(&self, chat_id: crate::ids::ChatId) -> bool {
        use crate::telegram::envelope::ChatKind;
        matches!(
            self.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup {
                is_channel: false,
                ..
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    fn slot(id: i32, chat: i64, cooldown: i32) -> ParsedBoostSlot {
        ParsedBoostSlot {
            slot_id: id,
            chat_id: chat,
            start_date: 1,
            expiration_date: 2_000_000,
            cooldown_until_date: cooldown,
        }
    }

    fn counters(level: i32, boosts: i32, this: i32, next: i32, mine: i32) -> BoostCounters {
        BoostCounters {
            level,
            boosts,
            this_level_boosts: this,
            next_level_boosts: next,
            mine,
        }
    }

    #[test]
    fn slots_are_sorted_like_tdesktop() {
        let slots = [
            slot(1, 0, 0),
            slot(2, 0, 2_000_000),
            slot(3, -100, 0),
            slot(4, -200, 0),
            slot(5, 0, 5),
        ];
        let choice = classify(-100, &slots, NOW);
        assert_eq!(choice.free, vec![1, 5]);
        assert_eq!(choice.already, vec![3]);
        assert_eq!(
            choice.other.iter().map(|s| s.slot_id).collect::<Vec<_>>(),
            vec![2, 4]
        );
    }

    #[test]
    fn a_free_slot_boosts_with_the_first() {
        let choice = classify(-100, &[slot(7, 0, 0), slot(8, 0, 0)], NOW);
        assert_eq!(
            decide(&choice, false, NOW, |_| false),
            BoostDecision::Boost(7)
        );
    }

    #[test]
    fn no_free_slot_picks_the_notice() {
        let decide_for = |slots: &[ParsedBoostSlot], premium| {
            decide(&classify(-100, slots, NOW), premium, NOW, |_| false)
        };
        assert_eq!(
            decide_for(&[slot(1, -100, 0)], true),
            BoostDecision::Notice(BoostNotice::Already)
        );
        assert_eq!(
            decide_for(&[], false),
            BoostDecision::Notice(BoostNotice::PremiumNeeded)
        );
        assert_eq!(
            decide_for(&[], true),
            BoostDecision::Notice(BoostNotice::GiftedNoBoosts)
        );
    }

    #[test]
    fn other_slots_go_to_reassign_and_a_lone_cooling_slot_is_flood() {
        let choice = classify(-100, &[slot(1, -200, 0), slot(2, -300, 0)], NOW);
        match decide(&choice, true, NOW, |_| false) {
            BoostDecision::Reassign(list) => assert_eq!(list.len(), 2),
            other => panic!("{other:?}"),
        }
        let one = classify(-100, &[slot(1, -200, NOW as i32 + 7_200)], NOW);
        assert_eq!(
            decide(&one, true, NOW, |chat| chat == -200),
            BoostDecision::Notice(BoostNotice::Flood {
                seconds_left: 7_200,
                group: true
            })
        );
        // Two cooling-down slots still open the list.
        let two = classify(
            -100,
            &[
                slot(1, -200, NOW as i32 + 10),
                slot(2, -300, NOW as i32 + 20),
            ],
            NOW,
        );
        assert!(matches!(
            decide(&two, true, NOW, |_| false),
            BoostDecision::Reassign(_)
        ));
    }

    #[test]
    fn list_order_puts_ready_rows_before_waiting_ones() {
        let list = sorted_for_list(
            &[
                slot(1, -2, NOW as i32 + 500),
                slot(2, -3, 100),
                slot(3, -4, NOW as i32 + 100),
                slot(4, -5, 900),
            ],
            NOW,
        );
        // Ready rows by -cooldown ascending (latest cooldown first), then
        // waiting rows by cooldown, as `std::stable_sort` on tdesktop's key.
        assert_eq!(
            list.iter().map(|s| s.slot_id).collect::<Vec<_>>(),
            vec![4, 2, 3, 1]
        );
    }

    #[test]
    fn texts_follow_the_counters() {
        let first = counters(0, 1, 0, 3, 0);
        assert_eq!(
            first.title("News", false),
            "Enable stories for this channel"
        );
        assert_eq!(first.title("Club", true), "Enable stories for group");
        assert_eq!(
            first.text("News", false),
            "News needs 2 more boosts to unlock new features."
        );
        assert_eq!(first.button(false).label, "Boost Channel");
        assert_eq!(first.button(true).label, "Boost Group");
        let more = counters(2, 5, 4, 8, 0);
        assert_eq!(more.title("News", false), "Help upgrade channel");
        // The viewer's own boost changes the title and the button.
        let mine = counters(1, 3, 3, 5, 1);
        assert_eq!(mine.title("News", false), "You boosted News!");
        assert_eq!(mine.repeated(), None);
        assert_eq!(counters(1, 3, 3, 5, 3).repeated().as_deref(), Some("x3"));
        assert_eq!(
            mine.button(false),
            BoostButton {
                label: "OK",
                closes: true
            }
        );
        let top = counters(9, 100, 90, 0, 0);
        assert_eq!(top.title("News", false), "Maximum level reached");
        assert!(top.button(false).closes);
    }

    #[test]
    fn reached_levels_say_what_they_unlock() {
        let reached_first = counters(0, 3, 0, 3, 1);
        assert_eq!(
            reached_first.text("News", false),
            "This channel reached Level 1 and can now post stories."
        );
        let level_two = counters(2, 9, 8, 0, 1);
        assert_eq!(
            level_two.text("Club", true),
            "This group reached Level 2 and can now post 2 stories per day."
        );
        let one_left = counters(1, 4, 3, 5, 1);
        assert_eq!(
            one_left.text("News", false),
            "News needs 1 more boost to unlock new features."
        );
    }

    #[test]
    fn progress_fraction() {
        assert_eq!(counters(1, 4, 2, 6, 0).fraction(), 0.5);
        assert_eq!(counters(9, 90, 80, 0, 0).fraction(), 1.0);
    }

    #[test]
    fn notices_and_durations() {
        assert_eq!(left_text(90_000 * 2), "2 days");
        assert_eq!(left_text(7_300), "2 hours");
        assert_eq!(left_text(150), "2 minutes");
        assert_eq!(left_text(45), "45 seconds");
        let flood = BoostNotice::Flood {
            seconds_left: 7_300,
            group: false,
        };
        assert_eq!(
            flood.text(false),
            "You can only change the channel you boost once a day. You will be able to boost in 2 hours."
        );
        assert!(BoostNotice::PremiumNeeded.offers_premium());
        assert!(!BoostNotice::Already.offers_premium());
        assert_eq!(
            BoostNotice::Already.text(true),
            "You are already boosting this group."
        );
    }

    #[test]
    fn row_status_counts_down_or_dates() {
        let waiting = slot(1, -2, NOW as i32 + 3_723);
        assert_eq!(available_in(&waiting, NOW), "available in 1:02:03");
        assert_eq!(
            available_in(&slot(1, -2, NOW as i32 + 125), NOW),
            "available in 2:05"
        );
        assert!(row_status(&slot(1, -2, 0), NOW).starts_with("boost expires on "));
        assert_eq!(row_status(&waiting, NOW), "available in 1:02:03");
    }

    #[test]
    fn selecting_skips_rows_that_are_cooling_down() {
        let slots = vec![slot(1, -2, 0), slot(2, -3, NOW as i32 + 60), slot(3, -2, 0)];
        let mut state = BoostBoxState::new(-100, false);
        state.apply_decision(BoostDecision::Reassign(slots));
        state.toggle_slot(1, NOW);
        state.toggle_slot(2, NOW);
        assert_eq!(state.note.as_deref(), Some(WAITING_NOTE));
        state.toggle_slot(3, NOW);
        assert_eq!(state.reassign_selection(NOW), vec![1, 3]);
        state.toggle_slot(1, NOW);
        assert_eq!(state.reassign_selection(NOW), vec![3]);
        // An unknown slot id changes nothing.
        state.toggle_slot(99, NOW);
        assert_eq!(state.reassign_selection(NOW), vec![3]);
    }

    #[test]
    fn a_single_slot_is_selected_for_the_confirmation() {
        let mut state = BoostBoxState::new(-100, false);
        state.apply_decision(BoostDecision::Reassign(vec![slot(4, -2, 0)]));
        assert_eq!(state.reassign_selection(NOW), vec![4]);
        // It is never selected while cooling down.
        state.apply_decision(BoostDecision::Reassign(vec![slot(4, -2, NOW as i32 + 9)]));
        assert!(state.reassign_selection(NOW).is_empty());
    }

    #[test]
    fn no_selection_outside_the_reassign_phase() {
        let state = BoostBoxState::new(-100, false);
        assert!(state.reassign_selection(NOW).is_empty());
        let mut state = state;
        state.apply_decision(BoostDecision::Boost(3));
        assert_eq!(state.phase, BoostPhase::Working);
    }

    #[test]
    fn sources_count_distinct_chats() {
        let slots = [
            slot(1, -2, 0),
            slot(2, -2, 0),
            slot(3, -3, 0),
            slot(4, -4, 0),
        ];
        let is_group = |chat| chat == -3;
        assert_eq!(sources(&slots, &[1, 2, 3], is_group), (1, 1));
        assert_eq!(sources(&slots, &[1, 2, 4], is_group), (0, 2));
        assert_eq!(sources(&slots, &[], is_group), (0, 0));
    }

    #[test]
    fn done_text_names_the_sources() {
        assert_eq!(
            reassign_done_text(1, 0, 1),
            "1 boost is reassigned from 1 channel."
        );
        assert_eq!(
            reassign_done_text(3, 2, 0),
            "3 boosts are reassigned from 2 groups."
        );
        assert_eq!(
            reassign_done_text(2, 1, 1),
            "2 boosts are reassigned from 2 groups and channels."
        );
        assert_eq!(
            replace_question("A", "B"),
            "You currently boost A. Do you want to boost B instead?"
        );
    }
}
