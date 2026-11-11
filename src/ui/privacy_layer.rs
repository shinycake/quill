//! What the Privacy page has open over itself: one layer at a time, so a
//! rule editor, its exceptions list, a contact picker, the Unblock
//! confirmation and the "Last seen" warning cannot be open in a
//! combination that means nothing (docs/contributing/state-machines.md).
#![deny(clippy::indexing_slicing)]

use super::privacy::{PrivacyEditorTarget, PrivacyExceptionKind};
use quill::privacy::PrivacyRuleDetail;
use quill::telegram::requests_privacy::PrivacySettingKey;

/// The picker over an exceptions list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExceptionPicker {
    #[default]
    Closed,
    /// Add a contact.
    Contacts,
    /// Add a group (`...ChatMembers` rules).
    Groups,
}

/// The layer on top of the Privacy page.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum PrivacyLayer {
    /// The page alone.
    #[default]
    Page,
    /// The editor of a rule (or "Who can message me", "File open
    /// confirmations").
    Editor(PrivacyEditorTarget),
    /// The always / never list over the editor of `target`, with its picker.
    Exceptions {
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        picker: ExceptionPicker,
    },
    /// The contact picker that blocks a user.
    BlockPicker,
    /// "Unblock?" for this user, on the page.
    ConfirmUnblock(i64),
    /// The "Last seen" warning over the editor, holding the rule it will
    /// save.
    LastSeenWarning(PrivacyRuleDetail),
}

impl PrivacyLayer {
    /// The editor target, while one is open (it stays under the exceptions
    /// list and the warning).
    pub(crate) fn editor(&self) -> Option<PrivacyEditorTarget> {
        match self {
            Self::Editor(target) | Self::Exceptions { target, .. } => Some(*target),
            Self::LastSeenWarning(_) => {
                Some(PrivacyEditorTarget::Rule(PrivacySettingKey::ShowStatus))
            }
            Self::Page | Self::BlockPicker | Self::ConfirmUnblock(_) => None,
        }
    }

    pub(crate) fn exceptions(&self) -> Option<(PrivacyEditorTarget, PrivacyExceptionKind)> {
        match self {
            Self::Exceptions { target, kind, .. } => Some((*target, *kind)),
            _ => None,
        }
    }

    pub(crate) fn exception_picker(&self) -> ExceptionPicker {
        match self {
            Self::Exceptions { picker, .. } => *picker,
            _ => ExceptionPicker::Closed,
        }
    }

    pub(crate) fn block_picker_open(&self) -> bool {
        matches!(self, Self::BlockPicker)
    }

    pub(crate) fn unblock_confirm(&self) -> Option<i64> {
        match self {
            Self::ConfirmUnblock(user_id) => Some(*user_id),
            _ => None,
        }
    }

    pub(crate) fn last_seen_warning(&self) -> Option<&PrivacyRuleDetail> {
        match self {
            Self::LastSeenWarning(detail) => Some(detail),
            _ => None,
        }
    }

    /// Whether something covers the page (what Esc peels).
    pub(crate) fn is_open(&self) -> bool {
        !matches!(self, Self::Page | Self::ConfirmUnblock(_))
    }

    /// Open the editor of `target` over the page.
    pub(crate) fn open_editor(&mut self, target: PrivacyEditorTarget) {
        *self = Self::Editor(target);
    }

    /// Open the `kind` list of the open editor.
    pub(crate) fn open_exceptions(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
    ) {
        *self = Self::Exceptions {
            target,
            kind,
            picker: ExceptionPicker::Closed,
        };
    }

    /// Open the picker over the exceptions list (nothing without a list).
    pub(crate) fn open_exception_picker(&mut self, which: ExceptionPicker) {
        if let Self::Exceptions { picker, .. } = self {
            *picker = which;
        }
    }

    /// An exception was added: the picker goes, the list stays.
    pub(crate) fn close_exception_picker(&mut self) {
        self.open_exception_picker(ExceptionPicker::Closed);
    }

    pub(crate) fn open_block_picker(&mut self) {
        *self = Self::BlockPicker;
    }

    pub(crate) fn close_block_picker(&mut self) {
        if self.block_picker_open() {
            *self = Self::Page;
        }
    }

