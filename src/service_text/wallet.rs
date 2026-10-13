//! Service wording of the TON wallet messages, following the official iOS
//! app (`Notification.GramTransfer`, `Notification.GramTransferYou`,
//! `Notification.GramTransferUnknown`); Telegram Desktop has no wallet.
use super::*;
use crate::ton_wallet::{SERVICE_NOTIFICATIONS_CHAT_ID, TonWalletTransfer, format_grams};

/// "Dana sent you 5 Grams", "You sent Dana 5 Grams", and in the service
/// chat "Someone sent you 5 Grams". The name links to the profile.
pub(super) fn transfer_line(transfer: &TonWalletTransfer, ctx: &ServiceCtx<'_>) -> ServiceText {
    let amount = format_grams(transfer.amount);
    if transfer.is_received() {
        if ctx.chat_id == SERVICE_NOTIFICATIONS_CHAT_ID {
            return B::default().t(format!("{WHO} sent you {amount}")).done();
        }
        // The sender by short name; else, as the iOS app, the chat's
        // other side.
        let sender = match (transfer.sender_user_id, ctx.actor) {
            (0, Some(MessageSender::User { user_id })) => user_id,
            (id, _) => id,
        };
        let mut peer = if sender != 0 && ctx.names.user_short_name(sender).is_some() {
            ctx.user_short(sender)
        } else {
            conversation_peer(ctx)
        };
        if peer.0.is_empty() {
            peer = B::default().t(WHO);
        }
        peer.t(format!(" sent you {amount}")).done()
    } else {
        let peer = conversation_peer(ctx);
        if peer.0.is_empty() {
            B::default().t(format!("You sent {amount}")).done()
        } else {
            B::default()
                .t("You sent ")
                .extend(peer)
                .t(format!(" {amount}"))
                .done()
        }
    }
}

/// The other side of the chat the transfer message is in (a private
/// chat's id is the user's id), by its short name; empty when unknown.
fn conversation_peer(ctx: &ServiceCtx<'_>) -> B {
    let user = ctx
        .names
        .user_short_name(ctx.chat_id)
        .filter(|name| !name.trim().is_empty())
        .map(|name| {
            (
                name,
                MessageSender::User {
                    user_id: ctx.chat_id,
                },
            )
        });
    let peer = user.or_else(|| {
        ctx.names
            .chat_name(ctx.chat_id)
            .filter(|name| !name.trim().is_empty())
            .map(|name| {
                (
                    name,
                    MessageSender::Chat {
                        chat_id: ctx.chat_id,
                    },
                )
            })
    });
    match peer {
        Some((name, sender)) => B::default().link(name, ServiceLink::Sender(sender)),
        None => B::default(),
    }
}
