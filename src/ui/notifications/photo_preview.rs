//! "Show photo previews": a photo message's notification shows the photo.
//!
//! The photo is rarely on disk when the message arrives, so the
//! notification waits for its download, at most [`PHOTO_WAIT`], and then
//! shows with whatever is there. Nothing is decoded: the platform reads
//! TDLib's file itself (macOS copies it on the notification thread).

use super::*;
use quill::ids::{AccountKey, FileId};
use std::time::{Duration, Instant};

/// How long a notification waits for its photo before it shows without.
const PHOTO_WAIT: Duration = Duration::from_secs(3);

/// How often a waiting notification looks for its photo.
const PHOTO_CHECK: Duration = Duration::from_millis(100);

/// TDLib's highest download priority: the user is about to see it.
const PHOTO_PRIORITY: i32 = 32;

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
        let key = (account.clone(), notification.chat_id);
        // This notification replaces any older one still waiting.
        self.notify.preview_waits.remove(&key);
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
        self.notify.next_preview_wait += 1;
        let id = self.notify.next_preview_wait;
        self.notify.preview_waits.insert(key.clone(), id);
        let deadline = Instant::now() + PHOTO_WAIT;
        // `None` means the account on screen: a switch while waiting would
        // post this one under the other account.
        let on_screen = self.session().map(|session| session.account.clone());
        let mut waiting = Some((notification, photo));
        Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor().timer(PHOTO_CHECK).await;
                let done = this
                    .update(cx, |app, cx| {
                        let path = app.preview_path(key.0.as_ref(), file);
                        if path.is_none() && Instant::now() < deadline {
                            return false;
                        }
                        // Replaced or dismissed while waiting: never shown.
                        if app.notify.preview_waits.get(&key) != Some(&id) {
                            return true;
                        }
                        if key.0.is_none()
                            && app.session().map(|session| session.account.clone()) != on_screen
                        {
                            app.notify.preview_waits.remove(&key);
                            return true;
                        }
                        app.notify.preview_waits.remove(&key);
                        let Some((mut notification, mut photo)) = waiting.take() else {
                            return true;
                        };
                        // The passcode lock came up while waiting: show
                        // what a locked app shows.
                        let now_locked = locked || app.account.passcode.locked;
                        let path = if now_locked { None } else { path };
                        if now_locked && !locked {
                            notification.title = quill::notify::APP_TITLE.to_string();
                            notification.body = quill::notify::GENERIC_BODY.to_string();
                            photo = None;
                        }
                        app.show_notification_with_photo(
                            key.0.clone(),
                            notification,
                            photo,
                            path,
                            now_locked,
                            cx,
                        );
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
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
