//! Service text: the wording for every `ServiceAction`, including giveaway results and gifts.
use super::*;

#[allow(clippy::too_many_lines)]
pub fn render_action(action: &ServiceAction, ctx: &ServiceCtx<'_>) -> ServiceText {
    use ServiceAction as A;
    let text = |s: &str| B::default().t(s).done();
    let from = || ctx.from();
    match action {
        A::ChatCreate { title, supergroup } => {
            if *supergroup && ctx.is_channel {
                text("Channel created")
            } else {
                from()
                    .t(format!(" created the group \u{AB}{title}\u{BB}"))
                    .done()
            }
        }
        A::ChatTitle { title } => {
            if ctx.is_channel {
                text(&format!("Channel name was changed to \u{AB}{title}\u{BB}"))
            } else {
                from()
                    .t(format!(" changed group name to \u{AB}{title}\u{BB}"))
                    .done()
            }
        }
        A::ChatPhoto { .. } => {
            if ctx.is_channel {
                text("Channel photo updated")
            } else {
                from().t(" updated group photo").done()
            }
        }
        A::ChatPhotoDeleted => {
            if ctx.is_channel {
                text("Channel photo removed")
            } else {
                from().t(" removed group photo").done()
            }
        }
        A::OwnerLeft { new_owner } => ctx
            .user(*new_owner)
            .t(" will become the new main admin in 7 days if ")
            .extend(from())
            .t(" does not return.")
            .done(),
        A::OwnerChanged { new_owner } => from()
            .t(" made ")
            .extend(ctx.user(*new_owner))
            .t(" the new main admin of the group.")
            .done(),
        A::ProtectedToggled { enabled } => match (ctx.actor_is_me(), enabled) {
            (true, true) => text("You disabled sharing in this chat"),
            (true, false) => text("You enabled sharing in this chat"),
            (false, true) => from().t(" disabled sharing in this chat").done(),
            (false, false) => from().t(" enabled sharing in this chat").done(),
        },
        A::ProtectedDisableRequested { expired } => {
            if *expired {
                text("Sharing enable request has expired")
            } else {
                let features = "\nForwarding messages\nSaving photos and videos\nCopying messages";
                if ctx.actor_is_me() {
                    text(&format!(
                        "You requested to enable sharing in this chat, which includes:{features}"
                    ))
                } else {
                    from()
                        .t(format!(
                            " would like to enable sharing in this chat, which includes:{features}"
                        ))
                        .done()
                }
            }
        }
        A::AddMembers { users } => match users.as_slice() {
            [only] if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *only) => {
                if ctx.is_channel {
                    text("You joined this channel")
                } else {
                    from().t(" joined the group").done()
                }
            }
            [only] if ctx.is_me_user(*only) && !ctx.actor_is_me() => from()
                .t(if ctx.is_channel {
                    " added you to this channel"
                } else {
                    " added you to this group"
                })
                .done(),
            users => from().t(" added ").extend(ctx.users(users)).done(),
        },
        A::DeleteMember { user } => {
            if matches!(ctx.actor, Some(MessageSender::User { user_id }) if user_id == *user) {
                from().t(" left the group").done()
            } else {
                from().t(" removed ").extend(ctx.user(*user)).done()
            }
        }
        A::JoinByLink => from().t(" joined the group via invite link").done(),
        A::JoinByRequest => {
            if ctx.actor_is_me() {
                text(if ctx.is_channel {
                    "Your request to join the channel was approved"
                } else {
                    "Your request to join the group was approved"
                })
            } else {
                from().t(" was accepted to the group").done()
            }
        }
        A::UpgradedTo => text("This group was upgraded to a supergroup"),
        A::UpgradedFrom { title } => text(&format!(
            "The group \u{AB}{title}\u{BB} was upgraded to a supergroup"
        )),
        A::Pin { message_id } => {
            let (excerpt, is_media) = pin_text(ctx.names.pinned(*message_id));
            let link = ServiceLink::Message(*message_id);
            if is_media {
                from().t(" pinned ").link(excerpt, link).done()
            } else {
                from().t(" pinned \"").link(excerpt, link).t("\"").done()
            }
        }
        A::Background { only_for_self } => {
            if ctx.actor_is_me() {
                if *only_for_self {
                    text("You set a new wallpaper for this chat")
                } else {
                    B::default()
                        .t("You set a new wallpaper for ")
                        .extend(ctx.chat(ctx.chat_id))
                        .t(" and you.")
                        .done()
                }
            } else {
                from().t(" set a new wallpaper for this chat").done()
            }
        }
        A::Theme { emoji, gift_name } => match (gift_name, emoji.is_empty()) {
            (Some(name), _) => {
                if ctx.actor_is_me() {
                    text(&format!("You set {name} as a new theme for this chat."))
                } else {
                    from()
                        .t(format!(" set {name} as a new theme for this chat."))
                        .done()
                }
            }
            (None, false) => {
                if ctx.actor_is_me() {
                    text(&format!("You changed the chat theme to {emoji}"))
                } else {
                    from()
                        .t(format!(" changed the chat theme to {emoji}"))
                        .done()
                }
            }
            (None, true) => {
                if ctx.actor_is_me() {
                    text("You disabled the chat theme")
                } else {
                    from().t(" disabled the chat theme").done()
                }
            }
        },
        A::Boost { count } => {
            if ctx.actor_is_me() {
                text("You boosted the group")
            } else if *count > 1 {
                from().t(format!(" boosted the group {count} times")).done()
            } else {
                from().t(" boosted the group").done()
            }
        }
        A::TopicCreated { name } => text(&format!("The topic {} was created", quote(name))),
        A::TopicEdited {
            name, icon_edited, ..
        } => {
            if name.is_empty() && *icon_edited {
                from().t(" changed the topic icon").done()
            } else if name.is_empty() {
                text("Topic edited")
            } else {
                from()
                    .t(format!(" renamed the topic to {}", quote(name)))
                    .done()
            }
        }
        A::TopicClosed { closed } => {
            if *closed {
                from().t(" closed the topic").done()
            } else {
                from().t(" reopened the topic").done()
            }
        }
        A::TopicHidden { hidden } => text(if *hidden {
            "Topic hidden"
        } else {
            "Topic unhidden"
        }),
        A::SuggestProfilePhoto { .. } => {
            if ctx.actor_is_me() {
                B::default()
                    .t("You suggested this photo for ")
                    .extend(ctx.chat(ctx.chat_id))
                    .t("'s Telegram profile.")
                    .done()
            } else {
                from()
                    .t(" suggests this photo for your Telegram profile.")
                    .done()
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
            if ctx.actor_is_me() {
                B::default()
                    .t("You suggest ")
                    .extend(ctx.chat(ctx.chat_id))
                    .t(format!(" add a date of birth: {date}"))
                    .done()
            } else {
                from()
                    .t(format!(" suggests you add your date of birth: {date}"))
                    .done()
            }
        }
        A::Custom { text: custom } => text(custom),
        A::GameScore {
            game_message_id,
            score,
        } => {
            let scored = if ctx.actor_is_me() {
                B::default().t(format!("You scored {score}"))
            } else {
                from().t(format!(" scored {score}"))
            };
            match ctx.names.game_title(*game_message_id) {
                Some(title) => scored
                    .t(" in ")
                    .link(title, ServiceLink::Message(*game_message_id))
                    .done(),
                None => scored.done(),
            }
        }
        A::ManagedBotCreated { bot_user_id } => from()
            .t(" created a bot ")
            .extend(ctx.user(*bot_user_id))
            .t(".")
            .done(),
        A::PaymentRefunded { owner, money } => {
            let who = match owner {
                Some(owner) => ctx.peer(*owner),
                None => B::default().t(WHO),
            };
            who.t(format!(" refunded {}", amount(money))).done()
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
                    ctx.channel_prize(
                        "You have an unclaimed prize from a giveaway by ",
                        *chat_id,
                        ".",
                    )
                } else if *from_giveaway {
                    ctx.channel_prize("You won a prize in a giveaway organized by ", *chat_id, ".")
                } else {
                    ctx.channel_prize("You've received a gift from ", *chat_id, ".")
                }
            }
            Some(MessageSender::User { user_id }) => {
                ctx.gift_line_between(*user_id, &amount(money))
            }
            None => ctx.gift_line(&amount(money)),
        },
        A::GiveawayCreated { stars } => {
            let target = if ctx.is_channel {
                "followers"
            } else {
                "members"
            };
            if *stars > 0 {
                from()
                    .t(format!(
                        " just started a giveaway of {} to its {target}.",
                        stars_cost(*stars)
                    ))
                    .done()
            } else {
                from()
                    .t(format!(
                        " just started a giveaway of Telegram Premium subscriptions to its {target}."
                    ))
                    .done()
            }
        }
        A::Giveaway {
            winners,
            stars,
            months,
        } => {
            if *stars > 0 {
                text(&format!(
                    "Giveaway: {} for {}",
                    stars_cost(*stars),
                    plural(i64::from(*winners), "winner", "winners")
                ))
            } else {
                text(&format!(
                    "Giveaway: {winners} Telegram Premium {} for {}",
                    if *winners == 1 {
                        "subscription"
                    } else {
                        "subscriptions"
                    },
                    plural(i64::from(*months), "month", "months")
                ))
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
                ctx.channel_prize(
                    "You have an unclaimed prize from a giveaway by ",
                    *boosted_chat_id,
                    ".",
                )
            } else {
                ctx.channel_prize(
                    "You won a prize in a giveaway organized by ",
                    *boosted_chat_id,
                    ".",
                )
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
        A::RefundedUpgradedGift => text(
            "This gift was downgraded because a request to refund the payment related to this gift was made, and the money was returned.",
        ),
        A::GiftOffer { name, price, state } => match state {
            1 => text("This offer was accepted."),
            2 => text("This offer was rejected."),
            _ => {
                if ctx.actor_is_me() {
                    text(&format!("You offered {} for {name}.", cost(price)))
                } else {
                    text(&format!("An offer to buy this gift for {}.", cost(price)))
                }
            }
        },
        A::GiftOfferRejected {
            name,
            price,
            expired,
        } => {
            let price = cost(price);
            match (*expired, ctx.actor_is_me()) {
                (true, true) => text(&format!(
                    "Your offer to buy {name} for {price} has expired."
                )),
                (true, false) => from()
                    .t(format!(
                        "'s offer to buy your {name} for {price} has expired."
                    ))
                    .done(),
                (false, false) => from()
                    .t(format!(" rejected your offer to buy {name} for {price}."))
                    .done(),
                (false, true) => text(&format!(
                    "You rejected the offer to buy your {name} for {price}."
                )),
            }
        }
        A::PaidMessagesRefunded { count: _, stars } => {
            if ctx.actor_is_me() {
                B::default()
                    .t(format!("You refunded {} to ", stars_cost(*stars)))
                    .extend(ctx.chat(ctx.chat_id))
                    .done()
            } else {
                from()
                    .t(format!(" refunded {} to you", stars_cost(*stars)))
                    .done()
            }
        }
        A::PaidMessagePriceChanged { stars } => {
            if *stars == 0 {
                text("Messages are now free in this group.")
            } else {
                text(&format!(
                    "Messages now cost {} each in this group.",
                    stars_cost(*stars)
                ))
            }
        }
        A::DirectMessagePriceChanged { enabled, stars } => {
            if !*enabled {
                text("Channel disabled Direct Messages.")
            } else if *stars == 0 {
                text("Channel enabled Direct Messages.")
            } else {
                text(&format!(
                    "Channel allows Direct Messages for {} each.",
                    stars_cost(*stars)
                ))
            }
        }
        A::ChecklistDone { done, not_done, .. } => {
            let tasks = |n: usize| {
                if n == 1 {
                    "task".to_string()
                } else {
                    format!("{n} tasks")
                }
            };
            let who = |tail: String| {
                if ctx.actor_is_me() {
                    B::default().t(format!("You {tail}"))
                } else {
                    from().t(format!(" {tail}"))
                }
            };
            if *done > 0 && *not_done == 0 {
                who(format!("marked {} as done.", tasks(*done))).done()
            } else if *not_done > 0 && *done == 0 {
                who(format!("marked {} as not done.", tasks(*not_done))).done()
            } else {
                who(format!(
                    "marked {} as done and {} as not done.",
                    tasks(*done),
                    tasks(*not_done)
                ))
                .done()
            }
        }
        A::ChecklistAdded { titles } => {
            let list = join_and(titles);
            if ctx.actor_is_me() {
                text(&format!("You added {list} to the list."))
            } else {
                from().t(format!(" added {list} to the list.")).done()
            }
        }
        A::PollOptionAdded { text: option } => {
            if ctx.actor_is_me() {
                text(&format!("You added {} to the poll.", quote(option)))
            } else {
                from()
                    .t(format!(" added {} to the poll.", quote(option)))
                    .done()
            }
        }
        A::PollOptionDeleted { text: option } => {
            if ctx.actor_is_me() {
                text(&format!("You removed {} from the poll.", quote(option)))
            } else {
                from()
                    .t(format!(" removed {} from the poll.", quote(option)))
                    .done()
            }
        }
        A::SuggestedPostApprovalFailed => text("Transaction failed."),
        A::SuggestedPostApproved => text("Agreement reached!"),
        A::SuggestedPostDeclined => text("The post was rejected."),
        A::SuggestedPostPaid { stars, grams: g } => {
            let received = if *stars > 0 {
                stars_cost(*stars)
            } else {
                grams_cost(*g)
            };
            from()
                .t(format!(" has received {received} for publishing post."))
                .done()
        }
        A::SuggestedPostRefunded { post_deleted } => text(if *post_deleted {
            "Admin deleted the post early so that the price was refunded to the user."
        } else {
            "User refunded the Stars so that post was deleted."
        }),
        A::ContactRegistered => from().t(" joined Telegram").done(),
        A::UsersShared { users } => {
            let mut shared = B::default();
            for (index, (id, name)) in users.iter().enumerate() {
                if index > 0 {
                    shared = shared.t(if index + 1 == users.len() {
                        " and "
                    } else {
                        ", "
                    });
                }
                let display = ctx
                    .names
                    .user_name(*id)
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| name.clone());
                shared = shared.link(
                    display,
                    ServiceLink::Sender(MessageSender::User { user_id: *id }),
                );
            }
            B::default()
                .t("You shared ")
                .extend(shared)
                .t(" with ")
                .extend(ctx.chat(ctx.chat_id))
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
            B::default()
                .t("You shared ")
                .extend(shared)
                .t(" with ")
                .extend(ctx.chat(ctx.chat_id))
                .done()
        }
        A::BotWriteAccess { reason } => match reason {
            BotAccessReason::ConnectedWebsite { domain } => B::default()
                .t("You allowed this bot to message you when you logged in on ")
                .link(domain.clone(), ServiceLink::Url(format!("http://{domain}")))
                .t(".")
                .done(),
            BotAccessReason::AttachMenu => text(
                "You allowed this bot to message you when you added it to your attachment menu.",
            ),
            BotAccessReason::WebApp { title } => {
                let app = if title.is_empty() {
                    "App"
                } else {
                    title.as_str()
                };
                text(&format!(
                    "You allowed this bot to message you when you opened {app}."
                ))
            }
            BotAccessReason::AcceptedRequest => {
                text("You allowed this bot to message you in its web-app.")
            }
        },
        A::WebAppDataSent { button } => text(&format!(
            "Data from the {} button was transferred to the bot.",
            quote(button)
        )),
        A::WebAppDataReceived { button } => text(&format!(
            "Data from the {} button was received.",
            quote(button)
        )),
        A::PassportSent { kinds } => {
            let mut names: Vec<&str> = Vec::new();
            for kind in kinds {
                let name = match kind {
                    PassportKind::PersonalDetails => "personal details",
                    PassportKind::ProofOfIdentity => "proof of identity",
                    PassportKind::Address => "address",
                    PassportKind::ProofOfAddress => "proof of address",
                    PassportKind::Phone => "phone number",
                    PassportKind::Email => "email address",
                };
                names.push(name);
            }
            ctx.chat(ctx.chat_id)
                .t(format!(
                    " received the following documents: {}",
                    names.join(", ")
                ))
                .done()
        }
        A::PassportReceived => text("Telegram Passport data received"),
        A::Proximity {
            traveler,
            watcher,
            distance: meters,
        } => {
            let near = distance(*meters);
            let me = |s: &MessageSender| matches!(s, MessageSender::User { user_id } if ctx.is_me_user(*user_id));
            if me(traveler) {
                B::default()
                    .t(format!("You are now within {near} from "))
                    .extend(ctx.peer(*watcher))
                    .done()
            } else if me(watcher) {
                ctx.peer(*traveler)
                    .t(format!(" is now within {near} from you"))
                    .done()
            } else {
                ctx.peer(*traveler)
                    .t(format!(" is now within {near} from "))
                    .extend(ctx.peer(*watcher))
                    .done()
            }
        }
        A::VideoChatScheduled { start_date } => {
            let when = ctx.format_schedule(*start_date);
            if ctx.is_channel {
                text(&format!("Live stream scheduled for {when}"))
            } else {
                from()
                    .t(format!(" scheduled a video chat for {when}"))
                    .done()
            }
        }
        A::VideoChatStarted => {
            if ctx.is_channel {
                text("Live stream started")
            } else {
                from().t(" started a video chat").done()
            }
        }
        A::VideoChatEnded { duration } => {
            let length = call_duration(*duration);
            if ctx.is_channel {
                text(&format!("Live stream finished ({length})"))
            } else {
                from().t(format!(" ended the video chat ({length})")).done()
            }
        }
        A::VideoChatInvite { users } => from()
            .t(" invited ")
            .extend(ctx.users(users))
            .t(" to the video chat")
            .done(),
        A::Expired(kind) => text(match kind {
            ExpiredKind::Photo => "Expired photo",
            ExpiredKind::Video => "Expired video",
            ExpiredKind::VideoNote => "Round message expired",
            ExpiredKind::VoiceNote => "Voice message expired",
        }),
        A::StakeDice { stake, prize } => {
            let amount = grams_cost;
            let (verb_me, verb_other) = if *prize > *stake {
                (
                    format!("You won {}", amount(*prize - *stake)),
                    format!(" won {}", amount(*prize - *stake)),
                )
            } else if *prize == 0 && *stake > 0 {
                (
                    format!("You lost {}", amount(*stake)),
                    format!(" lost {}", amount(*stake)),
                )
            } else {
                (
                    "You didn't win anything".to_string(),
                    " didn't win anything".to_string(),
                )
            };
            if ctx.actor_is_me() {
                text(&verb_me)
            } else {
                from().t(verb_other).done()
            }
        }
        A::Story { via_mention } => text(if *via_mention {
            "Mentioned in a story"
        } else {
            "Story"
        }),
        A::PaidMedia { stars, .. } => text(&format!("Paid media \u{B7} {}", stars_cost(*stars))),
    }
}

