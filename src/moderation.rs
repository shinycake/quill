//! Member moderation rules shared by the member list, the delete box and
//! the restrict dialog. Mirrors tdesktop's
//! `boxes/peers/edit_participants_box.cpp` (participant context menu),
//! `boxes/moderate_messages_box.cpp` (the admin delete box) and
//! `boxes/peers/edit_participant_box.cpp` (restrict-until). Everything here
//! is pure so the permission gating can be tested without a session.

use crate::telegram::envelope::ChannelMemberStatus;

/// TDLib treats a ban shorter than 30 seconds or longer than 366 days as
/// "forever" (`banChatMember` docs); tdesktop caps the picker the same way
/// (`kMaxRestrictDelayDays`).
pub const MIN_RESTRICT_SECS: i64 = 30;
pub const MAX_RESTRICT_DAYS: i64 = 366;

/// How long a restriction or ban lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestrictUntil {
    Forever,
    Day,
    Week,
    Month,
    /// A date and time the admin picked (unix seconds).
    Custom(i64),
}

impl RestrictUntil {
    pub const PRESETS: [RestrictUntil; 4] = [
        RestrictUntil::Forever,
        RestrictUntil::Day,
        RestrictUntil::Week,
        RestrictUntil::Month,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RestrictUntil::Forever => "Forever",
            RestrictUntil::Day => "1 day",
            RestrictUntil::Week => "1 week",
            RestrictUntil::Month => "1 month",
            RestrictUntil::Custom(_) => "Custom date",
        }
    }

    /// The `*_until_date` to send: 0 for forever, else a unix time.
    pub fn until_date(self, now: i64) -> i32 {
        let secs = match self {
            RestrictUntil::Forever => return 0,
            RestrictUntil::Day => now + 86_400,
            RestrictUntil::Week => now + 7 * 86_400,
            RestrictUntil::Month => now + 30 * 86_400,
            RestrictUntil::Custom(unix) => unix,
        };
        i32::try_from(secs).unwrap_or(0)
    }
}

/// Why a picked custom time cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntilError {
    Invalid,
    TooSoon,
    TooFar,
}

impl UntilError {
    pub fn message(self) -> &'static str {
        match self {
            UntilError::Invalid => "Pick a date and a time.",
            UntilError::TooSoon => "Pick a time in the future.",
            UntilError::TooFar => "Pick a time within the next year, or choose Forever.",
        }
    }
}

/// A custom restrict-until must lie in `[now + 30s, now + 366 days]`
/// (`ChooseDateTimeBox` with `min = now`, `max = now + kMaxRestrictDelayDays`).
pub fn validate_restrict_until(unix: i64, now: i64) -> Result<i64, UntilError> {
    if unix < now + MIN_RESTRICT_SECS {
        Err(UntilError::TooSoon)
    } else if unix > now + MAX_RESTRICT_DAYS * 86_400 {
        Err(UntilError::TooFar)
    } else {
        Ok(unix)
    }
}

/// What "Mention" puts in the composer: `@username`, or a text mention for
/// someone without one (tdesktop `fieldForMention->insertTag`).
pub fn mention_text(user_id: i64, username: &str, name: &str) -> String {
    if username.is_empty() {
        let name = name.replace(['[', ']'], "");
        format!("[{name}](tg://user?id={user_id}) ")
    } else {
        format!("@{username} ")
    }
}

/// Append a mention to the draft, separated by a space when needed.
pub fn append_mention(draft: &str, mention: &str) -> String {
    if draft.is_empty() || draft.ends_with(char::is_whitespace) {
        format!("{draft}{mention}")
    } else {
        format!("{draft} {mention}")
    }
}

/// The kind of chat a member list belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupFlavor {
    BasicGroup,
    Supergroup,
    Channel,
}

/// One entry of the member context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberAction {
    /// Insert `@username` (or a text mention) into the composer.
    Mention,
    /// Open the in-chat search filtered to this sender.
    SearchMessages,
    /// Add or edit the member tag (`setChatMemberTag`).
    EditTag,
    Promote,
    EditAdminRights,
    Restrict,
    Ban,
    /// "Remove from group": the member leaves and may come back.
    Remove,
    Unban,
    Unrestrict,
}

/// Everything the menu depends on, read from the session by the caller.
#[derive(Debug, Clone, Copy)]
pub struct MemberMenuContext {
    pub flavor: GroupFlavor,
    pub target_is_user: bool,
    pub target_is_self: bool,
    pub target_status: ChannelMemberStatus,
    /// `chatMember.can_be_edited` for an administrator target.
    pub target_can_be_edited: bool,
    pub viewer_can_promote: bool,
    pub viewer_can_restrict: bool,
    pub viewer_can_manage_tags: bool,
    /// The member list belongs to the chat that is open in the composer
    /// (needed for Mention and Search messages).
    pub chat_is_open: bool,
}

