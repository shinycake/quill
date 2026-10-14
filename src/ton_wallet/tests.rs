//! TON wallet, phase 1: parsing, wording, address checks and the
//! read-only guard.
#![allow(clippy::string_slice, reason = "test code slices known-good literals")]
use super::address::{encode_test_address, explorer_url_from_config};
use super::push::push_text;
use super::*;
use crate::service_text::{PinTarget, ServiceCtx, ServiceLink, ServiceNames, render};
use crate::telegram::envelope::{MessageContent, MessageSender, ServiceAction, parse_content};
use crate::text::{LinkTarget, TextEntity, TextEntityKind, styled_runs};
use serde_json::{Value, json};

const ME: i64 = 9;
const DANA: i64 = 1;

struct Names;

impl ServiceNames for Names {
    fn user_name(&self, id: i64) -> Option<String> {
        (id == DANA).then(|| "Dana Cole".to_string())
    }
    fn user_short_name(&self, id: i64) -> Option<String> {
        (id == DANA).then(|| "Dana".to_string())
    }
    fn chat_name(&self, id: i64) -> Option<String> {
        (id == DANA).then(|| "Dana Cole".to_string())
    }
    fn pinned(&self, _: i64) -> PinTarget {
        PinTarget::Loading
    }
    fn game_title(&self, _: i64) -> Option<String> {
        None
    }
}

fn content(value: Value) -> MessageContent {
    parse_content(Some(&value)).0
}

fn transfer_json(amount: i64, comment: &str, encrypted: bool) -> Value {
    json!({
        "@type": "messageTonWalletTransfer",
        "sender_user_id": if amount >= 0 { DANA } else { ME },
        "transaction_id": "tx1",
        "peer_address": test_address(),
        "amount": amount,
        "comment": comment,
        "is_comment_encrypted": encrypted,
    })
}

fn test_address() -> String {
    encode_test_address(0x11, 0x00, [7; 32])
}

fn line(content: &MessageContent, chat_id: i64, outgoing: bool) -> String {
    let ctx = ServiceCtx {
        actor: Some(MessageSender::User {
            user_id: if outgoing { ME } else { DANA },
        }),
        is_outgoing: outgoing,
        my_id: Some(ME),
        is_channel: false,
        is_secret: false,
        chat_id,
        now: 0,
        names: &Names,
    };
    render(content, &ctx).expect("a service message").plain()
}

fn transfer_of(content: &MessageContent) -> &TonWalletTransfer {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::TonWalletTransfer(transfer) => transfer,
            other => panic!("not a transfer: {other:?}"),
        },
        other => panic!("not an action: {other:?}"),
    }
}

#[test]
fn amounts_follow_the_ios_format() {
    assert_eq!(format_gram_amount(0), "0");
    assert_eq!(format_gram_amount(5_000_000_000), "5");
    assert_eq!(format_gram_amount(-5_000_000_000), "5");
    assert_eq!(format_gram_amount(1_234_567_000_000), "1,234.567");
    assert_eq!(format_gram_amount(1_500_000_000), "1.5");
    // Three decimals at most…
    assert_eq!(format_gram_amount(1_234_567_891), "1.234");
    // …unless that would read zero.
    assert_eq!(format_gram_amount(42), "0.000000042");
    assert_eq!(format_gram_amount(i64::MIN), "9,223,372,036.854");
    assert_eq!(format_grams(1_000_000_000), "1 Gram");
    assert_eq!(format_grams(-1_000_000_000), "1 Gram");
    assert_eq!(format_grams(1_100_000_000), "1.1 Grams");
    assert_eq!(format_grams(2_000_000_000), "2 Grams");
    assert_eq!(signed_gram_amount(2_000_000_000, true), "+2 Grams");
    assert_eq!(signed_gram_amount(-2_000_000_000, false), "\u{2212}2 Grams");
}

#[test]
fn addresses_need_a_valid_checksum() {
    let address = test_address();
    assert_eq!(address.len(), 48);
    assert!(is_valid_ton_address(&address));
    // One changed character breaks the CRC.
    let mut broken: Vec<char> = address.chars().collect();
    broken[10] = if broken[10] == 'A' { 'B' } else { 'A' };
    assert!(!is_valid_ton_address(
        &broken.into_iter().collect::<String>()
    ));
    assert!(!is_valid_ton_address(&address[..47]));
    assert!(!is_valid_ton_address("0:1234"));
    assert_eq!(
        group_address("ABCDEFGHIJ"),
        "ABCD EFGH IJ",
        "the card prints groups of four"
    );
}