    /// Ask "Unblock?" on the page (nothing while an editor covers it).
    pub(crate) fn ask_unblock(&mut self, user_id: i64) {
        if matches!(self, Self::Page | Self::ConfirmUnblock(_)) {
            *self = Self::ConfirmUnblock(user_id);
        }
    }

    pub(crate) fn clear_unblock(&mut self) {
        if self.unblock_confirm().is_some() {
            *self = Self::Page;
        }
    }

    /// The editor closes (saved, cancelled).
    pub(crate) fn close_editor(&mut self) {
        if self.editor().is_some() {
            *self = Self::Page;
        }
    }

    /// Raise the "Last seen" warning for `detail`.
    pub(crate) fn warn_last_seen(&mut self, detail: PrivacyRuleDetail) {
        *self = Self::LastSeenWarning(detail);
    }

    /// Answer the warning: the rule it held, and back to the editor.
    pub(crate) fn take_last_seen_warning(&mut self) -> Option<PrivacyRuleDetail> {
        match std::mem::take(self) {
            Self::LastSeenWarning(detail) => {
                *self = Self::Editor(PrivacyEditorTarget::Rule(PrivacySettingKey::ShowStatus));
                Some(detail)
            }
            other => {
                *self = other;
                None
            }
        }
    }

    /// Leave the page or Settings: everything closes.
    pub(crate) fn close_all(&mut self) {
        *self = Self::Page;
    }

    /// Back one layer: picker, then exceptions list, then editor (like
    /// TGX's back stack). An Unblock question goes too.
    pub(crate) fn close_top(&mut self) {
        *self = match std::mem::take(self) {
            Self::Exceptions {
                target,
                picker: ExceptionPicker::Closed,
                ..
            } => Self::Editor(target),
            Self::Exceptions { target, kind, .. } => Self::Exceptions {
                target,
                kind,
                picker: ExceptionPicker::Closed,
            },
            Self::LastSeenWarning(_) | Self::Editor(_) => Self::Page,
            Self::BlockPicker | Self::ConfirmUnblock(_) | Self::Page => Self::Page,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> PrivacyEditorTarget {
        PrivacyEditorTarget::Rule(PrivacySettingKey::ShowStatus)
    }

    #[test]
    fn escape_peels_the_picker_then_the_list_then_the_editor() {
        let mut layer = PrivacyLayer::Page;
        layer.open_editor(rule());
        layer.open_exceptions(rule(), PrivacyExceptionKind::Always);
        layer.open_exception_picker(ExceptionPicker::Groups);
        assert_eq!(layer.exception_picker(), ExceptionPicker::Groups);
        layer.close_top();
        assert_eq!(layer.exception_picker(), ExceptionPicker::Closed);
        assert!(layer.exceptions().is_some());
        layer.close_top();
        assert_eq!(layer.editor(), Some(rule()));
        assert!(layer.exceptions().is_none());
        layer.close_top();
        assert!(!layer.is_open());
    }

    #[test]
    fn opening_a_layer_replaces_the_one_under_it_instead_of_stacking_flags() {
        let mut layer = PrivacyLayer::Page;
        layer.open_block_picker();
        layer.open_editor(rule());
        assert!(!layer.block_picker_open(), "one layer at a time");
        layer.open_exception_picker(ExceptionPicker::Contacts);
        assert_eq!(
            layer.exception_picker(),
            ExceptionPicker::Closed,
            "no picker without an exceptions list"
        );
    }

    #[test]
    fn the_warning_returns_its_rule_and_lands_back_on_the_editor() {
        let mut layer = PrivacyLayer::Editor(rule());
        layer.warn_last_seen(PrivacyRuleDetail::default());
        assert_eq!(layer.editor(), Some(rule()), "the editor stays under it");
        assert!(layer.last_seen_warning().is_some());
        assert!(layer.take_last_seen_warning().is_some());
        assert_eq!(layer, PrivacyLayer::Editor(rule()));
        assert!(layer.take_last_seen_warning().is_none());
    }

    #[test]
    fn unblock_is_asked_on_the_page_only_and_leaving_closes_it() {
        let mut layer = PrivacyLayer::Editor(rule());
        layer.ask_unblock(7);
        assert_eq!(layer.unblock_confirm(), None);
        layer.close_all();
        layer.ask_unblock(7);
        assert_eq!(layer.unblock_confirm(), Some(7));
        layer.clear_unblock();
        assert_eq!(layer, PrivacyLayer::Page);
    }
}