fn join_and(items: &[String]) -> String {
    match items {
        [] => "task".into(),
        [only] => only.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

fn giveaway_results(winners: i32, stars: bool, unclaimed: i32) -> ServiceText {
    let text = if winners == 0 {
        "No winners of the giveaway could be selected.".to_string()
    } else {
        match (stars, unclaimed > 0) {
            (true, true) => "Some winners of the giveaway were randomly selected by Telegram and received their prize.".into(),
            (false, true) => "Some winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes.".into(),
            (true, false) => {
                if winners == 1 {
                    "1 winner of the giveaway was randomly selected by Telegram and received their prize.".into()
                } else {
                    format!("{winners} winners of the giveaway were randomly selected by Telegram and received their prize.")
                }
            }
            (false, false) => {
                if winners == 1 {
                    "1 winner of the giveaway was randomly selected by Telegram and received private messages with giftcodes.".into()
                } else {
                    format!("{winners} winners of the giveaway were randomly selected by Telegram and received private messages with giftcodes.")
                }
            }
        }
    };
    B::default().t(text).done()
}

fn render_gift(
    ctx: &ServiceCtx<'_>,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    stars: i64,
    prepaid_upgrade: bool,
    from_auction: bool,
) -> ServiceText {
    let cost = stars_cost(stars);
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let sent_by_me = me(&sender) || (sender.is_none() && ctx.actor_is_me());
    if stars == 0 {
        return if sent_by_me {
            B::default().t("You sent a unique collectible item").done()
        } else {
            ctx.peer_of_gift()
                .t(" sent you a unique collectible item")
                .done()
        };
    }
    if prepaid_upgrade {
        return if sent_by_me {
            B::default()
                .t(format!("You sent an upgrade worth {cost} for "))
                .extend(match receiver {
                    Some(r) if !me(&receiver) => ctx.peer(r),
                    _ => B::default().t("this gift"),
                })
                .t(if me(&receiver) { "." } else { "'s gift." })
                .done()
        } else {
            ctx.peer_of_gift()
                .t(format!(" sent an upgrade worth {cost} for your gift."))
                .done()
        };
    }
    if sender.is_none() && !ctx.actor_is_me() {
        return B::default()
            .t(format!("Unknown user sent you a gift for {cost}"))
            .done();
    }
    if from_auction {
        return B::default()
            .t(format!(
                "You've successfully bought a gift in the auction for {cost}."
            ))
            .done();
    }
    if sent_by_me && me(&receiver) {
        return B::default()
            .t(format!("You bought a gift for {cost}"))
            .done();
    }
    ctx.gift_line(&cost)
}

fn render_upgraded_gift(
    ctx: &ServiceCtx<'_>,
    name: &str,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    origin: GiftOrigin,
    price: Option<&Money>,
) -> ServiceText {
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let actor_me = ctx.actor_is_me();
    match origin {
        GiftOrigin::Upgrade | GiftOrigin::PrepaidUpgrade => {
            if actor_me && (me(&sender) || sender.is_none()) {
                B::default()
                    .t("You turned this gift into a unique collectible")
                    .done()
            } else if actor_me {
                B::default()
                    .t("You turned the gift from ")
                    .extend(match sender {
                        Some(s) => ctx.peer(s),
                        None => B::default().t(WHO),
                    })
                    .t(" into a unique collectible")
                    .done()
            } else {
                ctx.from()
                    .t(" turned the gift from you into a unique collectible")
                    .done()
            }
        }
        GiftOrigin::Transfer | GiftOrigin::Blockchain => {
            if actor_me {
                match receiver {
                    Some(r) if !me(&receiver) => B::default()
                        .t("You transferred a gift to ")
                        .extend(ctx.peer(r))
                        .done(),
                    _ => B::default()
                        .t("You transferred a unique collectible")
                        .done(),
                }
            } else if ctx.actor.is_some() {
                ctx.from().t(" transferred you a gift").done()
            } else {
                B::default().t("Someone transferred you a gift").done()
            }
        }
        GiftOrigin::Resale => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me || me(&sender) {
                B::default().t(format!("You sold a gift for {cost}")).done()
            } else {
                ctx.from().t(format!(" sold you a gift for {cost}")).done()
            }
        }
        GiftOrigin::Offer => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me {
                B::default()
                    .t(format!("You sold {name} for {cost}."))
                    .done()
            } else {
                ctx.from().t(format!(" sold {name} for {cost}.")).done()
            }
        }
        GiftOrigin::Craft => B::default().t("You crafted a new gift").done(),
    }
}
