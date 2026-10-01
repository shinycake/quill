use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;

impl QuillApp {
    fn load_marketplace_gift(&mut self, cx: &mut Context<Self>) {
        let name = self
            .marketplace_name_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let chat = self.session().and_then(|s| s.open_chat);
        let result = chat.and_then(|chat| {
            self.live
                .as_mut()
                .map(|live| live.driver.fetch_marketplace_gift(chat, &name))
        });
        if !matches!(result, Some(Ok(_))) {
            self.marketplace_error=Some("Choose a private chat or channel and enter a valid collectible gift name. Wait for pending gift requests before reloading.".into());
        } else {
            self.marketplace_error = None;
        }
        cx.notify();
    }
    pub(super) fn build_marketplace_dialog(
        app: &Entity<Self>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close =
            QuillShell::on_close_kind(app, shell, DialogKind::Marketplace, |this, _, cx| {
                this.marketplace_open = false;
                cx.notify();
            });
        app.update(cx,|this,cx|{
            let gift=this.session().and_then(|s|s.marketplace_gift.clone());
            let limit=this.session().and_then(|s|s.gift_text_length_max);
            let busy=gift.as_ref().is_some_and(|g|g.loading || g.sending);
            let mut body=div().flex().flex_col().gap_3()
                .child(div().id("gift-instructions").role(Role::Label).aria_label("Load a collectible by its Telegram gift name, then review the recipient and price before buying.").child("Load a collectible by its Telegram gift name, then review the recipient and price before buying."))
                .child(Textarea::new(&this.marketplace_name_input).disabled(busy).aria_label("Collectible gift name"))
                .child(Button::new("gift-load").label("Load gift").disabled(busy).on_click(cx.listener(|this,_,_,cx|this.load_marketplace_gift(cx))));
            if let Some(error)=this.marketplace_error.clone() { body=body.child(div().id("gift-error").role(Role::Label).aria_label(error.clone()).child(error)); }
            if let Some(gift)=gift {
                body=body.child(div().id("gift-recipient").role(Role::Label).aria_label(format!("Recipient: {}",gift.recipient_name)).child(format!("Recipient: {}",gift.recipient_name)));
                if gift.loading { body=body.child(div().child("Loading the gift…")); }
                if let Some(note)=gift.note { body=body.child(div().id("gift-result").role(Role::Label).aria_label(note.clone()).child(note)); }
                if let Some(quote)=gift.quote {
                    let title=format!("{} — {}",quote.title,quote.name);
                    body=body.child(div().id("gift-title").role(Role::Label).aria_label(title.clone()).font_semibold().child(title));
                    for price in [quote.stars,quote.ton].into_iter().flatten() {
                        body=body.child(Button::new(format!("gift-price-{}",price.label())).label(price.label()).selected(gift.price==Some(price)).disabled(busy || gift.completed).on_click(cx.listener(move|this,_,_,cx|{
                            if let Some(live)=this.live.as_mut() && let Some(g)=live.driver.session.marketplace_gift.as_mut() && !g.sending { g.price=Some(price); }
                            cx.notify();
                        })));
                    }
                    body=body.child(Textarea::new(&this.marketplace_comment_input).disabled(busy || gift.completed).aria_label("Personal comment"))
                        .child(div().text_xs().child(limit.map(|l|format!("Up to {l} characters. Paid-message recipients may require an empty comment.")).unwrap_or("Comment limit unavailable. Reload the gift before sending a comment.".into())))
                        .child(Button::new("gift-private").label(if this.marketplace_private { "Comment and sender: receiver only" } else { "Comment and sender: visible to everyone" }).selected(this.marketplace_private).disabled(busy || gift.completed).on_click(cx.listener(|this,_,_,cx|{this.marketplace_private=!this.marketplace_private;cx.notify();})));
                    if let Some(price)=gift.price {
                        let quoted_name=quote.name.clone();
                        body=body.child(Button::new("gift-buy").label(format!("Buy for {} and send to {}",price.label(),gift.recipient_name)).disabled(busy || gift.completed).on_click(cx.listener(move|this,_,_,cx|{
                            if this.marketplace_name_input.read(cx).value().trim()!=quoted_name {
                                this.marketplace_error=Some("Load the newly entered gift before buying.".into());
                                cx.notify();
                                return;
                            }
                            let comment=this.marketplace_comment_input.read(cx).value().to_string();
                            let private=this.marketplace_private;
                            if let Some(live)=this.live.as_mut() {
                                this.marketplace_error=if live.driver.buy_marketplace_gift(price,&comment,private).is_err() { Some("Purchase refused. Review the quote and comment length, then retry.".into()) } else { None };
                            }
                            cx.notify();
                        })));
                    }
                }
            }
            dialog.overlay(true).title("Marketplace collectible gift").child(body).on_close(close)
        })
    }
}
