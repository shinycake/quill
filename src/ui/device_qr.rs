//! A bundled native camera scanner. Its pipe never enters diagnostics; linking
//! waits for a separate affirmative action in the account's Sessions dialog.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use zeroize::Zeroizing;

pub(super) struct DeviceQrScanner(Child);
pub(super) enum ScanEvent {
    Login(Zeroizing<String>),
    Cancelled,
    Failed,
}
impl DeviceQrScanner {
    fn start() -> Result<Self, &'static str> {
        if !cfg!(target_os = "macos") {
            return Err(tr(keys::Q_MISC_CAMERA_DEVICE_LINKING_IS_AVAILABLE));
        }
        let exe = std::env::current_exe()
            .map_err(|_| tr(keys::Q_MISC_THE_CAMERA_SCANNER_IS_UNAVAILABLE))?;
        let sibling = exe.with_file_name("quill-qr-scanner");
        let developer = exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("qr-scanner/quill-qr-scanner"));
        let path = std::iter::once(sibling)
            .chain(developer)
            .find(|p| p.is_file())
            .ok_or(tr(keys::Q_MISC_THE_CAMERA_SCANNER_IS_MISSING))?;
        Command::new(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map(Self)
            .map_err(|_| tr(keys::Q_MISC_THE_CAMERA_SCANNER_COULDNT_START))
    }
    fn poll(&mut self) -> Option<ScanEvent> {
        let status = match self.0.try_wait() {
            Ok(None) => return None,
            Ok(Some(status)) => status,
            Err(_) => return Some(ScanEvent::Failed),
        };
        if status.code() == Some(2) {
            return Some(ScanEvent::Cancelled);
        }
        if !status.success() {
            return Some(ScanEvent::Failed);
        }
        let mut link = Zeroizing::new(String::new());
        let Some(output) = self.0.stdout.take() else {
            return Some(ScanEvent::Failed);
        };
        if output.take(1501).read_to_string(&mut link).is_err()
            || !quill::auth::is_device_login_qr(&link)
        {
            return Some(ScanEvent::Failed);
        }
        Some(ScanEvent::Login(link))
    }
}
impl Drop for DeviceQrScanner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Where linking another device stands: one value, so a scanner, the link it
/// read and the notice cannot disagree. Every stage has a way out: the
/// camera's answer, the consent, the cancel, the account's answer, or the
/// page being left (`clear_device_qr`).
pub(super) enum DeviceLink {
    /// Nothing runs; the notice is the last attempt's outcome, if any.
    Idle { notice: Option<&'static str> },
    /// The camera scanner runs.
    Scanning(DeviceQrScanner),
    /// A scanned or pasted link waits for the user's explicit consent.
    Confirm(Zeroizing<String>),
    /// The link went to Telegram; its answer ends this stage.
    Linking,
}

impl Default for DeviceLink {
    fn default() -> Self {
        Self::Idle { notice: None }
    }
}

impl DeviceLink {
    /// The line the Devices page shows above the link controls.
    pub(super) fn notice(&self) -> Option<&'static str> {
        match self {
            Self::Idle { notice } => *notice,
            Self::Linking => Some(tr(keys::Q_MISC_WAITING_FOR_TELEGRAM_TO_LINK)),
            Self::Scanning(_) | Self::Confirm(_) => None,
        }
    }

    pub(super) fn is_confirming(&self) -> bool {
        matches!(self, Self::Confirm(_))
    }

    pub(super) fn is_scanning(&self) -> bool {
        matches!(self, Self::Scanning(_))
    }

    /// A scan, a consent or the account's answer is pending: no new link may start.
    pub(super) fn is_busy(&self) -> bool {
        !matches!(self, Self::Idle { .. })
    }

    fn failed(notice: &'static str) -> Self {
        Self::Idle {
            notice: Some(notice),
        }
    }
}

