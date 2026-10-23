use super::*;
use crate::telegram::envelope::StarSubscriptionPricing;

const NOW: i64 = 1_800_000_000;

fn channel() -> ChatFacts {
    ChatFacts {
        group: false,
        public: false,
        global_request_approval: false,
    }
}

fn group() -> ChatFacts {
    ChatFacts {
        group: true,
        ..ChatFacts::default()
    }
}

fn link(expiration_date: i32, member_limit: i32, stars: Option<i64>) -> ParsedChatInviteLink {
    ParsedChatInviteLink {
        invite_link: "https://t.me/+abc".into(),
        name: "Team".into(),
        creator_user_id: 1,
        date: 0,
        edit_date: 0,
        expiration_date,
        subscription_pricing: stars.map(|star_count| StarSubscriptionPricing {
            period: 2_592_000,
            star_count,
        }),
        member_limit,
        member_count: 0,
        expired_member_count: 0,
        pending_join_request_count: 0,
        creates_join_request: false,
        is_primary: false,
        is_revoked: false,
    }
}

#[test]
fn the_default_presets_are_tdesktops() {
    use ExpireOption::{Custom, Value};
    assert_eq!(
        expire_options(Expire::Never, NOW),
        vec![
            Value(Expire::Never),
            Value(Expire::After(HOUR)),
            Value(Expire::After(DAY)),
            Value(Expire::After(7 * DAY)),
            Custom,
        ]
    );
    assert_eq!(
        usage_options(Usage::Any),
        vec![
            UsageOption::Value(Usage::Any),
            UsageOption::Value(Usage::Limit(1)),
            UsageOption::Value(Usage::Limit(10)),
            UsageOption::Value(Usage::Limit(100)),
            UsageOption::Custom,
        ]
    );
}

#[test]
fn a_custom_date_sits_among_the_presets() {
    let in_three_hours = Expire::At((NOW + 3 * 3600) as i32);
    let options = expire_options(in_three_hours, NOW);
    assert_eq!(options.get(2), Some(&ExpireOption::Value(in_three_hours)));
    let in_a_month = Expire::At((NOW + 30 * 86_400) as i32);
    let options = expire_options(in_a_month, NOW);
    assert_eq!(options.get(4), Some(&ExpireOption::Value(in_a_month)));
    assert_eq!(options.last(), Some(&ExpireOption::Custom));
    assert_eq!(options.len(), 6);
}

#[test]
fn a_custom_limit_sits_in_order() {
    let options = usage_options(Usage::Limit(25));
    assert_eq!(options.get(3), Some(&UsageOption::Value(Usage::Limit(25))));
    let options = usage_options(Usage::Limit(500));
    assert_eq!(options.get(4), Some(&UsageOption::Value(Usage::Limit(500))));
    assert_eq!(usage_options(Usage::Limit(10)).len(), 5);
}

#[test]
fn presets_map_to_create_requests() {
    let mut form = InviteLinkForm::new_link(group());
    form.expire = Expire::After(DAY);
    form.usage = Usage::Limit(10);
    assert_eq!(
        form.request("  Friends ", 0, NOW),
        InviteLinkRequest::Create {
            name: "Friends".into(),
            expiration_date: (NOW + 86_400) as i32,
            member_limit: 10,
            creates_join_request: false,
        }
    );
    form.expire = Expire::Never;
    form.usage = Usage::Any;
    assert_eq!(
        form.request("", 0, NOW),
        InviteLinkRequest::Create {
            name: String::new(),
            expiration_date: 0,
            member_limit: 0,
            creates_join_request: false,
        }
    );
}

