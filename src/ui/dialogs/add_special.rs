use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::add_special::{SpecialConfirm, SpecialRole};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};

/// The "Add exception" / "Remove user" picker (tdesktop
/// `AddSpecialBoxController`): the group's members with a search field,
/// and the question a pick may need before it goes on.
pub struct AddSpecialDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) role: SpecialRole,
    pub(crate) search: Entity<InputState>,
    /// What the picker is doing: listing members, asking about the one
    /// picked, or waiting for the ban it sent.
    pub(crate) stage: SpecialStage,
    pub(crate) scroll: ScrollHandle,
    _search_changes: Subscription,
}

/// One value for where the picker stands. Leaves with the user's pick or
/// Back, the answer of the ban (`Kicking`), or the picker closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecialStage {
    /// The member list, waiting for a pick.
    Picking,
    /// The member picked and what the picker asks about them.
    Confirming {
        user_id: i64,
        question: SpecialConfirm,
    },
    /// "Remove user" sent as `request`: the picker closes when that ban is
    /// answered, and only that one.
    Kicking { request: RequestId },
}

impl AddSpecialDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        role: SpecialRole,
    ) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_REST_SEARCH_MEMBERS)));
        let search_changes = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_add_special(cx);
            }
        });
        Self {
            chat_id,
            role,
            search,
            stage: SpecialStage::Picking,
            scroll: ScrollHandle::new(),
            _search_changes: search_changes,
        }
    }
}
