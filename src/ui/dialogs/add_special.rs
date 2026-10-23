use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::add_special::{SpecialConfirm, SpecialRole};
use quill::ids::ChatId;
use quill::lang::{keys, tr};

/// The "Add exception" / "Remove user" picker (tdesktop
/// `AddSpecialBoxController`): the group's members with a search field,
/// and the question a pick may need before it goes on.
pub struct AddSpecialDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) role: SpecialRole,
    pub(crate) search: Entity<InputState>,
    /// The member picked and what the picker asks about them.
    pub(crate) confirm: Option<(i64, SpecialConfirm)>,
    pub(crate) scroll: ScrollHandle,
    _search_changes: Subscription,
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
            confirm: None,
            scroll: ScrollHandle::new(),
            _search_changes: search_changes,
        }
    }
}
