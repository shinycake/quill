//! Service text: the wording for every `ServiceAction`, including giveaway results and gifts.
use super::gifts::{giveaway_results, render_gift, render_upgraded_gift};
use super::*;
use crate::telegram::envelope::GiftOfferState;

#[allow(clippy::too_many_lines)]
pub fn render_action(action: &ServiceAction, ctx: &ServiceCtx<'_>) -> ServiceText {
    use ServiceAction as A;
    let text = |s: &str| B::from(s).done();
    let from = || ctx.from();
    match action {
        A::ChatCreate { title, supergroup } => {
            if *supergroup && ctx.is_channel {
                say(keys::ACTION_CREATED_CHANNEL)
            } else {
                phrase(
                    keys::ACTION_CREATED_CHAT,
                    &[("from", from()), ("title", B::from(title))],
                )
                .done()
            }
        }
        A::ChatTitle { title } => {
            if ctx.is_channel {
                line(keys::ACTION_CHANGED_TITLE_CHANNEL, &[("title", title)])
            } else {
                phrase(
                    keys::ACTION_CHANGED_TITLE,
                    &[("from", from()), ("title", B::from(title))],
                )
                .done()
            }
        }
        A::ChatPhoto { .. } => {
            if ctx.is_channel {
                say(keys::ACTION_CHANGED_PHOTO_CHANNEL)
            } else {
                phrase(keys::ACTION_CHANGED_PHOTO, &[("from", from())]).done()
            }
        }
        A::ChatPhotoDeleted => {
            if ctx.is_channel {
                say(keys::ACTION_REMOVED_PHOTO_CHANNEL)
            } else {
                phrase(keys::ACTION_REMOVED_PHOTO, &[("from", from())]).done()
            }
        }
        A::OwnerLeft { new_owner } => phrase(
            keys::ACTION_NEW_CREATOR_PENDING,
            &[("user", ctx.user(*new_owner)), ("from", from())],
        )
        .done(),
        A::OwnerChanged { new_owner } => phrase(
            keys::ACTION_CHANGE_CREATOR,
            &[("from", from()), ("user", ctx.user(*new_owner))],
        )
        .done(),
        A::ProtectedToggled {
            enabled,
            was_enabled,
            ..
        } => {
            if enabled == was_enabled {
                say(keys::ACTION_NO_FORWARDS_STILL_DISABLED)
            } else if *enabled {
                ctx.by_actor(
                    keys::ACTION_NO_FORWARDS_YOU_DISABLED,
                    keys::ACTION_NO_FORWARDS_DISABLED,
                    &[],
                )
            } else {
                ctx.by_actor(
                    keys::ACTION_NO_FORWARDS_YOU_ENABLED,
                    keys::ACTION_NO_FORWARDS_ENABLED,
                    &[],
                )
            }
        }
        A::ProtectedDisableRequested { expired } => {
            if *expired {
                say(keys::ACTION_NO_FORWARDS_REQUEST_EXPIRED)
            } else {
                ctx.by_actor(
                    keys::ACTION_NO_FORWARDS_REQUEST_YOU,
                    keys::ACTION_NO_FORWARDS_REQUEST,
                    &[],
                )
            }
        }
        A::AddMembers { users } => match users.as_slice() {
            [only] if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *only) => {
                if ctx.is_channel {
                    say(keys::ACTION_YOU_JOINED)
                } else {
                    phrase(keys::ACTION_USER_JOINED, &[("from", from())]).done()
                }
            }
            [only] if ctx.is_me_user(*only) && !ctx.actor_is_me() => phrase(
                if ctx.is_channel {
                    keys::ACTION_ADD_YOU
                } else {
                    keys::ACTION_ADD_YOU_GROUP
                },
                &[("from", from())],
            )
            .done(),
            [one] => phrase(
                keys::ACTION_ADD_USER,
                &[("from", from()), ("user", ctx.user(*one))],
            )
            .done(),
            users => phrase(
                keys::ACTION_ADD_USERS_MANY,
                &[("from", from()), ("users", ctx.users(users))],
            )
            .done(),
        },
        A::DeleteMember { user } => {
            if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *user) {
                phrase(keys::ACTION_USER_LEFT, &[("from", from())]).done()
            } else {
                phrase(
                    keys::ACTION_KICK_USER,
                    &[("from", from()), ("user", ctx.user(*user))],
                )
                .done()
            }
        }
        A::JoinByLink => phrase(keys::ACTION_USER_JOINED_BY_LINK, &[("from", from())]).done(),
        // tdesktop `lng_action_you_joined`.
        A::YouJoinedChannel => say(keys::ACTION_YOU_JOINED),
        A::NewBotThread => say(keys::BOT_NEW_THREAD_ABOUT),
        // tdesktop `lng_replies_discussion_started` / `lng_replies_no_comments`.
        A::ThreadDivider { with_comments } => text(if *with_comments {
            tr(keys::REPLIES_DISCUSSION_STARTED)
        } else {
            tr(keys::REPLIES_NO_COMMENTS)
        }),
        A::JoinByRequest => {
            if ctx.actor_is_me() {
                say(if ctx.is_channel {
                    keys::ACTION_YOU_JOINED_BY_REQUEST_CHANNEL
                } else {
                    keys::ACTION_YOU_JOINED_BY_REQUEST
                })
            } else {
                phrase(keys::ACTION_USER_JOINED_BY_REQUEST, &[("from", from())]).done()
            }
        }
        A::UpgradedTo => say(keys::Q_SERVICE_UPGRADED_TO),
        A::UpgradedFrom { title } => line(keys::Q_SERVICE_UPGRADED_FROM, &[("title", title)]),
        A::Pin { message_id } => {
            let (excerpt, is_media) = pin_text(ctx.names.pinned(*message_id));
            let link = ServiceLink::Message(*message_id);
            if is_media {
                phrase(
                    keys::ACTION_PINNED_MEDIA,
                    &[
                        ("from", from()),
                        ("media", B::default().link(excerpt, link)),
                    ],
                )
                .done()
            } else {
                phrase(
                    keys::ACTION_PINNED_MESSAGE,
                    &[("from", from()), ("text", B::default().link(excerpt, link))],
                )
                .done()
            }
        }
        A::Background { only_for_self } => {
            if ctx.actor_is_me() {
                if *only_for_self {
                    say(keys::ACTION_SET_WALLPAPER_ME)
                } else {
                    phrase(
                        keys::ACTION_SET_WALLPAPER_BOTH_ME,
                        &[("user", ctx.chat(ctx.chat_id))],
                    )
                    .done()
                }
            } else {
                phrase(keys::ACTION_SET_WALLPAPER, &[("user", from())]).done()
            }
        }
        A::Theme { emoji, gift_name } => match (gift_name, emoji.is_empty()) {
            (Some(name), _) => ctx.by_actor(
                keys::ACTION_YOU_GIFT_THEME_CHANGED,
                keys::ACTION_GIFT_THEME_CHANGED,
                &[("name", B::from(name))],
            ),
            (None, false) => ctx.by_actor(
                keys::ACTION_YOU_THEME_CHANGED,
                keys::ACTION_THEME_CHANGED,
                &[("emoji", B::from(emoji))],
            ),
            (None, true) => ctx.by_actor(
                keys::ACTION_YOU_THEME_DISABLED,
                keys::ACTION_THEME_DISABLED,
                &[],
            ),
        },
        A::Boost { count } => {
            if ctx.actor_is_me() {
                say(keys::ACTION_BOOST_APPLY_ME)
            } else {
                phrase_plural(
                    keys::ACTION_BOOST_APPLY,
                    i64::from(*count),
                    &[("from", from())],
                )
                .done()
            }
        }
        A::TopicCreated { name } => line(keys::ACTION_TOPIC_CREATED, &[("topic", name)]),
        A::TopicEdited {
            name, icon_edited, ..
        } => {
            if name.is_empty() && *icon_edited {
                phrase(keys::Q_SERVICE_TOPIC_ICON_CHANGED, &[("from", from())]).done()
            } else if name.is_empty() {
                say(keys::Q_SERVICE_TOPIC_EDITED)
            } else {
                phrase(
                    keys::ACTION_TOPIC_RENAMED,
                    &[
                        ("from", from()),
                        ("link", B::from(tr(keys::ACTION_TOPIC_PLACEHOLDER))),
                        ("title", B::from(name)),
                    ],
                )
                .done()
            }
        }
        A::TopicClosed { closed } => phrase(
            if *closed {
                keys::ACTION_TOPIC_CLOSED_INSIDE_BY
            } else {
                keys::ACTION_TOPIC_REOPENED_INSIDE_BY
            },
            &[("from", from())],
        )
        .done(),
        A::TopicHidden { hidden } => say(if *hidden {
            keys::ACTION_TOPIC_HIDDEN_INSIDE
        } else {
            keys::ACTION_TOPIC_UNHIDDEN_INSIDE
        }),
        A::SuggestProfilePhoto { .. } => {
            if ctx.actor_is_me() {
                phrase(
                    keys::ACTION_SUGGESTED_PHOTO_ME,
                    &[("user", ctx.chat(ctx.chat_id))],
                )
                .done()
            } else {
                phrase(keys::ACTION_SUGGESTED_PHOTO, &[("user", from())]).done()
            }
        }
        A::SuggestBirthdate { day, month, year } => {
            let date = if *year > 0 {
                format!(
                    "{day} {} {year}",
                    crate::local_time::month_name(*month as u8)
                )
            } else {
                format!("{day} {}", crate::local_time::month_name(*month as u8))
            };
            let (key, who) = if ctx.actor_is_me() {
                (keys::Q_SERVICE_SUGGESTED_BIRTHDAY_ME, ctx.chat(ctx.chat_id))
            } else {
                (keys::Q_SERVICE_SUGGESTED_BIRTHDAY, from())
            };
            phrase(key, &[("user", who), ("date", B::from(date))]).done()
        }
        A::Custom { text: custom } => text(custom),
        A::GameScore {
            game_message_id,
            score,
        } => {
            let game = ctx
                .names
                .game_title(*game_message_id)
                .map(|title| B::default().link(title, ServiceLink::Message(*game_message_id)));
            let score = i64::from(*score);
            match game {
                Some(game) => ctx.by_actor_plural(
                    keys::ACTION_GAME_YOU_SCORED,
                    keys::ACTION_GAME_SCORE,
                    score,
                    &[("game", game)],
                ),
                None => ctx.by_actor_plural(
                    keys::ACTION_GAME_YOU_SCORED_NO_GAME,
                    keys::ACTION_GAME_SCORE_NO_GAME,
                    score,
                    &[],
                ),
            }
        }
        A::ManagedBotCreated { bot_user_id } => phrase(
            keys::ACTION_MANAGED_BOT_CREATED,
            &[("from", from()), ("bot", ctx.user(*bot_user_id))],
        )
        .done(),
        A::PaymentRefunded { owner, money } => {
            let who = match owner {
                Some(owner) => ctx.peer(*owner),
                None => B::from(someone()),
            };
            phrase(
                keys::ACTION_PAYMENT_REFUNDED,
                &[("peer", who), ("amount", B::from(amount(money)))],
            )
            .done()
        }
        A::GiftedPremium {
            gifter,
            receiver: _,
            money,
        } => ctx.gift_line_between(*gifter, &amount(money)),
        A::GiftedStars {
            gifter,
            receiver: _,
            money,
        } => ctx.gift_line_between(*gifter, &amount(money)),
        A::GiftedGrams {
            gifter, grams: g, ..
        } => ctx.gift_line_between(*gifter, &grams_cost(*g)),
        A::PremiumGiftCode {
            creator,
            from_giveaway,
            unclaimed,
            money,
        } => match creator {
            Some(MessageSender::Chat { chat_id }) => {
                if *unclaimed {
                    ctx.channel_prize(keys::PRIZE_UNCLAIMED_ABOUT, *chat_id)
                } else if *from_giveaway {
                    ctx.channel_prize(keys::PRIZE_ABOUT, *chat_id)
                } else {
                    ctx.channel_prize(keys::PRIZE_GIFT_ABOUT, *chat_id)
                }
            }
            Some(MessageSender::User { user_id }) => {
                ctx.gift_line_between(*user_id, &amount(money))
            }
            None => ctx.gift_line(&amount(money)),
        },
        A::GiveawayCreated { stars } => {
            let (key, premium_key) = if ctx.is_channel {
                (
                    keys::ACTION_GIVEAWAY_CREDITS_STARTED,
                    keys::ACTION_GIVEAWAY_STARTED,
                )
            } else {
                (
                    keys::ACTION_GIVEAWAY_CREDITS_STARTED_GROUP,
                    keys::ACTION_GIVEAWAY_STARTED_GROUP,
                )
            };
            if *stars > 0 {
                phrase(
                    key,
                    &[
                        ("from", from()),
                        (
                            "amount",
                            B::default()
                                .t(trn(keys::ACTION_GIVEAWAY_CREDITS_STARTED_AMOUNT, *stars)),
                        ),
                    ],
                )
                .done()
            } else {
                phrase(premium_key, &[("from", from())]).done()
            }
        }
        A::Giveaway {
            winners,
            stars,
            months,
        } => {
            let winners = i64::from(*winners);
            if *stars > 0 {
                phrase_plural(
                    keys::Q_SERVICE_GIVEAWAY_STARS,
                    winners,
                    &[("amount", B::from(stars_cost(*stars)))],
                )
                .done()
            } else {
                phrase_plural(
                    keys::Q_SERVICE_GIVEAWAY_PREMIUM,
                    winners,
                    &[("months", B::from(trn(keys::MONTHS, i64::from(*months))))],
                )
                .done()
            }
        }
        A::GiveawayCompleted {
            winners,
            stars,
            unclaimed,
        }
        | A::GiveawayWinners {
            winners,
            unclaimed,
            stars,
        } => giveaway_results(*winners, *stars, *unclaimed),
        A::GiveawayPrizeStars {
            boosted_chat_id,
            unclaimed,
        } => {
            if *unclaimed {
                ctx.channel_prize(keys::PRIZE_UNCLAIMED_ABOUT, *boosted_chat_id)
            } else {
                ctx.channel_prize(keys::PRIZE_ABOUT, *boosted_chat_id)
            }
        }
        A::Gift {
            sender,
            receiver,
            stars,
            prepaid_upgrade,
            from_auction,
        } => render_gift(
            ctx,
            *sender,
            *receiver,
            *stars,
            *prepaid_upgrade,
            *from_auction,
        ),
        A::UpgradedGift {
            name,
            sender,
            receiver,
            origin,
            price,
        } => render_upgraded_gift(ctx, name, *sender, *receiver, *origin, price.as_ref()),
        A::WithCard { action, .. } => render_action(action, ctx),
        A::TonWalletTransfer(transfer) => super::wallet::transfer_line(transfer, ctx),
        A::TonConnectRequest(request) => text(&request.service_line()),
        A::RefundedUpgradedGift { .. } => say(keys::ACTION_GIFT_REFUNDED),
        A::GiftOffer { name, price, state } => match state {
            GiftOfferState::Accepted => say(keys::ACTION_GIFT_OFFER_STATE_ACCEPTED),
            GiftOfferState::Rejected => say(keys::ACTION_GIFT_OFFER_STATE_REJECTED),
            GiftOfferState::Pending | GiftOfferState::Unknown => {
                if ctx.actor_is_me() {
                    phrase(
                        keys::ACTION_GIFT_OFFER_YOU,
                        &[("cost", B::from(cost(price))), ("name", B::from(name))],
                    )
                    .done()
                } else {
                    phrase(
                        keys::ACTION_GIFT_OFFER_INCOMING,
                        &[("amount", B::from(cost(price)))],
                    )
                    .done()
                }
            }
        },
        A::GiftOnChainOffer {
            name,
            price,
            state,
            is_buyer,
        } => super::gifts::render_on_chain_offer(name, price, *state, *is_buyer),
        A::BotInvite { inviter_user_id } => {
            // In the chat with the bot the bot itself posts it: "{user}
            // invited you"; a person's copy names them as `{from}`
            // (tdesktop `PrepareBotInviteText`).
            let from_bot = matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == ctx.chat_id);
            if from_bot || ctx.actor.is_none() {
                phrase(
                    keys::ACTION_BOT_INVITED_YOU,
                    &[("user", ctx.user(*inviter_user_id))],
                )
                .done()
            } else {
                phrase(
                    keys::ACTION_BOT_INVITED_USER,
                    &[("from", from()), ("user", ctx.user(*inviter_user_id))],
                )
                .done()
            }
        }
        A::GiftOfferRejected {
            name,
            price,
            expired,
        } => {
            let price = cost(price);
            let name = B::from(name);
            let price = B::from(price);
            match (*expired, ctx.actor_is_me()) {
                (true, true) => phrase(
                    keys::ACTION_GIFT_OFFER_EXPIRED_YOUR,
                    &[("name", name), ("cost", price)],
                )
                .done(),
                (true, false) => phrase(
                    keys::ACTION_GIFT_OFFER_EXPIRED,
                    &[("user", from()), ("name", name), ("cost", price)],
                )
                .done(),
                (false, false) => phrase(
                    keys::ACTION_GIFT_OFFER_DECLINED,
                    &[("user", from()), ("name", name), ("cost", price)],
                )
                .done(),
                (false, true) => phrase(
                    keys::Q_SERVICE_OFFER_DECLINED_ME,
                    &[("name", name), ("cost", price)],
                )
                .done(),
            }
        }
        A::PaidMessagesRefunded { count: _, stars } => {
            if ctx.actor_is_me() {
                phrase_plural(
                    keys::ACTION_PAID_MESSAGE_REFUND_SELF,
                    *stars,
                    &[("name", ctx.chat(ctx.chat_id))],
                )
                .done()
            } else {
                phrase_plural(
                    keys::ACTION_PAID_MESSAGE_REFUND,
                    *stars,
                    &[("from", from())],
                )
                .done()
            }
        }
        A::PaidMessagePriceChanged { stars } => {
            if *stars == 0 {
                say(keys::ACTION_MESSAGE_PRICE_FREE)
            } else {
                phrase_plural(keys::ACTION_MESSAGE_PRICE_PAID, *stars, &[]).done()
            }
        }
        A::DirectMessagePriceChanged { enabled, stars } => {
            if !*enabled {
                say(keys::ACTION_DIRECT_MESSAGES_DISABLED)
            } else if *stars == 0 {
                say(keys::ACTION_DIRECT_MESSAGES_ENABLED)
            } else {
                phrase_plural(keys::ACTION_DIRECT_MESSAGES_PAID, *stars, &[]).done()
            }
        }
        A::ChecklistDone { done, not_done, .. } => {
            let tasks = |n: usize| B::from(trn(keys::ACTION_TODO_TASKS_FALLBACK, n as i64));
            let (key, self_key, parts) = if *done > 0 && *not_done == 0 {
                (
                    keys::ACTION_TODO_MARKED_DONE,
                    keys::ACTION_TODO_MARKED_DONE_SELF,
                    vec![("tasks", tasks(*done))],
                )
            } else if *not_done > 0 && *done == 0 {
                (
                    keys::ACTION_TODO_MARKED_NOT_DONE,
                    keys::ACTION_TODO_MARKED_NOT_DONE_SELF,
                    vec![("tasks", tasks(*not_done))],
                )
            } else {
                (
                    keys::Q_SERVICE_TODO_MARKED_BOTH,
                    keys::Q_SERVICE_TODO_MARKED_BOTH_SELF,
                    vec![("done", tasks(*done)), ("not_done", tasks(*not_done))],
                )
            };
            ctx.by_actor(self_key, key, &parts)
        }
        A::ChecklistAdded { titles } => ctx.by_actor(
            keys::ACTION_TODO_ADDED_SELF,
            keys::ACTION_TODO_ADDED,
            &[("tasks", task_list(titles))],
        ),
        A::PollOptionAdded { text: option } => ctx.by_actor(
            keys::ACTION_POLL_ADDED_ANSWER_SELF,
            keys::ACTION_POLL_ADDED_ANSWER,
            &[("option", B::from(option))],
        ),
        A::PollOptionDeleted { text: option } => ctx.by_actor(
            keys::ACTION_POLL_DELETED_ANSWER_SELF,
            keys::ACTION_POLL_DELETED_ANSWER,
            &[("option", B::from(option))],
        ),
        A::SuggestedPostApprovalFailed => say(keys::ACTION_NOT_ENOUGH_FUNDS),
        A::SuggestedPostApproved => say(keys::SUGGEST_ACTION_AGREEMENT),
        A::SuggestedPostDeclined => say(keys::ACTION_POST_REJECTED),
        A::SuggestedPostPaid { stars, grams: g } => {
            if *stars > 0 {
                phrase_plural(
                    keys::ACTION_SUGGEST_SUCCESS_STARS,
                    *stars,
                    &[("from", from())],
                )
                .done()
            } else {
                let shown = grams(*g);
                phrase_plural_shown(
                    keys::ACTION_SUGGEST_SUCCESS_TON,
                    shown.form,
                    shown.text,
                    &[("from", from())],
                )
                .done()
            }
        }
        A::SuggestedPostRefunded { post_deleted } => say(if *post_deleted {
            keys::ACTION_SUGGEST_REFUND_ADMIN
        } else {
            keys::ACTION_SUGGEST_REFUND_USER
        }),
        A::ContactRegistered => phrase(keys::ACTION_USER_REGISTERED, &[("from", from())]).done(),
        A::UsersShared { users } => {
            let shared: Vec<B> = users
                .iter()
                .map(|(id, name)| {
                    let display = ctx
                        .names
                        .user_name(*id)
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| name.clone());
                    B::default().link(
                        display,
                        ServiceLink::Sender(MessageSender::User { user_id: *id }),
                    )
                })
                .collect();
            phrase(
                keys::Q_SERVICE_USERS_SHARED,
                &[
                    (
                        "users",
                        join_segments(shared, &USERS).unwrap_or_else(|| B::from(someone())),
                    ),
                    ("chat", ctx.chat(ctx.chat_id)),
                ],
            )
            .done()
        }
        A::ChatShared { chat_id, title } => {
            let shared = B::default().link(
                ctx.names
                    .chat_name(*chat_id)
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| title.clone()),
                ServiceLink::Sender(MessageSender::Chat { chat_id: *chat_id }),
            );
            phrase(
                keys::ACTION_SHARED_CHAT_WITH_BOT,
                &[("chat", shared), ("bot", ctx.chat(ctx.chat_id))],
            )
            .done()
        }
        A::BotWriteAccess { reason } => match reason {
            BotAccessReason::ConnectedWebsite { domain } => phrase(
                keys::ACTION_BOT_ALLOWED_FROM_DOMAIN,
                &[(
                    "domain",
                    B::default().link(domain.clone(), ServiceLink::Url(format!("http://{domain}"))),
                )],
            )
            .done(),
            BotAccessReason::AttachMenu => say(keys::ACTION_ATTACH_MENU_BOT_ALLOWED),
            BotAccessReason::WebApp { title } => {
                let app = if title.is_empty() {
                    tr(keys::Q_SERVICE_WEB_APP)
                } else {
                    title.as_str()
                };
                line(keys::ACTION_BOT_ALLOWED_FROM_APP, &[("app", app)])
            }
            BotAccessReason::AcceptedRequest => say(keys::ACTION_WEBAPP_BOT_ALLOWED),
        },
        A::WebAppDataSent { button } => phrase(
            keys::ACTION_WEBVIEW_DATA_DONE,
            &[("text", B::from(button.as_str()))],
        )
        .done(),
        A::WebAppDataReceived { button } => {
            line(keys::Q_SERVICE_WEBAPP_DATA_RECEIVED, &[("text", button)])
        }
        A::PassportSent { kinds } => {
            let names: Vec<&str> = kinds
                .iter()
                .map(|kind| {
                    tr(match kind {
                        PassportKind::PersonalDetails => keys::ACTION_SECURE_PERSONAL_DETAILS,
                        PassportKind::ProofOfIdentity => keys::ACTION_SECURE_PROOF_OF_IDENTITY,
                        PassportKind::Address => keys::ACTION_SECURE_ADDRESS,
                        PassportKind::ProofOfAddress => keys::ACTION_SECURE_PROOF_OF_ADDRESS,
                        PassportKind::Phone => keys::ACTION_SECURE_PHONE,
                        PassportKind::Email => keys::ACTION_SECURE_EMAIL,
                    })
                })
                .collect();
            phrase(
                keys::ACTION_SECURE_VALUES_SENT,
                &[
                    ("user", ctx.chat(ctx.chat_id)),
                    ("documents", B::from(names.join(", "))),
                ],
            )
            .done()
        }
        A::PassportReceived => say(keys::Q_SERVICE_PASSPORT_RECEIVED),
        A::Proximity {
            traveler,
            watcher,
            distance: meters,
        } => {
            let near = B::from(distance(*meters));
            let me = |s: &MessageSender| matches!(s, MessageSender::User { user_id } if ctx.is_me_user(*user_id));
            if me(traveler) {
                phrase(
                    keys::ACTION_YOU_PROXIMITY_REACHED,
                    &[("user", ctx.peer(*watcher)), ("distance", near)],
                )
                .done()
            } else if me(watcher) {
                phrase(
                    keys::ACTION_PROXIMITY_REACHED_YOU,
                    &[("from", ctx.peer(*traveler)), ("distance", near)],
                )
                .done()
            } else {
                phrase(
                    keys::ACTION_PROXIMITY_REACHED,
                    &[
                        ("from", ctx.peer(*traveler)),
                        ("user", ctx.peer(*watcher)),
                        ("distance", near),
                    ],
                )
                .done()
            }
        }
        A::VideoChatScheduled { start_date } => {
            let when = ctx.format_schedule(*start_date);
            if ctx.is_channel {
                line(
                    keys::ACTION_GROUP_CALL_SCHEDULED_CHANNEL,
                    &[("date", &when)],
                )
            } else {
                phrase(
                    keys::ACTION_GROUP_CALL_SCHEDULED_GROUP,
                    &[("from", from()), ("date", B::from(when))],
                )
                .done()
            }
        }
        A::VideoChatStarted => {
            if ctx.is_channel {
                say(keys::ACTION_GROUP_CALL_STARTED_CHANNEL)
            } else {
                phrase(keys::ACTION_GROUP_CALL_STARTED_GROUP, &[("from", from())]).done()
            }
        }
        A::VideoChatEnded { duration } => {
            let length = call_duration(*duration);
            if ctx.is_channel {
                line(keys::ACTION_GROUP_CALL_FINISHED, &[("duration", &length)])
            } else {
                phrase(
                    keys::ACTION_GROUP_CALL_FINISHED_GROUP,
                    &[("from", from()), ("duration", B::from(length))],
                )
                .done()
            }
        }
        A::VideoChatInvite { users } => {
            let invited = ctx.join_users(
                users,
                &ListKeys {
                    and_one: keys::ACTION_INVITE_USERS_AND_ONE,
                    and_last: keys::ACTION_INVITE_USERS_AND_LAST,
                    ..USERS
                },
            );
            let chat = B::from(tr(keys::ACTION_INVITE_USER_CHAT));
            if users.len() > 1 {
                phrase(
                    keys::ACTION_INVITE_USERS_MANY,
                    &[("from", from()), ("users", invited), ("chat", chat)],
                )
                .done()
            } else {
                phrase(
                    keys::ACTION_INVITE_USER,
                    &[("from", from()), ("user", invited), ("chat", chat)],
                )
                .done()
            }
        }
        A::Expired(kind) => say(match kind {
            ExpiredKind::Photo => keys::TTL_PHOTO_EXPIRED,
            ExpiredKind::Video => keys::TTL_VIDEO_EXPIRED,
            ExpiredKind::VideoNote => keys::TTL_ROUND_EXPIRED,
            ExpiredKind::VoiceNote => keys::TTL_VOICE_EXPIRED,
        }),
        A::StakeDice { stake, prize } => {
            let (key, self_key, amount) = if *prize > *stake {
                (
                    keys::ACTION_STAKE_GAME_WON,
                    keys::ACTION_STAKE_GAME_WON_YOU,
                    Some(grams_cost(*prize - *stake)),
                )
            } else if *prize == 0 && *stake > 0 {
                (
                    keys::ACTION_STAKE_GAME_LOST,
                    keys::ACTION_STAKE_GAME_LOST_YOU,
                    Some(grams_cost(*stake)),
                )
            } else {
                (
                    keys::Q_SERVICE_STAKE_NOTHING,
                    keys::Q_SERVICE_STAKE_NOTHING_YOU,
                    None,
                )
            };
            let parts: Vec<_> = amount.map(|a| ("amount", B::from(a))).into_iter().collect();
            ctx.by_actor(self_key, key, &parts)
        }
        // tdesktop `prepareStoryMentionText`: the chat's peer is named.
        A::Story {
            via_mention: true, ..
        } => {
            if ctx.is_outgoing {
                phrase(
                    keys::ACTION_STORY_MENTION_ME,
                    &[("user", ctx.chat(ctx.chat_id))],
                )
                .done()
            } else {
                phrase(
                    keys::ACTION_STORY_MENTION,
                    &[("user", ctx.chat(ctx.chat_id))],
                )
                .done()
            }
        }
        A::Story { .. } => say(keys::IN_DLG_STORY),
        A::PaidMedia { stars, .. } => phrase(
            keys::Q_SERVICE_PAID_MEDIA,
            &[("stars", B::from(stars_cost(*stars)))],
        )
        .done(),
    }
}

/// A list of checklist tasks, quoted and joined the way tdesktop does.
fn task_list(titles: &[String]) -> B {
    let tasks = titles
        .iter()
        .map(|title| B::from(tr_with(keys::ACTION_TODO_TASK_QUOTE, &[("task", title)])))
        .collect();
    join_segments(
        tasks,
        &ListKeys {
            and_one: keys::ACTION_TODO_TASKS_AND_ONE,
            and_last: keys::ACTION_TODO_TASKS_AND_LAST,
            accumulated: "tasks",
            item: "task",
        },
    )
    .unwrap_or_else(|| B::from(trn(keys::ACTION_TODO_TASKS_FALLBACK, 1)))
}
