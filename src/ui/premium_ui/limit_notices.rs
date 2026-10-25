//! The Premium limits tdesktop tells about without a box: the toasts for a
//! full saved GIF or favorite sticker list (`data_stickers.cpp`
//! `SavedGifsToast`, `FaveStickersToast`), the locked "More Channels" tile
//! (`history_view_similar_channels.cpp`), the bot limit
//! (`create_managed_bot_box.cpp`), and the "Show more channels" unlock
//! under a channel's similar channels (`info_similar_peers_widget.cpp`
//! `ListController::setupUnlock`). Links in them open Premium, or
//! @BotFather's delete screen.

use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use quill::ids::{ChatId, FileId};
use quill::lang::{trn, trn_with};
use quill::owned_bots::BOT_MANAGER_USERNAME;
use quill::premium_limits::{
    BotsLimitNotice, PremiumLimitKind, bots_limit_notice, saved_list_full,
};
use quill::state::{DeepLinkAction, ProfileChatsFetch, ProfileChatsKind};
use std::ops::Range;

/// @BotFather's start parameter for deleting a bot (tdesktop links
/// `https://t.me/botfather?start=deletebot`).
const DELETE_BOT_START: &str = "deletebot";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LimitLink {
    Premium(PremiumLimitKind),
    BotFather,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct MarkedText {
    pub(super) text: String,
    pub(super) bold: Vec<Range<usize>>,
    pub(super) links: Vec<(Range<usize>, LimitLink)>,
}

/// `template` with `**` toggling bold and each `{tag}` named in `links`
/// replaced by that link's text; other tags stay as written.
pub(super) fn marked(template: &str, links: &[(&str, &str, LimitLink)]) -> MarkedText {
    let mut out = MarkedText::default();
    let mut bold_from: Option<usize> = None;
    let mut rest = template;
    while !rest.is_empty() {
        let next = [rest.find("**"), rest.find('{')]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(rest.len());
        let Some((plain, tail)) = rest.split_at_checked(next) else {
            out.text.push_str(rest);
            break;
        };
        out.text.push_str(plain);
        if let Some(after) = tail.strip_prefix("**") {
            match bold_from.take() {
                Some(start) => out.bold.push(start..out.text.len()),
                None => bold_from = Some(out.text.len()),
            }
            rest = after;
        } else if let Some(after) = tail.strip_prefix('{') {
            let Some((tag, after_tag)) = after.split_once('}') else {
                out.text.push('{');
                rest = after;
                continue;
            };
            match links.iter().find(|(name, _, _)| *name == tag) {
                Some((_, text, link)) => {
                    let start = out.text.len();
                    out.text.push_str(text);
                    out.links.push((start..out.text.len(), *link));
                }
                None => {
                    out.text.push('{');
                    out.text.push_str(tag);
                    out.text.push('}');
                }
            }
            rest = after_tag;
        } else {
            rest = tail;
        }
    }
    if let Some(start) = bold_from {
        out.bold.push(start..out.text.len());
    }
    out
}

/// A limit toast: its text and whether it carries tdesktop's Premium star
/// (`ShowPremiumPromoToast`; the saved-list toasts have none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LimitToastText {
    pub(super) text: MarkedText,
    pub(super) star: bool,
}

