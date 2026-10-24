//! Notifications of accounts that are not on screen
//! (`parity:notify-all-accounts`): shown with the account's own tag, its
//! own chat pictures and its own notification sounds.

use super::*;
use quill::ids::AccountKey;

impl QuillApp {
    /// Show one notification a background account queued. Locked: no
    /// sender or text, as for the account on screen.
    pub(in crate::ui) fn show_background_notification(
        &mut self,
        account: &AccountKey,
        queued: QueuedNotification,
        cx: &mut Context<Self>,
    ) {
        let locked = self.account.passcode.locked
            || self
                .background
                .client(account)
                .and_then(|client| client.live.as_ref())
                .is_some_and(|live| !live.driver.session.settings.notify_view.shows_name());
        let notification = if locked {
            queued.for_locked_display()
        } else {
            queued.for_display()
        };
        let photo = if locked {
            None
        } else {
            self.background
                .client(account)
                .and_then(|client| client.live.as_ref())
                .and_then(|live| live.driver.session.chat_photo_path(notification.chat_id))
                .map(PathBuf::from)
        };
        let preview = queued.photo.filter(|_| !locked);
        self.show_notification_with_preview(
            Some(account.clone()),
            notification,
            photo,
            preview,
            locked,
            cx,
        );
    }

    /// Play a background account's notification sound: a custom sound is
    /// that account's file.
    pub(in crate::ui) fn play_background_sound(
        &mut self,
        account: &AccountKey,
        kind: NotificationSoundKind,
        chat_id: ChatId,
    ) {
        let volume = self
            .background
            .client(account)
            .and_then(|client| client.live.as_ref())
            .and_then(|live| live.driver.session.notification_volume_override(chat_id));
        let sound = self
            .background
            .client_mut(account)
            .and_then(|client| client.live.as_mut())
            .map_or(Some(NotificationSound::DefaultTone), |live| {
                notification_sound(live.driver.resolve_notification_sound(kind))
            });
        if let Some(sound) = sound {
            self.notify.notification_sounds.play_at(sound, volume);
        }
    }

    /// Play a custom sound whose download a background account finished.
    pub(in crate::ui) fn play_sound_file(&mut self, path: PathBuf) {
        self.notify
            .notification_sounds
            .play(NotificationSound::File(path));
    }
}
