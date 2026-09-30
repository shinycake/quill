//! Member list state: filters and membership changes.

/// Phase D3b: which admin-management operation a `setChatMemberStatus`
/// request performs. Correlated on the `SetChatMemberStatus` purpose so
/// the response handler knows how to refresh the admin list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatusChange {
    /// Member → administrator with a fresh rights block.
    Promote,
    /// Administrator → administrator with an edited rights block.
    EditRights,
    /// Administrator → plain member (`chatMemberStatusMember`).
    Demote,
    /// Member → restricted (`chatMemberStatusRestricted`, schema 1.8.67
    /// line 2510). Not supported in basic groups and channels.
    Restrict,
    /// Member → banned (`chatMemberStatusBanned`, schema 1.8.67 line
    /// 2517). Works in supergroups and channels.
    Ban,
    /// Restricted/banned member → plain member
    /// (`chatMemberStatusMember`).
    Unban,
}

/// Slice G1: which `getSupergroupMembers` filter backs one cached member
/// page. The `Search` page's query is tracked by the caller (promote
/// picker / member dialog) rather than the cache — one page per
/// (chat, filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberListFilter {
    /// `supergroupMembersFilterRecent` (schema 1.8.67, line 2559).
    Recent,
    /// `supergroupMembersFilterSearch` (line 2568).
    Search,
    /// `supergroupMembersFilterAdministrators` (line 2565).
    Administrators,
    /// `supergroupMembersFilterRestricted` (line 2571); admins only.
    Restricted,
    /// `supergroupMembersFilterBanned` (line 2574); admins only.
    Banned,
}

impl MemberListFilter {
    /// Slice G1: whether `getSupergroupMembers` with this filter requires
    /// the `can_restrict_members` administrator right (schema 1.8.67,
    /// lines 2570/2574: restricted/banned filters are admin-only).
    pub fn requires_restrict_right(self) -> bool {
        matches!(
            self,
            MemberListFilter::Restricted | MemberListFilter::Banned
        )
    }
}
