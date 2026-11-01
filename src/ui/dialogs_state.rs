//! Standalone boxes: profiles, contacts, chat look, export, tags and call rating.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};

pub(crate) struct DialogUi {
    /// Profile layer opened from a sender avatar (tdesktop's
    /// `Info::LayerWidget`); presents `session.users_state.open_info_panel`.
    pub(super) profile_modal: Option<super::profile_modal::ProfileModal>,
    /// B10: edit-contact / birthday / personal-channel / share-contact
    /// dialog behind the profile panels.
    pub(super) profile_dialog: Option<ProfileDialog>,
    /// "Verify accounts" for the owner of a verifying bot.
    pub(super) bot_verify_dialog: Option<super::bot_verify_dialog::BotVerifyDialog>,
    /// Edit screen of a bot the viewer owns.
    pub(super) bot_edit_dialog: Option<super::bot_edit_dialog::BotEditDialog>,
    /// "Create Bot" from a managed-bot link.
    pub(super) bot_create_dialog: Option<super::bot_create_dialog::BotCreateDialog>,
    /// Settings -> Quick Replies (Telegram Business shortcuts).
    pub(super) quick_replies_dialog: Option<super::quick_replies::QuickRepliesDialog>,
    /// Settings -> Telegram Business.
    pub(super) business_dialog: Option<super::business::BusinessDialog>,
    /// A5: edit-profile dialog (name / bio / username / photo) opened
    /// from the user's own info panel.
    pub(super) edit_profile_dialog: Option<EditProfileDialog>,
    /// "Your color": name color, reply icon, profile color and icon.
    pub(super) peer_color_dialog: Option<super::peer_color_dialog::PeerColorDialog>,
    /// "Appearance" of a channel or group: colors and emoji status.
    pub(super) chat_appearance_dialog: Option<super::chat_appearance_dialog::ChatAppearanceDialog>,
    /// B10: a profile photo gallery whose list was requested; the viewer
    /// opens when it lands (checked by the poll loop).
    pub(super) pending_profile_gallery: Option<quill::state::PhotoOwner>,
    /// "Do you want to delete this photo?" for a photo deleted from the
    /// profile photo viewer, with whether an earlier photo's message goes
    /// for everyone.
    pub(super) photo_delete: Option<(quill::state::PhotoDelete, bool)>,
    /// Phase 6: add-contact dialog (phone + first/last name) opened from
    /// the user info panel.
    pub(super) add_contact_dialog: Option<AddContactDialog>,
    /// Slice A6: vCard import dialog opened from the Contacts tab
    /// settings section.
    pub(super) import_contacts_dialog: Option<ImportContactsDialog>,
    /// Batch 8: "Block {name}" box opened from the chat action bar.
    pub(super) block_bar_dialog: Option<super::chat_bars::BlockBarDialog>,
    /// Theme and wallpaper picker for a chat.
    pub(super) chat_look_dialog: Option<super::chat_look_ui::ChatLookDialog>,
    /// The account wallpaper preview (gallery, `bg/` link, file).
    pub(super) wallpaper_preview: Option<super::wallpaper_preview::WallpaperPreview>,
    /// An installed wallpaper waiting for "Delete" to be confirmed.
    pub(super) wallpaper_remove: Option<i64>,
    /// The "Export chat history" box.
    pub(super) chat_export_dialog: Option<super::export_box::ChatExportDraft>,
    /// The running export's progress is not on screen; its result shows as a
    /// note instead.
    pub(super) chat_export_progress_hidden: bool,
    /// Last progress drawn, to redraw only when it changes.
    pub(super) chat_export_seen: Option<quill::export::Progress>,
    /// The "Export Telegram data" box (full account export).
    pub(super) account_export_dialog: Option<super::account_export_box::AccountExportDraft>,
    /// The running account export's progress is not on screen; its result
    /// shows as a note instead.
    pub(super) account_export_progress_hidden: bool,
    /// Last account export progress drawn.
    pub(super) account_export_seen: Option<quill::export::account::AccountProgress>,
    pub(super) export_stop_open: bool,
    /// Saved Messages: "Add Name" / "Edit Name" for a tag.
    pub(super) saved_tag_dialog: Option<super::saved_sublists::SavedTagDialog>,
    /// The Saved Messages sublist list, which pages near its end
    /// (`scroll_load`).
    pub(super) saved_scroll: ScrollHandle,
    pub(super) saved_asked: super::scroll_load::Asked,
    /// Phase C2i: rating detail draft for the call-end card — the star
    /// tap opens the problems checklist + comment field instead of
    /// sending immediately.
    pub(super) rating_detail: Option<RatingDetail>,
    /// Phase C2i: comment input for the rating detail card.
    pub(super) rating_comment_input: Entity<TextareaState>,
    /// Business hours row of a profile: expanded schedule and time zone.
    pub(super) business_hours: super::profile_business::BusinessHoursUi,
    /// The Disable Sharing box for this private chat.
    pub(super) disable_sharing: Option<quill::ids::ChatId>,
}

impl DialogUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        // Phase C2i: comment field for the call-rating detail card.
        let rating_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_WHAT_WENT_WRONG_OPTIONAL))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            profile_modal: None,
            profile_dialog: None,
            bot_verify_dialog: None,
            bot_edit_dialog: None,
            bot_create_dialog: None,
            quick_replies_dialog: None,
            business_dialog: None,
            edit_profile_dialog: None,
            peer_color_dialog: None,
            chat_appearance_dialog: None,
            pending_profile_gallery: None,
            photo_delete: None,
            add_contact_dialog: None,
            import_contacts_dialog: None,
            block_bar_dialog: None,
            chat_look_dialog: None,
            wallpaper_preview: None,
            wallpaper_remove: None,
            chat_export_dialog: None,
            chat_export_progress_hidden: false,
            chat_export_seen: None,
            account_export_dialog: None,
            account_export_progress_hidden: false,
            account_export_seen: None,
            export_stop_open: false,
            saved_tag_dialog: None,
            saved_scroll: ScrollHandle::new(),
            saved_asked: Default::default(),
            rating_detail: None,
            rating_comment_input,
            business_hours: Default::default(),
            disable_sharing: None,
        }
    }
}