impl MemberMenuContext {
    fn target_is_owner(&self) -> bool {
        self.target_status == ChannelMemberStatus::Creator
    }

    /// Admins can be touched only when we made them (`can_be_edited`).
    fn target_is_actionable(&self) -> bool {
        self.target_is_user
            && !self.target_is_self
            && !self.target_is_owner()
            && (self.target_status != ChannelMemberStatus::Administrator
                || self.target_can_be_edited)
    }
}

/// The items of a member's context menu, in tdesktop's order: info rows
/// first (Mention, Search messages), the tag, then Promote / Restrict /
/// Remove, with Ban beside them where TDLib allows it.
pub fn member_menu_actions(ctx: &MemberMenuContext) -> Vec<MemberAction> {
    let mut out = Vec::new();
    if !ctx.target_is_user {
        return out;
    }
    let is_member_now = !matches!(
        ctx.target_status,
        ChannelMemberStatus::Banned | ChannelMemberStatus::Left | ChannelMemberStatus::Unknown
    );
    if ctx.chat_is_open && is_member_now {
        // tdesktop offers Mention for every user: by @username when there
        // is one, else a text mention.
        out.push(MemberAction::Mention);
        if ctx.flavor != GroupFlavor::Channel {
            out.push(MemberAction::SearchMessages);
        }
    }
    // Tags exist in basic groups and supergroups only (schema 13598).
    if ctx.flavor != GroupFlavor::Channel
        && is_member_now
        && (ctx.target_is_self || (ctx.viewer_can_manage_tags && ctx.target_is_actionable()))
    {
        out.push(MemberAction::EditTag);
    }
    if ctx.target_status == ChannelMemberStatus::Banned {
        if ctx.viewer_can_restrict && ctx.flavor != GroupFlavor::BasicGroup {
            out.push(MemberAction::Unban);
        }
        return out;
    }
    if !ctx.target_is_actionable() {
        return out;
    }
    if ctx.viewer_can_promote && is_member_now {
        out.push(if ctx.target_status == ChannelMemberStatus::Administrator {
            MemberAction::EditAdminRights
        } else {
            MemberAction::Promote
        });
    }
    if ctx.viewer_can_restrict && is_member_now {
        // `chatMemberStatusRestricted` is not supported in basic groups
        // and channels (schema 2510).
        if ctx.flavor == GroupFlavor::Supergroup {
            out.push(MemberAction::Restrict);
        }
        if ctx.target_status == ChannelMemberStatus::Restricted {
            out.push(MemberAction::Unrestrict);
        }
        if ctx.flavor != GroupFlavor::BasicGroup {
            out.push(MemberAction::Ban);
        }
        out.push(MemberAction::Remove);
    }
    out
}

/// A member the owner could hand the chat to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerCandidate {
    pub user_id: i64,
    pub status: ChannelMemberStatus,
    pub is_bot: bool,
}

/// The people the new-owner list shows: admins first, then members. Not
/// yourself, the current owner, bots, or anyone banned, restricted out or
/// gone ("The ownership can't be transferred to a bot or to a deleted
/// user"). Each user once, in the order given.
pub fn owner_choices(candidates: &[OwnerCandidate], me: Option<i64>) -> Vec<(i64, bool)> {
    let mut seen = std::collections::HashSet::new();
    let mut admins = Vec::new();
    let mut members = Vec::new();
    for c in candidates {
        let eligible = !c.is_bot
            && Some(c.user_id) != me
            && matches!(
                c.status,
                ChannelMemberStatus::Administrator | ChannelMemberStatus::Member
            );
        if !eligible || !seen.insert(c.user_id) {
            continue;
        }
        if c.status == ChannelMemberStatus::Administrator {
            admins.push((c.user_id, true));
        } else {
            members.push((c.user_id, false));
        }
    }
    admins.extend(members);
    admins
}

/// What the admin delete box offers for the senders of the selected
/// messages (`CalculateModerateOptions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModerateOptions {
    pub report_spam: bool,
    pub delete_all: bool,
    pub delete_reactions: bool,
    pub ban_or_restrict: bool,
}

impl ModerateOptions {
    pub fn any(self) -> bool {
        self.report_spam || self.delete_all || self.delete_reactions || self.ban_or_restrict
    }
}

