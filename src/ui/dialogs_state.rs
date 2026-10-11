//! Standalone boxes: profiles, contacts, chat look, export, tags and call rating.

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

pub(crate) struct DialogUi {
    /// Profile layer opened from a sender avatar (tdesktop's
    /// `Info::LayerWidget`); presents `session.users_state.open_info_panel`.
    pub(super) profile_modal: Option<super::profile_modal::ProfileModal>,
    /// B10: edit-contact / birthday / personal-channel / share-contact
    /// dialog behind the profile panels.
    pub(super) profile_dialog: Option<ProfileDialog>,
    /// A5: edit-profile dialog (name / bio / username / photo) opened
    /// from the user's own info panel.
    pub(super) edit_profile_dialog: Option<EditProfileDialog>,
    /// B10: a profile photo gallery whose list was requested; the viewer
    /// opens when it lands (checked by the poll loop).
    pub(super) pending_profile_gallery: Option<i64>,
    /// Phase 6: add-contact dialog (phone + first/last name) opened from
    /// the user info panel.
    pub(super) add_contact_dialog: Option<AddContactDialog>,
    /// Slice A6: vCard import dialog opened from the Contacts tab
    /// settings section.
    pub(super) import_contacts_dialog: Option<ImportContactsDialog>,
    /// Batch 8: "Block {name}" box opened from the chat action bar.
    pub(super) block_bar_dialog: Option<super::chat_bars::BlockBarDialog>,
    /// Theme and wallpaper picker for a chat, or a `bg/` link preview.
    pub(super) chat_look_dialog: Option<super::chat_look_ui::ChatLookDialog>,
    /// The "Export chat history" box.
    pub(super) chat_export_dialog: Option<super::chat_export_ui::ChatExportDraft>,
    /// Saved Messages: "Add Name" / "Edit Name" for a tag.
    pub(super) saved_tag_dialog: Option<super::saved_sublists::SavedTagDialog>,
    /// Phase C2i: rating detail draft for the call-end card — the star
    /// tap opens the problems checklist + comment field instead of
    /// sending immediately.
    pub(super) rating_detail: Option<RatingDetail>,
    /// Phase C2i: comment input for the rating detail card.
    pub(super) rating_comment_input: Entity<TextareaState>,
    /// Business hours row of a profile: expanded schedule and time zone.
    pub(super) business_hours: super::profile_business::BusinessHoursUi,
}

impl DialogUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        // Phase C2i: comment field for the call-rating detail card.
        let rating_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What went wrong? (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            profile_modal: None,
            profile_dialog: None,
            edit_profile_dialog: None,
            pending_profile_gallery: None,
            add_contact_dialog: None,
            import_contacts_dialog: None,
            block_bar_dialog: None,
            chat_look_dialog: None,
            chat_export_dialog: None,
            saved_tag_dialog: None,
            rating_detail: None,
            rating_comment_input,
            business_hours: Default::default(),
        }
    }
}
