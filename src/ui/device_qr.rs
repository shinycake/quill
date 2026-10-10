//! A bundled native camera scanner. Its pipe never enters diagnostics; linking
//! waits for a separate affirmative action in the account's Sessions dialog.
use super::app::QuillApp;
use gpui_kit::*;
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
            return Err("Camera device linking is available on macOS.");
        }
        let exe = std::env::current_exe().map_err(|_| "The camera scanner is unavailable.")?;
        let sibling = exe.with_file_name("quill-qr-scanner");
        let developer = exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("qr-scanner/quill-qr-scanner"));
        let path = std::iter::once(sibling)
            .chain(developer)
            .find(|p| p.is_file())
            .ok_or("The camera scanner is missing. Rebuild the Quill app bundle.")?;
        Command::new(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map(Self)
            .map_err(|_| "The camera scanner couldn't start.")
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
impl QuillApp {
    pub(super) fn scan_device_qr(&mut self, cx: &mut Context<Self>) {
        if self.privacy.device_qr_scanner.is_some()
            || self.privacy.device_login_qr.is_some()
            || !self.live.as_ref().is_some_and(|live| {
                matches!(
                    live.driver.session.auth,
                    quill::telegram::envelope::AuthorizationState::Ready
                )
            })
        {
            return;
        }
        self.privacy.device_link_notice = None;
        match DeviceQrScanner::start() {
            Ok(scanner) => self.privacy.device_qr_scanner = Some(scanner),
            Err(error) => self.privacy.device_link_notice = Some(error),
        }
        cx.notify();
    }
    pub(super) fn poll_device_qr(&mut self, cx: &mut Context<Self>) {
        if !self.privacy.sessions_open
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
            .and_then(|live| live.driver.session.device_login_result.take())
        {
            self.privacy.device_link_notice = Some(match linked {
                quill::auth::DeviceLoginResult::Linked => "Device linked.",
                quill::auth::DeviceLoginResult::PasswordRequired => {
                    "QR code accepted. Enter your two-step verification password on the other device to finish signing in."
                }
                quill::auth::DeviceLoginResult::Failed => {
                    "The device couldn't be linked. Scan a fresh login QR code and try again."
                }
            });
            cx.notify();
        }
        let event = self
            .privacy
            .device_qr_scanner
            .as_mut()
            .and_then(DeviceQrScanner::poll);
        let Some(event) = event else {
            return;
        };
        self.privacy.device_qr_scanner = None;
        match event {
            ScanEvent::Login(link) => self.privacy.device_login_qr = Some(link),
            ScanEvent::Cancelled => {}
            ScanEvent::Failed => {
                self.privacy.device_link_notice =
                    Some("Couldn't read a Telegram login QR code. Please try again.")
            }
        }
        cx.activate(true);
        cx.notify();
    }
    /// Link a device from a `tg://login?token=...` link on the clipboard
    /// (no camera needed, so it works on every platform). The link goes
    /// through the same validation and explicit consent step as a scan; it
    /// is never logged.
    pub(super) fn paste_device_login_link(&mut self, cx: &mut Context<Self>) {
        if self.privacy.device_login_qr.is_some()
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
                self.privacy.device_link_notice = None;
                self.privacy.device_login_qr = Some(link);
            }
            _ => {
                self.privacy.device_link_notice = Some(
                    "Copy the tg://login link from the other device's QR code first, then try again.",
                );
            }
        }
        cx.notify();
    }
    pub(super) fn confirm_scanned_device(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.privacy.device_login_qr.take() else {
            return;
        };
        let result = self
            .live
            .as_mut()
            .map(|live| live.driver.confirm_device_login(&link));
        self.privacy.device_link_notice = Some(if matches!(result, Some(Ok(_))) {
            "Waiting for Telegram to link the device…"
        } else {
            "The device couldn't be linked. Scan a fresh login QR code and try again."
        });
        cx.notify();
    }
    pub(super) fn clear_device_qr(&mut self) {
        self.privacy.device_qr_scanner = None;
        self.privacy.device_login_qr = None;
        self.privacy.device_link_notice = None;
    }
}