impl QuillApp {
    pub(super) fn scan_device_qr(&mut self, cx: &mut Context<Self>) {
        if self.privacy.device_link.is_busy()
            || !self.live.as_ref().is_some_and(|live| {
                matches!(
                    live.driver.session.auth,
                    quill::telegram::envelope::AuthorizationState::Ready
                )
            })
        {
            return;
        }
        self.privacy.device_link = match DeviceQrScanner::start() {
            Ok(scanner) => DeviceLink::Scanning(scanner),
            Err(error) => DeviceLink::failed(error),
        };
        cx.notify();
    }
    pub(super) fn poll_device_qr(&mut self, cx: &mut Context<Self>) {
        if self.settings.page != Some(quill::settings_catalog::Page::Devices)
            || !self.live.as_ref().is_some_and(|live| {
                matches!(
                    live.driver.session.auth,
                    quill::telegram::envelope::AuthorizationState::Ready
                )
            })
        {
            self.clear_device_qr();
            return;
        }
        if let Some(linked) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.settings.device_login_result.take())
        {
            // The answer is for the link this stage sent; one that arrives
            // with nothing asking is dropped.
            if matches!(self.privacy.device_link, DeviceLink::Linking) {
                self.privacy.device_link = DeviceLink::failed(match linked {
                    quill::auth::DeviceLoginResult::Linked => tr(keys::Q_MISC_DEVICE_LINKED),
                    quill::auth::DeviceLoginResult::PasswordRequired => {
                        tr(keys::Q_MISC_QR_CODE_ACCEPTED_ENTER_YOUR)
                    }
                    quill::auth::DeviceLoginResult::Failed => {
                        tr(keys::Q_MISC_THE_DEVICE_COULDNT_BE_LINKED)
                    }
                });
                cx.notify();
            }
        }
        let DeviceLink::Scanning(scanner) = &mut self.privacy.device_link else {
            return;
        };
        let Some(event) = scanner.poll() else {
            return;
        };
        self.privacy.device_link = match event {
            ScanEvent::Login(link) => DeviceLink::Confirm(link),
            ScanEvent::Cancelled => DeviceLink::default(),
            ScanEvent::Failed => DeviceLink::failed(tr(keys::Q_MISC_COULDNT_READ_A_TELEGRAM_LOGIN)),
        };
        cx.activate(true);
        cx.notify();
    }
    /// Link a device from a `tg://login?token=...` link on the clipboard
    /// (no camera needed, so it works on every platform). The link goes
    /// through the same validation and explicit consent step as a scan; it
    /// is never logged.
    pub(super) fn paste_device_login_link(&mut self, cx: &mut Context<Self>) {
        if self.privacy.device_link.is_busy()
            || !self.live.as_ref().is_some_and(|live| {
                matches!(
                    live.driver.session.auth,
                    quill::telegram::envelope::AuthorizationState::Ready
                )
            })
        {
            return;
        }
        let link = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .map(|text| Zeroizing::new(text.trim().to_string()));
        match link {
            Some(link) if quill::auth::is_device_login_qr(&link) => {
                self.privacy.device_link = DeviceLink::Confirm(link);
            }
            _ => {
                self.privacy.device_link =
                    DeviceLink::failed(tr(keys::Q_LEFTOVERS_DEVICE_LINK_COPY_FIRST));
            }
        }
        cx.notify();
    }
    pub(super) fn confirm_scanned_device(&mut self, cx: &mut Context<Self>) {
        if !self.privacy.device_link.is_confirming() {
            return;
        }
        let DeviceLink::Confirm(link) = std::mem::take(&mut self.privacy.device_link) else {
            return;
        };
        let result = self
            .live
            .as_mut()
            .map(|live| live.driver.confirm_device_login(&link));
        self.privacy.device_link = if matches!(result, Some(Ok(_))) {
            DeviceLink::Linking
        } else {
            DeviceLink::failed(tr(keys::Q_MISC_THE_DEVICE_COULDNT_BE_LINKED))
        };
        cx.notify();
    }
    /// The link flow ends (the page was left, the app locked, the account
    /// changed): the answer of a link attempt that is still out goes with
    /// it, so the next visit does not show it as news.
    pub(super) fn clear_device_qr(&mut self) {
        self.privacy.device_link = DeviceLink::default();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.device_login_result = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linking_is_busy_so_no_new_link_replaces_the_pending_answer() {
        assert!(!DeviceLink::default().is_busy());
        assert!(!DeviceLink::failed("x").is_busy());
        assert!(DeviceLink::Linking.is_busy());
        assert!(DeviceLink::Confirm(Zeroizing::new(String::new())).is_busy());
    }
}
