//! "Show photo previews": a photo message's notification shows the photo.
//!
//! The photo is rarely on disk when the message arrives, so the
//! notification waits for its download, at most [`PHOTO_WAIT`], and then
//! shows with whatever is there. Nothing is decoded: the platform reads
//! TDLib's file itself (macOS copies it on the notification thread).

use super::*;
use quill::ids::{AccountKey, FileId};
use std::time::Duration;

/// How long a notification waits for its photo before it shows without.
const PHOTO_WAIT: Duration = Duration::from_secs(3);

/// TDLib's highest download priority: the user is about to see it.
const PHOTO_PRIORITY: i32 = 32;

/// A notification waiting for its photo. The entry owns the task that ends
/// the wait at [`PHOTO_WAIT`]; the poll shows it earlier when the photo lands
/// (`settle_preview_waits`). Removing the entry (a newer notification for the
/// chat, its dismissal, the account going) cancels the task and shows nothing.
pub(in crate::ui) struct PreviewWait {
    account: Option<AccountKey>,
    file: FileId,
    on_screen: Option<AccountKey>,
    locked: bool,
    notification: quill::notify::OsNotification,
    photo: Option<PathBuf>,
    _deadline: Task<()>,
}

impl QuillApp {
    /// Show a notification with its sender picture and, when `preview` is
    /// set, the message's photo, once that is downloaded or after
    /// [`PHOTO_WAIT`].
    pub(in crate::ui) fn show_notification_with_preview(
        &mut self,
        account: Option<AccountKey>,
        notification: quill::notify::OsNotification,
        photo: Option<PathBuf>,
        preview: Option<FileId>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        let key = (
            self.notification_account(account.as_ref()),
            notification.chat_id,
        );
        // This notification replaces any older one still waiting.
        self.delivery.preview_waits.remove(&key);
        let preview_path = preview.and_then(|file| self.preview_path(account.as_ref(), file));
        let waits = preview
            .filter(|file| preview_path.is_none() && self.request_preview(account.as_ref(), *file));
        let Some(file) = waits else {
            self.show_notification_with_photo(
                account,
                notification,
                photo,
                preview_path,
                locked,
                cx,
            );
            return;
        };
        // `None` means the account on screen: a switch while waiting would
        // post this one under the other account.
        let on_screen = self.session().map(|session| session.account.clone());
        self.arm_preview_wait(
            key,
            |deadline| PreviewWait {
                account,
                file,
                on_screen,
                locked,
                notification,
                photo,
                _deadline: deadline,
            },
            cx,
        );
    }

    /// Hold a wait for `key` with the task that ends it at [`PHOTO_WAIT`].
    fn arm_preview_wait(
        &mut self,
        key: (AccountKey, ChatId),
        wait: impl FnOnce(Task<()>) -> PreviewWait,
        cx: &mut Context<Self>,
    ) {
        // screen-scope: the wait names the notification's account and checks it
        // against the account on screen itself before showing anything.
        let deadline = Self::spawn_app(cx, {
            let key = key.clone();
            async move |this, cx| {
                cx.background_executor().timer(PHOTO_WAIT).await;
                let _ = this.update(cx, |app, cx| app.settle_preview_wait(&key, cx));
            }
        });
        self.delivery.preview_waits.insert(key, wait(deadline));
    }

    /// The poll's drain: every waiting notification whose photo has landed
    /// shows now. The rest wait for their deadline task.
    pub(in crate::ui) fn settle_preview_waits(&mut self, cx: &mut Context<Self>) -> bool {
        let landed: Vec<_> = self
            .delivery
            .preview_waits
            .iter()
            .filter(|(_, wait)| {
                self.preview_path(wait.account.as_ref(), wait.file)
                    .is_some()
            })
            .map(|(key, _)| key.clone())
            .collect();
        let settled = !landed.is_empty();
        for key in landed {
            self.settle_preview_wait(&key, cx);
        }
        settled
    }

    /// The photo landed or the wait ran out: show the notification, unless
    /// it was replaced or dismissed meanwhile (then its entry is gone).
    fn settle_preview_wait(&mut self, key: &(AccountKey, ChatId), cx: &mut Context<Self>) {
        let Some(wait) = self.delivery.preview_waits.remove(key) else {
            return;
        };
        let PreviewWait {
            account,
            file,
            on_screen,
            locked,
            notification,
            photo,
            _deadline,
        } = wait;
        // This may be the deadline task's own turn: dropping it only ends
        // the future once this update returns, and keeps it from settling a
        // newer wait for the same chat.
        drop(_deadline);
        if account.is_none() && self.session().map(|session| session.account.clone()) != on_screen {
            return;
        }
        let path = self.preview_path(account.as_ref(), file);
        let (mut notification, mut photo) = (notification, photo);
        // The passcode lock came up while waiting: show what a locked app
        // shows.
        let now_locked = locked || self.account.passcode.is_locked();
        let path = if now_locked { None } else { path };
        if now_locked && !locked {
            notification.title = quill::notify::APP_TITLE.to_string();
            notification.body = quill::notify::GENERIC_BODY.to_string();
            photo = None;
        }
        self.show_notification_with_photo(account, notification, photo, path, now_locked, cx);
    }

    /// The downloaded photo file, on the account that queued it.
    fn preview_path(&self, account: Option<&AccountKey>, file: FileId) -> Option<PathBuf> {
        let live = match account {
            None => self.live.as_ref(),
            Some(key) => self.background.client(key)?.live.as_ref(),
        }?;
        live.driver
            .session
            .media
            .files
            .get(&file.0)?
            .usable_path()
            .map(PathBuf::from)
    }

    /// Ask TDLib for the photo first in line. `false` when it will not
    /// come: no connection, data saver on, or the request failed.
    fn request_preview(&mut self, account: Option<&AccountKey>, file: FileId) -> bool {
        let live = match account {
            None => self.live.as_mut(),
            Some(key) => self
                .background
                .client_mut(key)
                .and_then(|client| client.live.as_mut()),
        };
        let Some(live) = live else {
            return false;
        };
        if live.driver.session.settings.media_prefs.data_saver {
            return false;
        }
        live.driver.download_file(file, PHOTO_PRIORITY).is_ok()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "photo_preview_tests.rs"]
mod tests;