#[test]
fn explorer_url_uses_the_app_config_prefix() {
    let address = test_address();
    assert_eq!(
        explorer_url(None, &address),
        format!("https://tonviewer.com/{address}")
    );
    assert_eq!(
        explorer_url(Some("https://tonscan.org"), &address),
        format!("https://tonscan.org/{address}")
    );
    // Only web prefixes; anything odd in the address is escaped.
    assert_eq!(
        explorer_url(Some("javascript:alert(1)//"), "a/b c"),
        "https://tonviewer.com/a%2Fb%20c"
    );
    let config = json!({"@type": "jsonValueObject", "members": [
        {"@type": "jsonObjectMember", "key": "boosts_per_sent_gift",
         "value": {"@type": "jsonValueNumber", "value": 3.0}},
        {"@type": "jsonObjectMember", "key": "ton_blockchain_explorer_url",
         "value": {"@type": "jsonValueString", "value": "https://tonscan.org/address/"}},
    ]});
    assert_eq!(
        explorer_url_from_config(&config).as_deref(),
        Some("https://tonscan.org/address/")
    );
    assert_eq!(explorer_url_from_config(&json!({"members": []})), None);
    let state = TonWalletState {
        explorer_url: explorer_url_from_config(&config),
        explorer_url_requested: true,
        ..TonWalletState::default()
    };
    assert_eq!(
        state.explorer_url_for(&address),
        format!("https://tonscan.org/address/{address}")
    );
}

#[test]
fn ton_address_entity_is_a_copyable_link_only_when_valid() {
    let address = test_address();
    let text = format!("send to {address} please");
    let entity = TextEntity {
        utf8_start: 8,
        utf8_end: 8 + address.len(),
        kind: TextEntityKind::TonAddress,
    };
    let runs = styled_runs(&text, std::slice::from_ref(&entity));
    let link = runs
        .iter()
        .find_map(|run| run.link.clone())
        .expect("the address is a link");
    assert_eq!(link, LinkTarget::TonAddress(address.clone()));
    assert_eq!(link.copy_label(), Some("Copy Address"));
    assert_eq!(link.copy_text(), Some(address.as_str()));
    // Not a real address: plain text, as the iOS app leaves it inert.
    let runs = styled_runs(
        "send to EQnotreallyanaddress",
        &[TextEntity {
            utf8_start: 8,
            utf8_end: 28,
            kind: TextEntityKind::TonAddress,
        }],
    );
    assert!(runs.iter().all(|run| run.link.is_none()));
}

#[test]
fn ton_address_entity_and_rich_text_parse() {
    let address = test_address();
    let message = content(json!({
        "@type": "messageText",
        "text": {"@type": "formattedText", "text": address, "entities": [
            {"@type": "textEntity", "offset": 0, "length": 48,
             "type": {"@type": "textEntityTypeTonAddress"}}
        ]},
    }));
    let MessageContent::Text(text) = message else {
        panic!("a text message");
    };
    assert_eq!(text.entities.len(), 1);
    assert_eq!(text.entities[0].kind, TextEntityKind::TonAddress);

    let rich = json!({"@type": "richTexts", "texts": [
        {"@type": "richTextPlain", "text": "Pay "},
        {"@type": "richTextTonAddress", "address": address,
         "text": {"@type": "richTextPlain", "text": address}},
    ]});
    let (plain, entities, _) = crate::rich::rich_text_to_parts(&rich);
    assert_eq!(plain, format!("Pay {address}"));
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].kind, TextEntityKind::TonAddress);
    assert_eq!(
        &plain[entities[0].utf8_start..entities[0].utf8_end],
        address
    );
}

#[test]
fn transfer_parses_and_words_like_the_ios_app() {
    let received = content(transfer_json(5_000_000_000, "for lunch", false));
    let transfer = transfer_of(&received);
    assert!(transfer.is_received());
    assert_eq!(transfer.public_comment(), Some("for lunch"));
    assert_eq!(transfer.peer_address, test_address());
    assert_eq!(line(&received, DANA, false), "Dana sent you 5 Grams");
    assert_eq!(
        line(&received, SERVICE_NOTIFICATIONS_CHAT_ID, false),
        "Someone sent you 5 Grams"
    );
    let sent = content(transfer_json(-1_000_000_000, "", false));
    assert!(!transfer_of(&sent).is_received());
    assert_eq!(line(&sent, DANA, true), "You sent Dana 1 Gram");
    // The chat-list and notification lines of a received transfer.
    assert_eq!(
        transfer.received_preview(DANA, false).as_deref(),
        Some("sent you 5 Grams")
    );
    assert_eq!(
        transfer.received_preview(DANA, true).as_deref(),
        Some("sent you \u{1F48E}5 Grams")
    );
    assert_eq!(
        transfer
            .received_preview(SERVICE_NOTIFICATIONS_CHAT_ID, false)
            .as_deref(),
        Some("Someone sent you 5 Grams")
    );
    assert_eq!(transfer_of(&sent).received_preview(DANA, false), None);
}

