//! Icons of the attachment menu bots: the `default_icon` file of each bot
//! is tracked like any file, so the driver can download it and the menu
//! can read the SVG from disk.
use super::*;
use crate::telegram::envelope::AttachmentMenuBot;

impl Session {
    /// Remember the icon files that came with `attachmentMenuBot` objects
    /// (the local state may already be newer; `upsert_file` keeps it).
    pub(crate) fn keep_attach_bot_icons(&mut self, bots: &[AttachmentMenuBot]) {
        for icon in bots.iter().filter_map(|bot| bot.icon.clone()) {
            self.upsert_file(icon, false);
        }
    }

    /// Icon files of the bots shown in the attachment menu that still need
    /// a download.
    pub fn attach_bot_icons_to_download(&self) -> Vec<FileId> {
        self.bots
            .web_apps
            .attachment_menu_bots
            .iter()
            .filter(|bot| bot.is_added && bot.show_in_attachment_menu)
            .filter_map(|bot| bot.icon.as_ref().map(|icon| icon.id))
            .filter(|id| self.should_download(*id))
            .collect()
    }

    /// The downloaded SVG of a bot's menu icon.
    pub fn attach_bot_icon_path(&self, bot_user_id: i64) -> Option<&str> {
        let icon = self
            .bots
            .web_apps
            .attachment_menu_bots
            .iter()
            .find(|bot| bot.bot_user_id == bot_user_id)?
            .icon
            .as_ref()?;
        self.file(icon.id)?.usable_path()
    }
}