pub(super) fn limit_toast_text(
    session: &Session,
    kind: PremiumLimitKind,
) -> Option<LimitToastText> {
    let pair = session.premium_limit_pair(kind);
    let replaced = |title, more, link| {
        let template = format!(
            "**{}**\n{}",
            trn(title, pair.default_value),
            trn(more, pair.premium_value)
        );
        LimitToastText {
            text: marked(&template, &[("link", tr(link), LimitLink::Premium(kind))]),
            star: false,
        }
    };
    let bot = "@BotFather";
    Some(match kind {
        PremiumLimitKind::SavedGifs => replaced(
            keys::SAVED_GIF_LIMIT_TITLE,
            keys::SAVED_GIF_LIMIT_MORE,
            keys::SAVED_GIF_LIMIT_LINK,
        ),
        PremiumLimitKind::FavoriteStickers => replaced(
            keys::FAVE_STICKER_LIMIT_TITLE,
            keys::FAVE_STICKER_LIMIT_MORE,
            keys::FAVE_STICKER_LIMIT_LINK,
        ),
        PremiumLimitKind::SimilarChats => LimitToastText {
            text: marked(
                &trn(keys::SIMILAR_CHANNELS_PREMIUM_ALL, pair.premium_value),
                &[(
                    "link",
                    tr(keys::SIMILAR_CHANNELS_PREMIUM_ALL_LINK),
                    LimitLink::Premium(kind),
                )],
            ),
            star: true,
        },
        PremiumLimitKind::OwnedBots => match bots_limit_notice(
            session.premium_limit_in_force(kind),
            pair.premium_value,
            session.my_is_premium(),
            session.premium_possible(),
        ) {
            BotsLimitNotice::Final { count } => LimitToastText {
                text: marked(
                    &trn(keys::BOTS_CREATE_LIMIT_FINAL, count),
                    &[("bot", bot, LimitLink::BotFather)],
                ),
                star: false,
            },
            BotsLimitNotice::Promo {
                count,
                premium_count,
            } => LimitToastText {
                text: marked(
                    &trn_with(
                        keys::BOTS_CREATE_LIMIT,
                        count,
                        &[("premium_count", &format!("**{premium_count}**"))],
                    ),
                    &[
                        (
                            "link",
                            tr(keys::BOTS_CREATE_LIMIT_LINK),
                            LimitLink::Premium(kind),
                        ),
                        ("bot", bot, LimitLink::BotFather),
                    ],
                ),
                star: true,
            },
        },
        _ => return None,
    })
}