#[test]
fn transfer_line_links_the_peer() {
    let received = content(transfer_json(5_000_000_000, "", false));
    let ctx = ServiceCtx {
        actor: Some(MessageSender::User { user_id: DANA }),
        is_outgoing: false,
        my_id: Some(ME),
        is_channel: false,
        is_secret: false,
        chat_id: DANA,
        now: 0,
        names: &Names,
    };
    let text = render(&received, &ctx).expect("service text");
    let (_, links) = text.spans();
    assert_eq!(
        links.first().map(|(_, link)| link.clone()),
        Some(ServiceLink::Sender(MessageSender::User { user_id: DANA }))
    );
}

#[test]
fn encrypted_comment_is_never_shown() {
    let message = content(transfer_json(5_000_000_000, "c2VjcmV0", true));
    let transfer = transfer_of(&message);
    assert_eq!(transfer.public_comment(), None);
    assert!(transfer.has_encrypted_comment());
    let line = line(&message, DANA, false);
    assert!(!line.contains("c2VjcmV0"), "{line}");
}

#[test]
fn ton_connect_request_states() {
    let request = |state: Value, name: &str| {
        content(json!({
            "@type": "messageTonConnectRequest",
            "session_id": "42",
            "state": state,
            "dapp_name": name,
            "topic": "",
            "trace_id": "",
        }))
    };
    let pending = request(
        json!({"@type": "tonConnectRequestStatePending", "expiration_date": 1_900_000_000}),
        "Fragment",
    );
    match &pending {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::TonConnectRequest(parsed) => {
                assert_eq!(parsed.session_id, 42);
                assert_eq!(
                    parsed.state,
                    TonConnectRequestState::Pending {
                        expiration_date: 1_900_000_000
                    }
                );
            }
            other => panic!("not a TON Connect request: {other:?}"),
        },
        other => panic!("not an action: {other:?}"),
    }
    assert_eq!(
        line(&pending, 777_000, false),
        "Wallet request from Fragment"
    );
    assert_eq!(
        line(
            &request(
                json!({"@type": "tonConnectRequestStateAccepted"}),
                "Fragment"
            ),
            777_000,
            false
        ),
        "Wallet request from Fragment accepted"
    );
    assert_eq!(
        line(
            &request(json!({"@type": "tonConnectRequestStateRejected"}), ""),
            777_000,
            false
        ),
        "Wallet request declined"
    );
    // The chat list and notifications use the same line.
    assert_eq!(pending.preview(), "Wallet request from Fragment");
}

#[test]
fn push_contents_word_like_the_ios_pushes() {
    let names = |id: i64| (id == DANA).then(|| "Dana".to_string());
    assert_eq!(
        push_text(
            &json!({"@type": "pushMessageContentTonWalletTransfer",
                    "sender_user_id": DANA, "amount": "5 Grams", "comment": "lunch"}),
            names
        )
        .as_deref(),
        Some("Dana sent you 5 Grams: lunch")
    );
    assert_eq!(
        push_text(
            &json!({"@type": "pushMessageContentTonWalletTransfer",
                    "sender_user_id": 0, "amount": "5 Grams", "comment": ""}),
            names
        )
        .as_deref(),
        Some("You received 5 Grams")
    );
    assert_eq!(
        push_text(
            &json!({"@type": "pushMessageContentTonConnectRequest", "dapp_name": "Fragment"}),
            names
        )
        .as_deref(),
        Some("You have a TON Connect request from Fragment")
    );
    assert_eq!(
        push_text(
            &json!({"@type": "pushMessageContentTonConnectRequest", "dapp_name": ""}),
            names
        )
        .as_deref(),
        Some("You have a TON Connect request")
    );
    assert_eq!(
        push_text(&json!({"@type": "pushMessageContentText"}), names),
        None
    );
}

/// The read-only guard: no request builder or driver code names a method
/// that needs the wallet's keys, moves value or claims a request.
#[test]
fn no_forbidden_wallet_method_is_ever_built() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![
        root.join("telegram"),
        root.join("connect"),
        root.join("ton_wallet"),
        root.join("state/domains/payments"),
    ];
    // The list itself and its tests name the methods on purpose.
    let lists = [
        root.join("ton_wallet/mod.rs"),
        root.join("ton_wallet/tests.rs"),
    ];
    let mut scanned = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") && !lists.contains(&path) {
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                scanned += 1;
                for method in FORBIDDEN_METHODS {
                    assert!(
                        !source.contains(&format!("\"{method}\"")),
                        "{} names the forbidden wallet method {method}",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(scanned > 50, "the scan found the sources ({scanned})");
}
