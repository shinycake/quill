//! Open or closed state of the shared "not supported in Quill" notice.

use super::TonWalletState;

pub const OFFICIAL_APPS_URL: &str = "https://telegram.org/apps";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalletSurface {
    TransferLink,
    ConnectLink,
    Send,
    Receive,
    AddFunds,
    Backup,
    CreateWallet,
    EncryptedComment,
    SendNft,
    ConnectRequest,
    SendMoney,
    GiftToWallet,
}

impl TonWalletState {
    pub fn show_notice(&mut self, surface: WalletSurface) {
        self.notice = Some(surface);
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_opens_for_a_surface_and_closes() {
        let mut wallet = TonWalletState::default();
        assert_eq!(wallet.notice, None);
        wallet.show_notice(WalletSurface::Send);
        assert_eq!(wallet.notice, Some(WalletSurface::Send));
        wallet.dismiss_notice();
        assert_eq!(wallet.notice, None);
    }

    #[test]
    fn the_button_points_at_the_official_apps_page() {
        assert_eq!(OFFICIAL_APPS_URL, "https://telegram.org/apps");
    }
}
