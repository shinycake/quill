//! Open or closed state of the shared "not supported in Quill" notice.

use super::TonWalletState;

/// Where the notice's button points: the official apps page.
pub const OFFICIAL_APPS_URL: &str = "https://telegram.org/apps";

/// The place that asked for the notice. It picks the dialog title and keeps
/// every entry point visible to tests.
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

impl WalletSurface {
    /// The surface is an action on the Money page, so the dialog keeps the
    /// "Money" title; elsewhere it has no title.
    pub fn on_money_page(self) -> bool {
        matches!(
            self,
            Self::Send
                | Self::Receive
                | Self::AddFunds
                | Self::Backup
                | Self::CreateWallet
                | Self::SendNft
        )
    }
}

impl TonWalletState {
    /// Open the notice for `surface`.
    pub fn show_notice(&mut self, surface: WalletSurface) {
        self.notice = Some(surface);
    }

    /// Close the notice.
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
    fn only_money_page_actions_keep_the_money_title() {
        assert!(WalletSurface::Send.on_money_page());
        assert!(WalletSurface::SendNft.on_money_page());
        assert!(!WalletSurface::SendMoney.on_money_page());
        assert!(!WalletSurface::GiftToWallet.on_money_page());
        assert!(!WalletSurface::TransferLink.on_money_page());
    }

    #[test]
    fn the_button_points_at_the_official_apps_page() {
        assert_eq!(OFFICIAL_APPS_URL, "https://telegram.org/apps");
    }
}
