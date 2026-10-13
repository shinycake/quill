//! Reducer for the read-only TON wallet (`crate::ton_wallet`): the own
//! state, history, NFTs, profile addresses and exchange rates. Answers only
//! land when they match the request that asked for them.
use crate::state::*;
use crate::ton_wallet::{ProfileAddress, TonWalletPayload, TonWalletPurpose, WalletTx};

impl Session {
    pub(crate) fn apply_ton_wallet(
        &mut self,
        payload: TonWalletPayload,
        pending: Option<&PendingRequest>,
    ) {
        let purpose = match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Payments(PaymentsPurpose::TonWallet(purpose))) => Some(purpose),
            _ => None,
        };
        let wallet = &mut self.ton_wallet;
        match payload {
            // An update: TDLib sends it after `loadTonWalletState` and on
            // every change, so it applies whenever the wallet is on.
            TonWalletPayload::State(state) => {
                if wallet.can_use {
                    wallet.own = Some(state);
                }
            }
            TonWalletPayload::ExchangeRates(rates) => {
                if purpose == Some(TonWalletPurpose::ExchangeRates) {
                    wallet.rates = rates;
                }
            }
            TonWalletPayload::Transactions(page) => {
                let Some(TonWalletPurpose::Transactions { append }) = purpose else {
                    return;
                };
                if let Some(own) = wallet.own.as_mut() {
                    own.gram_amount = page.gram_amount;
                }
                wallet
                    .money
                    .history
                    .apply(page.transactions, page.next_offset, append);
                self.want_wallet_thumbs();
            }
            TonWalletPayload::Nfts(page) => {
                let Some(TonWalletPurpose::Nfts { append }) = purpose else {
                    return;
                };
                wallet.money.nfts.apply(page.nfts, page.next_offset, append);
                self.want_wallet_thumbs();
            }
            TonWalletPayload::UserAddresses(found) => {
                if purpose != Some(TonWalletPurpose::UserAddress) {
                    return;
                }
                for (user_id, address) in found {
                    wallet
                        .profile_addresses
                        .insert(user_id, ProfileAddress::Found(address));
                }
                // TDLib leaves out a user without a wallet.
                if let Some(user_id) = pending.and_then(|p| p.user_id)
                    && matches!(
                        wallet.profile_addresses.get(&user_id),
                        Some(ProfileAddress::Loading)
                    )
                {
                    wallet
                        .profile_addresses
                        .insert(user_id, ProfileAddress::None);
                }
            }
            TonWalletPayload::Transaction(tx) => {
                if purpose != Some(TonWalletPurpose::Transaction) || !wallet.money.tx_loading {
                    return;
                }
                wallet.money.tx_loading = false;
                self.show_wallet_transaction(*tx);
            }
        }
    }

    /// Show `tx`'s details. An incoming Gram transfer brings up "Your
    /// first Grams!" once, while TDLib still suggests it (the iOS
    /// transaction screen's `checkFirstGramsOnAppear`).
    pub fn show_wallet_transaction(&mut self, tx: WalletTx) {
        let hint = tx.is_incoming_gram_transfer()
            && self
                .chat_list
                .suggestions
                .actions
                .contains(crate::ton_wallet::FIRST_TRANSFER_HINT);
        let money = &mut self.ton_wallet.money;
        money.tx_error = None;
        money.selected_nft = None;
        money.selected_tx = Some(tx);
        if hint {
            money.hint_open = true;
        }
    }

    /// The NFT thumbnails not on disk yet (the next ingest downloads them).
    fn want_wallet_thumbs(&mut self) {
        let money = &self.ton_wallet.money;
        let files: Vec<_> = money
            .nfts
            .items
            .iter()
            .chain(money.history.items.iter().filter_map(|tx| match &tx.kind {
                crate::ton_wallet::TxKind::NftTransfer { nft, .. } => Some(nft.as_ref()),
                _ => None,
            }))
            .filter_map(|nft| nft.thumbnail.clone())
            .collect();
        self.remember_files(&files);
        self.media
            .profile_thumbs_due
            .extend(files.iter().map(|file| file.id).filter(|id| id.0 != 0));
    }

    /// A wallet request failed.
    pub(crate) fn apply_ton_wallet_error(
        &mut self,
        err: &TdError,
        purpose: TonWalletPurpose,
        user_id: Option<i64>,
    ) {
        let reason = error_reason(err);
        let wallet = &mut self.ton_wallet;
        match purpose {
            TonWalletPurpose::LoadState => wallet.state_requested = false,
            TonWalletPurpose::ExchangeRates => wallet.rates_requested = false,
            TonWalletPurpose::Transactions { .. } => wallet
                .money
                .history
                .fail(format!("Couldn't load transactions: {reason}")),
            TonWalletPurpose::Nfts { .. } => wallet
                .money
                .nfts
                .fail(format!("Couldn't load collectibles: {reason}")),
            // A failed lookup is the same as no wallet: no row.
            TonWalletPurpose::UserAddress => {
                if let Some(user_id) = user_id {
                    wallet
                        .profile_addresses
                        .insert(user_id, ProfileAddress::None);
                }
            }
            TonWalletPurpose::Transaction => {
                wallet.money.tx_loading = false;
                wallet.money.tx_error = Some(format!("Couldn't load the transfer: {reason}"));
            }
        }
    }
}