/// Inputs for [`moderate_options`].
#[derive(Debug, Clone, Copy)]
pub struct ModerateInput {
    pub flavor: GroupFlavor,
    pub sender_is_user: bool,
    pub sender_is_self: bool,
    /// `messageProperties.can_report_supergroup_spam`.
    pub can_report_spam: bool,
    /// `messageProperties.can_be_deleted_for_all_users`.
    pub can_delete_for_all: bool,
    /// `messageProperties.can_delete_reactions`.
    pub can_delete_reactions: bool,
    pub viewer_can_restrict: bool,
    pub sender_status: ChannelMemberStatus,
    pub sender_can_be_edited: bool,
}

/// Which checkboxes the delete box shows. Supergroups and channels only;
/// own messages, other chats' senders and untouchable admins offer nothing.
pub fn moderate_options(input: &ModerateInput) -> ModerateOptions {
    if input.flavor == GroupFlavor::BasicGroup || !input.sender_is_user || input.sender_is_self {
        return ModerateOptions::default();
    }
    let sender_touchable = match input.sender_status {
        ChannelMemberStatus::Creator => false,
        ChannelMemberStatus::Administrator => input.sender_can_be_edited,
        _ => true,
    };
    ModerateOptions {
        report_spam: input.can_report_spam && input.flavor == GroupFlavor::Supergroup,
        delete_all: input.can_delete_for_all && input.flavor == GroupFlavor::Supergroup,
        delete_reactions: input.can_delete_reactions,
        ban_or_restrict: input.viewer_can_restrict && sender_touchable,
    }
}

/// What the admin ticked in the delete box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModerateChoice {
    pub report_spam: bool,
    pub delete_all: bool,
    pub delete_reactions: bool,
    pub ban: bool,
    /// With `ban`: restrict instead of ban (tdesktop's expander); the
    /// member stays and loses what is switched off.
    pub restrict_instead: bool,
}

impl ModerateChoice {
    pub fn any(self) -> bool {
        self.report_spam || self.delete_all || self.delete_reactions || self.ban
    }

    /// Drop what `options` does not offer, so a stale tick can never send a
    /// request the viewer is not allowed to make.
    pub fn clamp(self, options: ModerateOptions) -> Self {
        let ban = self.ban && options.ban_or_restrict;
        Self {
            report_spam: self.report_spam && options.report_spam,
            delete_all: self.delete_all && options.delete_all,
            delete_reactions: self.delete_reactions && options.delete_reactions,
            ban,
            restrict_instead: ban && self.restrict_instead,
        }
    }
}

/// One request of a moderation, in the order they must go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModerationStep {
    ReportSpam,
    DeleteAllFromSender,
    DeleteReactionsFromSender,
    Ban,
    Restrict,
}