#[test]
fn the_name_is_cut_to_32_characters() {
    let form = InviteLinkForm::new_link(group());
    let long = "x".repeat(40);
    match form.request(&long, 0, NOW) {
        InviteLinkRequest::Create { name, .. } => assert_eq!(name.chars().count(), NAME_MAX),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn approval_drops_the_member_limit_and_the_fee() {
    let mut form = InviteLinkForm::new_link(channel());
    form.usage = Usage::Limit(100);
    assert_eq!(form.toggle_subscription(), SubscriptionToggle::Toggled);
    assert!(form.subscription);
    form.toggle_approval();
    assert!(form.request_approval);
    assert!(!form.subscription);
    assert!(!form.shows_usage());
    assert_eq!(
        form.request("", 0, NOW),
        InviteLinkRequest::Create {
            name: String::new(),
            expiration_date: 0,
            member_limit: 0,
            creates_join_request: true,
        }
    );
    form.toggle_subscription();
    assert!(!form.request_approval);
    assert!(!form.shows_limits());
}

#[test]
fn a_fee_makes_a_subscription_link() {
    let mut form = InviteLinkForm::new_link(channel());
    form.toggle_subscription();
    assert_eq!(
        form.request("VIP", 250, NOW),
        InviteLinkRequest::CreateSubscription {
            name: "VIP".into(),
            star_count: 250,
        }
    );
    assert!(matches!(
        form.request("VIP", 0, NOW),
        InviteLinkRequest::Create {
            creates_join_request: false,
            member_limit: 0,
            expiration_date: 0,
            ..
        }
    ));
}

#[test]
fn only_private_channels_offer_a_fee() {
    assert!(InviteLinkForm::new_link(channel()).offers_subscription());
    assert!(!InviteLinkForm::new_link(group()).offers_subscription());
    let public = ChatFacts {
        public: true,
        ..channel()
    };
    assert!(!InviteLinkForm::new_link(public).offers_subscription());
}

#[test]
fn a_public_chat_locks_approval_off() {
    let public = ChatFacts {
        group: true,
        public: true,
        global_request_approval: false,
    };
    let mut form = InviteLinkForm::new_link(public);
    assert!(form.approval_locked());
    form.toggle_approval();
    assert!(!form.request_approval);
    let approving = ChatFacts {
        global_request_approval: true,
        ..public
    };
    let mut form = InviteLinkForm::new_link(approving);
    form.toggle_approval();
    assert!(form.request_approval);
}

#[test]
fn editing_prefills_and_sends_edit() {
    let date = (NOW + 5 * 86_400) as i32;
    let form = InviteLinkForm::edit_link(group(), &link(date, 25, None));
    assert_eq!(form.expire, Expire::At(date));
    assert_eq!(form.usage, Usage::Limit(25));
    assert_eq!(
        form.request("Team", 0, NOW),
        InviteLinkRequest::Edit {
            link: "https://t.me/+abc".into(),
            name: "Team".into(),
            expiration_date: date,
            member_limit: 25,
            creates_join_request: false,
        }
    );
}

#[test]
fn a_subscription_link_only_renames() {
    let mut form = InviteLinkForm::edit_link(channel(), &link(0, 0, Some(100)));
    assert_eq!(form.locked_stars, Some(100));
    assert!(!form.shows_limits());
    assert!(!form.shows_approval());
    assert_eq!(form.toggle_subscription(), SubscriptionToggle::Locked);
    form.toggle_approval();
    assert!(!form.request_approval);
    assert_eq!(
        form.request("Gold", 999, NOW),
        InviteLinkRequest::EditSubscription {
            link: "https://t.me/+abc".into(),
            name: "Gold".into(),
        }
    );
}

#[test]
fn custom_values_are_checked_like_tdesktop() {
    assert!(!custom_date_ok(NOW, NOW));
    assert!(custom_date_ok(NOW + 3600, NOW));
    assert!(!custom_date_ok(NOW + 400 * 86_400, NOW));
    assert_eq!(custom_limit(" 42 "), Some(42));
    assert_eq!(custom_limit("0"), None);
    assert_eq!(custom_limit("abc"), None);
    assert_eq!(custom_limit("100000"), None);
    assert_eq!(stars_amount("3000", 2500), 2500);
    assert_eq!(stars_amount("", 2500), 0);
}

#[test]
fn the_dollar_rate_comes_from_the_app_config() {
    let config = serde_json::json!({
        "@type": "jsonValueObject",
        "members": [{
            "@type": "jsonObjectMember",
            "key": "stars_usd_withdraw_rate_x1000",
            "value": {"@type": "jsonValueNumber", "value": 1300.0}
        }]
    });
    assert_eq!(stars_usd_rate_from_config(&config), Some(1.3));
    assert_eq!(stars_usd_rate_from_config(&serde_json::json!({})), None);
    assert_eq!(stars_usd(100, 1.3).as_deref(), Some("$1.30"));
    assert_eq!(stars_usd(2500, 1.3).as_deref(), Some("$32.50"));
    assert_eq!(stars_usd(0, 1.3), None);
}