impl QuillApp {
    pub(in crate::ui) fn premium_limit_toast_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.hub_mut().and_then(|hub| hub.limit_toast.take()) else {
            return;
        };
        let Some(toast) = self.session().and_then(|s| limit_toast_text(s, kind)) else {
            return;
        };
        let app = cx.entity().downgrade();
        let text = toast.text;
        let notification = Notification::new()
            .when(toast.star, |toast| toast.icon(IconName::Star))
            .content(move |_, _, cx| marked_element("premium-limit-toast", &text, app.clone(), cx));
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(notification, cx);
        });
    }

    fn follow_limit_link(&mut self, link: LimitLink, cx: &mut Context<Self>) {
        match link {
            LimitLink::Premium(kind) => self.open_premium_from(&kind.premium_ref(), cx),
            LimitLink::BotFather => {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.resolve_deep_link(DeepLinkAction::OpenUsername {
                        domain: BOT_MANAGER_USERNAME.to_string(),
                        start_param: Some(DELETE_BOT_START.to_string()),
                        post: None,
                        story_id: None,
                    });
                }
                cx.notify();
            }
        }
    }

    /// After "Add to GIFs" on a full list: TDLib dropped the oldest GIF,
    /// and tdesktop tells a free account so (`Stickers::addSavedGif`).
    pub(in crate::ui) fn note_gif_replaced(&mut self, cx: &mut Context<Self>) {
        let full = self.session().is_some_and(|s| {
            !s.my_is_premium()
                && s.stickers.gifs.loaded
                && saved_list_full(
                    s.stickers.gifs.animations.len(),
                    s.premium_limit_in_force(PremiumLimitKind::SavedGifs),
                )
        });
        if full {
            self.show_premium_limit(PremiumLimitKind::SavedGifs, 0, cx);
        }
    }

    /// After "Add to Favorites" on a full list (tdesktop
    /// `Stickers::checkFavedLimit`): the same toast for stickers.
    pub(in crate::ui) fn note_faved_replaced(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        let full = self.session().is_some_and(|s| {
            let favorites = &s.stickers.stickers.favorites;
            !s.my_is_premium()
                && !favorites.iter().any(|sticker| sticker.file_id == file_id)
                && saved_list_full(
                    favorites.len(),
                    s.premium_limit_in_force(PremiumLimitKind::FavoriteStickers),
                )
        });
        if full {
            self.show_premium_limit(PremiumLimitKind::FavoriteStickers, 0, cx);
        }
    }

    pub(in crate::ui) fn similar_unlock(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if session.my_is_premium() || !session.premium_possible() {
            return None;
        }
        let ProfileChatsFetch::Loaded(list) = session
            .users_state
            .profile_chat_lists
            .get(&(ProfileChatsKind::SimilarChats, chat_id.0))?
        else {
            return None;
        };
        let total = session.users_state.similar_chat_totals.get(&chat_id.0)?;
        if usize::try_from(*total).unwrap_or(0) <= list.len() {
            return None;
        }
        let upto = session
            .premium_limit_pair(PremiumLimitKind::SimilarChats)
            .premium_value;
        let about = marked(
            &trn(keys::SIMILAR_CHANNELS_PREMIUM_ALL, upto),
            &[(
                "link",
                tr(keys::SIMILAR_CHANNELS_PREMIUM_ALL_LINK),
                LimitLink::Premium(PremiumLimitKind::SimilarChats),
            )],
        );
        let label = tr(keys::SIMILAR_CHANNELS_SHOW_MORE);
        Some(
            v_flex()
                .w_full()
                .gap_2()
                .pt_2()
                .items_center()
                .child(
                    Button::new("similar-unlock")
                        .debug_selector(|| "similar-unlock".into())
                        .primary()
                        .w_full()
                        .accessibility_label(label)
                        .child(div().font_semibold().child(label))
                        .child(Icon::new(IconName::Lock).with_size(px(16.)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_premium_from(
                                &PremiumLimitKind::SimilarChats.premium_ref(),
                                cx,
                            )
                        })),
                )
                .child(
                    div()
                        .id("similar-unlock-about")
                        .text_xs()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child(marked_element(
                            "similar-unlock-about-text",
                            &about,
                            cx.entity().downgrade(),
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }
}

fn marked_element(
    id: &'static str,
    text: &MarkedText,
    app: WeakEntity<QuillApp>,
    cx: &App,
) -> AnyElement {
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let link = HighlightStyle {
        color: Some(cx.theme().primary),
        ..bold
    };
    let mut highlights: Vec<_> = text
        .bold
        .iter()
        .filter(|range| {
            !text
                .links
                .iter()
                .any(|(link, _)| link.start < range.end && range.start < link.end)
        })
        .map(|range| (range.clone(), bold))
        .chain(text.links.iter().map(|(range, _)| (range.clone(), link)))
        .collect();
    highlights.sort_by_key(|(range, _)| range.start);
    let targets: Vec<LimitLink> = text.links.iter().map(|(_, target)| *target).collect();
    InteractiveText::new(
        id,
        StyledText::new(text.text.clone()).with_highlights(highlights),
    )
    .on_click(
        text.links.iter().map(|(range, _)| range.clone()).collect(),
        move |ix, _, cx| {
            if let Some(target) = targets.get(ix).copied() {
                let _ = app.update(cx, |this, cx| this.follow_limit_link(target, cx));
            }
        },
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{LimitLink, marked};
    use quill::premium_limits::PremiumLimitKind;
    use std::ops::Range;

    #[test]
    fn bold_runs_and_links_resolve_in_place() {
        let text = marked(
            "Subscribe to {link} to unlock up to **100** similar channels.",
            &[(
                "link",
                "Telegram Premium",
                LimitLink::Premium(PremiumLimitKind::SimilarChats),
            )],
        );
        assert_eq!(
            text.text,
            "Subscribe to Telegram Premium to unlock up to 100 similar channels."
        );
        assert_eq!(
            text.links,
            [(13..29, LimitLink::Premium(PremiumLimitKind::SimilarChats))]
        );
        assert_eq!(text.bold, vec![Range { start: 46, end: 49 }]);
    }

    #[test]
    fn unknown_tags_and_stray_braces_stay() {
        let text = marked(
            "a {x} **b {bot}",
            &[("bot", "@BotFather", LimitLink::BotFather)],
        );
        assert_eq!(text.text, "a {x} b @BotFather");
        assert_eq!(text.links, [(8..18, LimitLink::BotFather)]);
        assert_eq!(text.bold, vec![Range { start: 6, end: 18 }]);
        assert_eq!(marked("{open", &[]).text, "{open");
    }
}