/// Reports and deletes name the user's messages, so they go before the
/// ban or restriction (which is last, as in tdesktop's `confirms` event).
pub fn plan_moderation(choice: ModerateChoice) -> Vec<ModerationStep> {
    let mut steps = Vec::new();
    if choice.report_spam {
        steps.push(ModerationStep::ReportSpam);
    }
    if choice.delete_all {
        steps.push(ModerationStep::DeleteAllFromSender);
    }
    if choice.delete_reactions {
        steps.push(ModerationStep::DeleteReactionsFromSender);
    }
    if choice.ban {
        steps.push(if choice.restrict_instead {
            ModerationStep::Restrict
        } else {
            ModerationStep::Ban
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChannelMemberStatus as S;

    fn ctx(flavor: GroupFlavor, status: S) -> MemberMenuContext {
        MemberMenuContext {
            flavor,
            target_is_user: true,
            target_is_self: false,
            target_status: status,
            target_can_be_edited: false,
            viewer_can_promote: true,
            viewer_can_restrict: true,
            viewer_can_manage_tags: true,
            chat_is_open: true,
        }
    }

    #[test]
    fn an_admin_sees_the_full_menu_for_a_supergroup_member() {
        let items = member_menu_actions(&ctx(GroupFlavor::Supergroup, S::Member));
        assert_eq!(
            items,
            vec![
                MemberAction::Mention,
                MemberAction::SearchMessages,
                MemberAction::EditTag,
                MemberAction::Promote,
                MemberAction::Restrict,
                MemberAction::Ban,
                MemberAction::Remove,
            ]
        );
    }

    #[test]
    fn rights_gate_each_item() {
        let mut c = ctx(GroupFlavor::Supergroup, S::Member);
        c.viewer_can_promote = false;
        c.viewer_can_restrict = false;
        c.viewer_can_manage_tags = false;
        assert_eq!(
            member_menu_actions(&c),
            vec![MemberAction::Mention, MemberAction::SearchMessages]
        );
        c.chat_is_open = false;
        assert!(member_menu_actions(&c).is_empty());
    }

    #[test]
    fn basic_groups_cannot_restrict_or_ban_only_remove() {
        let items = member_menu_actions(&ctx(GroupFlavor::BasicGroup, S::Member));
        assert!(!items.contains(&MemberAction::Restrict));
        assert!(!items.contains(&MemberAction::Ban));
        assert!(items.contains(&MemberAction::Remove));
    }

    #[test]
    fn channels_ban_and_remove_but_do_not_restrict_search_or_tag() {
        let items = member_menu_actions(&ctx(GroupFlavor::Channel, S::Member));
        assert!(items.contains(&MemberAction::Ban));
        assert!(items.contains(&MemberAction::Remove));
        for gone in [
            MemberAction::Restrict,
            MemberAction::SearchMessages,
            MemberAction::EditTag,
        ] {
            assert!(!items.contains(&gone), "{gone:?}");
        }
    }

    #[test]
    fn nobody_can_touch_the_owner_or_themselves() {
        let owner = member_menu_actions(&ctx(GroupFlavor::Supergroup, S::Creator));
        assert!(!owner.contains(&MemberAction::Ban));
        assert!(!owner.contains(&MemberAction::Remove));
        assert!(!owner.contains(&MemberAction::Promote));
        let mut me = ctx(GroupFlavor::Supergroup, S::Member);
        me.target_is_self = true;
        let items = member_menu_actions(&me);
        assert!(items.contains(&MemberAction::Mention));
        assert!(items.contains(&MemberAction::EditTag));
        assert!(!items.contains(&MemberAction::Ban));
        assert!(!items.contains(&MemberAction::Remove));
    }

    #[test]
    fn admins_are_editable_only_when_we_promoted_them() {
        let mut c = ctx(GroupFlavor::Supergroup, S::Administrator);
        let locked = member_menu_actions(&c);
        assert!(!locked.contains(&MemberAction::EditAdminRights));
        assert!(!locked.contains(&MemberAction::Ban));
        c.target_can_be_edited = true;
        let items = member_menu_actions(&c);
        assert!(items.contains(&MemberAction::EditAdminRights));
        assert!(!items.contains(&MemberAction::Promote));
        assert!(items.contains(&MemberAction::Remove));
    }

    #[test]
    fn a_banned_member_only_offers_unban() {
        let items = member_menu_actions(&ctx(GroupFlavor::Supergroup, S::Banned));
        assert_eq!(items, vec![MemberAction::Unban]);
        let mut c = ctx(GroupFlavor::Supergroup, S::Banned);
        c.viewer_can_restrict = false;
        assert!(member_menu_actions(&c).is_empty());
    }

    #[test]
    fn a_restricted_member_can_be_unrestricted() {
        let items = member_menu_actions(&ctx(GroupFlavor::Supergroup, S::Restricted));
        assert!(items.contains(&MemberAction::Unrestrict));
        assert!(items.contains(&MemberAction::Restrict));
    }

    #[test]
    fn owner_choices_put_admins_first_and_skip_the_unfit() {
        let c = |user_id, status, is_bot| OwnerCandidate {
            user_id,
            status,
            is_bot,
        };
        let list = [
            c(1, S::Member, false),
            c(2, S::Administrator, false),
            c(3, S::Member, true),
            c(4, S::Creator, false),
            c(5, S::Banned, false),
            c(6, S::Restricted, false),
            c(7, S::Administrator, false),
            c(2, S::Administrator, false),
            c(9, S::Member, false),
        ];
        assert_eq!(
            owner_choices(&list, Some(9)),
            vec![(2, true), (7, true), (1, false)]
        );
        assert!(owner_choices(&[], None).is_empty());
    }

    #[test]
    fn mentions_use_the_username_or_a_text_mention() {
        assert_eq!(mention_text(5, "ada", "Ada L"), "@ada ");
        assert_eq!(mention_text(5, "", "Ada [L]"), "[Ada L](tg://user?id=5) ");
        assert_eq!(append_mention("", "@ada "), "@ada ");
        assert_eq!(append_mention("hi", "@ada "), "hi @ada ");
        assert_eq!(append_mention("hi ", "@ada "), "hi @ada ");
    }

    #[test]
    fn a_chat_sender_gets_no_user_actions() {
        let mut c = ctx(GroupFlavor::Supergroup, S::Member);
        c.target_is_user = false;
        assert!(member_menu_actions(&c).is_empty());
    }

    #[test]
    fn restrict_until_presets_and_custom_dates() {
        let now = 1_800_000_000;
        assert_eq!(RestrictUntil::Forever.until_date(now), 0);
        assert_eq!(RestrictUntil::Day.until_date(now), (now + 86_400) as i32);
        assert_eq!(
            RestrictUntil::Month.until_date(now),
            (now + 30 * 86_400) as i32
        );
        assert_eq!(
            RestrictUntil::Custom(now + 5_000).until_date(now),
            (now + 5_000) as i32
        );
    }

    #[test]
    fn custom_restrict_until_is_bounded_like_tdesktop() {
        let now = 1_800_000_000;
        assert_eq!(
            validate_restrict_until(now + 5, now),
            Err(UntilError::TooSoon)
        );
        assert_eq!(
            validate_restrict_until(now + 367 * 86_400, now),
            Err(UntilError::TooFar)
        );
        assert_eq!(validate_restrict_until(now + 3_600, now), Ok(now + 3_600));
        assert_eq!(
            validate_restrict_until(now + 366 * 86_400, now),
            Ok(now + 366 * 86_400)
        );
    }

    fn input() -> ModerateInput {
        ModerateInput {
            flavor: GroupFlavor::Supergroup,
            sender_is_user: true,
            sender_is_self: false,
            can_report_spam: true,
            can_delete_for_all: true,
            can_delete_reactions: true,
            viewer_can_restrict: true,
            sender_status: S::Member,
            sender_can_be_edited: false,
        }
    }

    #[test]
    fn an_admin_gets_every_checkbox_for_a_member() {
        let o = moderate_options(&input());
        assert!(o.report_spam && o.delete_all && o.delete_reactions && o.ban_or_restrict);
    }

    #[test]
    fn missing_rights_hide_their_checkbox() {
        let mut i = input();
        i.viewer_can_restrict = false;
        i.can_delete_for_all = false;
        let o = moderate_options(&i);
        assert!(!o.ban_or_restrict && !o.delete_all && o.report_spam);
        i.can_report_spam = false;
        i.can_delete_reactions = false;
        assert!(!moderate_options(&i).any());
    }

    #[test]
    fn no_moderation_for_own_messages_owners_or_basic_groups() {
        let mut i = input();
        i.sender_is_self = true;
        assert!(!moderate_options(&i).any());
        let mut i = input();
        i.flavor = GroupFlavor::BasicGroup;
        assert!(!moderate_options(&i).any());
        let mut i = input();
        i.sender_status = S::Creator;
        assert!(!moderate_options(&i).ban_or_restrict);
        i.sender_status = S::Administrator;
        assert!(!moderate_options(&i).ban_or_restrict);
        i.sender_can_be_edited = true;
        assert!(moderate_options(&i).ban_or_restrict);
    }

    #[test]
    fn channels_only_offer_ban_and_reaction_cleanup() {
        let mut i = input();
        i.flavor = GroupFlavor::Channel;
        let o = moderate_options(&i);
        assert!(o.ban_or_restrict && o.delete_reactions);
        assert!(!o.report_spam && !o.delete_all);
    }

    #[test]
    fn a_stale_tick_is_dropped_by_clamp() {
        let choice = ModerateChoice {
            report_spam: true,
            delete_all: true,
            delete_reactions: true,
            ban: true,
            restrict_instead: true,
        };
        let options = ModerateOptions {
            report_spam: true,
            ..ModerateOptions::default()
        };
        let clamped = choice.clamp(options);
        assert_eq!(
            clamped,
            ModerateChoice {
                report_spam: true,
                ..ModerateChoice::default()
            }
        );
        assert_eq!(plan_moderation(clamped), vec![ModerationStep::ReportSpam]);
    }

    #[test]
    fn the_plan_puts_the_ban_last_and_restrict_replaces_it() {
        let all = ModerateChoice {
            report_spam: true,
            delete_all: true,
            delete_reactions: true,
            ban: true,
            restrict_instead: false,
        };
        assert_eq!(
            plan_moderation(all),
            vec![
                ModerationStep::ReportSpam,
                ModerationStep::DeleteAllFromSender,
                ModerationStep::DeleteReactionsFromSender,
                ModerationStep::Ban,
            ]
        );
        let restrict = ModerateChoice {
            restrict_instead: true,
            ..all
        };
        assert_eq!(
            plan_moderation(restrict).last(),
            Some(&ModerationStep::Restrict)
        );
        assert!(plan_moderation(ModerateChoice::default()).is_empty());
    }
}
