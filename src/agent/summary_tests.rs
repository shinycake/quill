use super::*;
use crate::agent::grants::ChatGrant;

fn inputs() -> Inputs {
    Inputs {
        enabled: true,
        confirm_sends: false,
        ui_driver: false,
        raw_calls: true,
        takeover: Takeover::Idle,
        screenshots_built: true,
        screen_reader: false,
        grants: AccountGrants::default(),
        window_shows_everything: false,
        connected: Vec::new(),
        refused: Vec::new(),
        // The real table, so the states below are the ones the page shows.
        tools: crate::agent::tools::registry(),
        log: Vec::new(),
    }
}

fn state(summary: &AccessSummary, name: &str) -> Option<ToolState> {
    summary
        .tools
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.state)
}

fn action(chat_id: i64, time: i64) -> Action {
    Action {
        id: 0,
        time,
        account: AccountKey::primary(),
        agent: "Claude Code".into(),
        tool: "send_message".into(),
        chat_id,
        chat_title: "Chat".into(),
        summary: String::new(),
        ok: true,
        declined: false,
        message_ids: Vec::new(),
        undone: false,
    }
}

#[test]
fn access_off_turns_every_tool_off_and_says_only_that() {
    let summary = summarize(&Inputs {
        enabled: false,
        ui_driver: true,
        ..inputs()
    });
    assert!(
        summary
            .tools
            .iter()
            .all(|tool| tool.state == ToolState::Off(OffReason::AccessOff))
    );
    assert_eq!(summary.claims, vec![Claim::AccessOff]);
    assert_eq!(summary.window.state, WindowState::Off);
    assert_eq!(summary.window.view, WindowView::Nothing);
    assert_eq!(OffReason::AccessOff.fix(), Some(Fix::TurnOnAccess));
}

#[test]
fn read_tools_are_available_but_contacts_need_their_grant() {
    let summary = summarize(&inputs());
    assert_eq!(state(&summary, "list_chats"), Some(ToolState::Available));
    assert_eq!(
        state(&summary, "list_contacts"),
        Some(ToolState::Off(OffReason::ContactsOff))
    );
    assert_eq!(
        state(&summary, "request_access"),
        Some(ToolState::AsksFirst(Asks::Always))
    );
    let mut with_contacts = inputs();
    with_contacts.grants.contacts = true;
    assert_eq!(
        state(&summarize(&with_contacts), "list_contacts"),
        Some(ToolState::Available)
    );
    assert_eq!(OffReason::ContactsOff.fix(), Some(Fix::TurnOnContacts));
}

#[test]
fn write_tools_need_a_writable_chat_and_ask_with_confirm_each_send() {
    let mut read_only = inputs();
    read_only.grants.saved = Some(Scope::Read);
    read_only.grants.bots = None;
    let summary = summarize(&read_only);
    assert_eq!(
        state(&summary, "send_message"),
        Some(ToolState::Off(OffReason::NothingWritable))
    );
    assert_eq!(OffReason::NothingWritable.fix(), Some(Fix::GrantWriting));

    let summary = summarize(&inputs());
    assert_eq!(state(&summary, "send_message"), Some(ToolState::Available));

    let confirming = summarize(&Inputs {
        confirm_sends: true,
        ..inputs()
    });
    assert_eq!(
        state(&confirming, "send_message"),
        Some(ToolState::AsksFirst(Asks::EachSend))
    );
    // Marking read is not a send.
    assert_eq!(state(&confirming, "mark_read"), Some(ToolState::Available));
    assert!(
        confirming
            .always_asks
            .iter()
            .any(|ask| ask.kind == AskKind::EachSend && ask.reachable())
    );
    assert!(
        !summary
            .always_asks
            .iter()
            .any(|ask| ask.kind == AskKind::EachSend)
    );
}

#[test]
fn one_writable_added_chat_is_enough_to_write() {
    let mut grants = AccountGrants {
        saved: None,
        bots: None,
        own_channels: None,
        ..AccountGrants::default()
    };
    grants.chats.insert(
        7,
        ChatGrant {
            scope: Some(Scope::ReadWrite),
            title: "Team".into(),
        },
    );
    let summary = summarize(&Inputs { grants, ..inputs() });
    assert_eq!(state(&summary, "send_message"), Some(ToolState::Available));
    assert!(
        summary
            .claims
            .contains(&Claim::ReadWrite(vec![Place::Added(1)]))
    );
}

#[test]
fn raw_calls_ask_for_risky_ones_and_turn_off_with_their_switch() {
    let summary = summarize(&inputs());
    assert_eq!(
        state(&summary, "td_call"),
        Some(ToolState::AsksFirst(Asks::RiskyCalls))
    );
    assert!(summary.always_asks.iter().all(|ask| ask.reachable()));
    let off = summarize(&Inputs {
        raw_calls: false,
        ..inputs()
    });
    assert_eq!(
        state(&off, "td_call"),
        Some(ToolState::Off(OffReason::RawCallsOff))
    );
    assert_eq!(OffReason::RawCallsOff.fix(), Some(Fix::TurnOnRawCalls));
    let unreachable: Vec<(AskKind, Option<OffReason>)> = off
        .always_asks
        .iter()
        .filter(|ask| !ask.reachable())
        .map(|ask| (ask.kind, ask.unreachable))
        .collect();
    let raw_off = Some(OffReason::RawCallsOff);
    assert_eq!(
        unreachable,
        vec![
            (AskKind::BeyondOneChat, raw_off),
            (AskKind::AccountAndSessions, raw_off),
            (AskKind::MoneyStarsGifts, raw_off)
        ]
    );
    assert!(off.claims.contains(&Claim::RawCalls(false)));
}

#[test]
fn window_tools_follow_the_switch_pause_stop_and_capture() {
    let off = summarize(&inputs());
    assert_eq!(
        state(&off, "read_ui_tree"),
        Some(ToolState::Off(OffReason::WindowControlOff))
    );
    assert_eq!(
        OffReason::WindowControlOff.fix(),
        Some(Fix::TurnOnWindowControl)
    );

    let on = Inputs {
        ui_driver: true,
        ..inputs()
    };
    let ready = summarize(&on);
    assert_eq!(state(&ready, "read_ui_tree"), Some(ToolState::Available));
    assert_eq!(state(&ready, "take_screenshot"), Some(ToolState::Available));
    assert_eq!(ready.window.state, WindowState::Ready);
    assert_eq!(ready.window.view, WindowView::GrantedChats);

    let paused = summarize(&Inputs {
        takeover: Takeover::Paused,
        ..on.clone()
    });
    assert_eq!(
        state(&paused, "read_ui_tree"),
        Some(ToolState::Off(OffReason::Paused))
    );
    assert_eq!(OffReason::Paused.fix(), Some(Fix::LetContinue));

    let stopped = summarize(&Inputs {
        takeover: Takeover::Stopped,
        ..on.clone()
    });
    assert_eq!(
        state(&stopped, "click_element").or(state(&stopped, "read_ui_tree")),
        Some(ToolState::Off(OffReason::Stopped))
    );
    assert_eq!(OffReason::Stopped.fix(), Some(Fix::RestartWindowControl));

    let no_capture = summarize(&Inputs {
        screenshots_built: false,
        ..on.clone()
    });
    assert_eq!(
        state(&no_capture, "take_screenshot"),
        Some(ToolState::Off(OffReason::NoCapture))
    );
    assert_eq!(OffReason::NoCapture.fix(), None);

    // A screen reader holds every window tool, even mid-drive; a pause or
    // Stop still says what the user did.
    for takeover in [
        Takeover::Idle,
        Takeover::Driving {
            agent: "Claude Code".into(),
        },
    ] {
        let held = summarize(&Inputs {
            screen_reader: true,
            takeover,
            ..on.clone()
        });
        assert_eq!(held.window.state, WindowState::ScreenReader);
        assert_eq!(
            state(&held, "click_element"),
            Some(ToolState::Off(OffReason::ScreenReader))
        );
        assert_eq!(held.describe()["window_control"]["state"], "screen_reader");
    }
    let paused_too = summarize(&Inputs {
        screen_reader: true,
        takeover: Takeover::Paused,
        ..on.clone()
    });
    assert_eq!(paused_too.window.state, WindowState::Paused);
    assert_eq!(OffReason::ScreenReader.fix(), None);

    let confirming = summarize(&Inputs {
        confirm_sends: true,
        ..on.clone()
    });
    // Send in the message box (a click or Enter) goes through the confirm;
    // typing only fills the box.
    for name in ["click_element", "press_keys"] {
        assert_eq!(
            state(&confirming, name),
            Some(ToolState::AsksFirst(Asks::EachSend)),
            "{name}"
        );
    }
    for name in ["type_text", "read_ui_tree", "scroll_view", "hover_element"] {
        assert_eq!(
            state(&confirming, name),
            Some(ToolState::Available),
            "{name}"
        );
    }

    let driving = summarize(&Inputs {
        takeover: Takeover::Driving {
            agent: "Cursor".into(),
        },
        ..on
    });
    assert_eq!(
        driving.window.state,
        WindowState::Driving {
            agent: "Cursor".into()
        }
    );
}

#[test]
fn all_chats_shows_the_whole_window_and_hides_only_secret_chats() {
    let mut on = Inputs {
        ui_driver: true,
        ..inputs()
    };
    on.grants.all_chats = Some(Scope::Read);
    on.window_shows_everything = true;
    let summary = summarize(&on);
    assert_eq!(summary.window.view, WindowView::WholeWindow);
    assert!(summary.claims.contains(&Claim::SecretChatsHidden));
    assert!(!summary.claims.contains(&Claim::OtherChatsHidden));
    assert!(summary.claims.contains(&Claim::ReadOnly(vec![
        Place::OwnChannels,
        Place::AllOtherChats
    ])));
}

#[test]
fn the_window_view_follows_the_open_account_not_the_one_shown() {
    // Settings shows an account with All chats on while the open account,
    // whose window the driver shows, has it off.
    let mut other = Inputs {
        ui_driver: true,
        ..inputs()
    };
    other.grants.all_chats = Some(Scope::ReadWrite);
    assert_eq!(summarize(&other).window.view, WindowView::GrantedChats);
    // And the other way round.
    let open_all = Inputs {
        ui_driver: true,
        window_shows_everything: true,
        ..inputs()
    };
    assert_eq!(summarize(&open_all).window.view, WindowView::WholeWindow);
}

#[test]
fn claims_follow_the_defaults_and_link_to_their_sections() {
    let summary = summarize(&inputs());
    assert_eq!(
        summary.claims,
        vec![
            Claim::ReadWrite(vec![Place::SavedMessages, Place::Bots]),
            Claim::ReadOnly(vec![Place::OwnChannels]),
            Claim::OtherChatsHidden,
            Claim::Contacts(false),
            Claim::RawCalls(true),
            Claim::Window(WindowState::Off),
            Claim::EachSend(false),
            Claim::AlwaysAsks,
            Claim::Agents(Vec::new()),
        ]
    );
    let sections: Vec<Section> = summary.claims.iter().map(Claim::section).collect();
    assert_eq!(sections[0], Section::Chats);
    assert!(sections.contains(&Section::Window));
    assert!(sections.contains(&Section::Confirmations));
    assert!(sections.contains(&Section::Tools));
    assert!(sections.contains(&Section::AlwaysAsks));
    assert_eq!(sections.last(), Some(&Section::Connection));
}

#[test]
fn nothing_granted_says_so() {
    let summary = summarize(&Inputs {
        grants: AccountGrants {
            saved: None,
            bots: None,
            own_channels: None,
            ..AccountGrants::default()
        },
        ..inputs()
    });
    assert_eq!(summary.claims.first(), Some(&Claim::NoChats));
    assert!(!summary.chats.readable_anywhere());
}

#[test]
fn added_chats_carry_secrecy_and_when_an_agent_last_used_them() {
    let secret = -2_000_000_000_000 + 5;
    let mut grants = AccountGrants::default();
    for (id, title) in [(7, "Team"), (secret, "Dana")] {
        grants.chats.insert(
            id,
            ChatGrant {
                scope: Some(Scope::Read),
                title: title.into(),
            },
        );
    }
    let summary = summarize(&Inputs {
        grants,
        log: vec![action(7, 300), action(9, 250), action(7, 200)],
        ..inputs()
    });
    let team = summary.chats.added.iter().find(|c| c.chat_id == 7);
    assert_eq!(
        team.map(|c| (c.secret, c.last_used)),
        Some((false, Some(300)))
    );
    let dana = summary.chats.added.iter().find(|c| c.chat_id == secret);
    assert_eq!(dana.map(|c| (c.secret, c.last_used)), Some((true, None)));
}

#[test]
fn connected_agents_list_their_tools_and_name_the_claim() {
    let summary = summarize(&Inputs {
        connected: vec![Connected {
            id: 3,
            agent: "Claude Code".into(),
            since: 100,
            tools: vec!["quill_status".into(), "list_chats".into()],
        }],
        refused: vec!["Cursor".into()],
        ..inputs()
    });
    assert_eq!(summary.agents.len(), 1);
    assert_eq!(summary.agents.first().map(|a| a.tools.len()), Some(2));
    assert_eq!(summary.refused, vec!["Cursor".to_string()]);
    assert_eq!(
        summary.claims.last(),
        Some(&Claim::Agents(vec!["Claude Code".into()]))
    );
}

#[test]
fn agents_read_the_same_facts_the_page_shows() {
    let summary = summarize(&Inputs {
        raw_calls: false,
        confirm_sends: true,
        ..inputs()
    });
    let described = summary.describe();
    assert_eq!(described["raw_tdlib_calls"], false);
    assert_eq!(described["confirm_each_send"], true);
    assert_eq!(described["chats"]["saved_messages"], "read_write");
    assert_eq!(described["window_control"]["state"], "off");
    let td = described["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|t| t["name"] == "td_call"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(td["state"], "off");
    assert_eq!(td["reason"], "the user turned raw TDLib calls off");
    let lines = summary.text_lines();
    assert!(lines.iter().any(|l| l == "Raw TDLib calls (td_call): off"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Off now: ") && l.contains("td_call"))
    );
    assert_eq!(
        summarize(&Inputs {
            enabled: false,
            ..inputs()
        })
        .text_lines(),
        vec!["Agent access is off.".to_string()]
    );
}

#[test]
fn the_registry_covers_every_group() {
    let registry = crate::agent::tools::registry();
    for group in ToolGroup::ALL {
        assert!(registry.iter().any(|tool| tool.group == group), "{group:?}");
    }
    assert!(registry.iter().any(|tool| tool.name == "take_screenshot"));
}

#[test]
fn all_chats_raises_bots_and_own_channels_as_the_grants_do() {
    let mut raised = inputs();
    raised.grants.bots = Some(Scope::Read);
    raised.grants.own_channels = None;
    raised.grants.all_chats = Some(Scope::ReadWrite);
    let summary = summarize(&raised);
    // What the page claims is what a call to a bot or an own channel gets.
    for class in [ChatClass::Bot, ChatClass::OwnChannel, ChatClass::Other] {
        assert_eq!(
            raised.grants.scope_for(7, class),
            Some(Scope::ReadWrite),
            "{class:?}"
        );
    }
    assert_eq!(summary.chats.effective.bots, Some(Scope::ReadWrite));
    assert_eq!(summary.chats.effective.own_channels, Some(Scope::ReadWrite));
    // The controls still show what the user set.
    assert_eq!(summary.chats.bots, Some(Scope::Read));
    assert_eq!(
        summary.claims.first(),
        Some(&Claim::ReadWrite(vec![
            Place::SavedMessages,
            Place::Bots,
            Place::OwnChannels,
            Place::AllOtherChats,
        ]))
    );
    assert!(
        !summary
            .claims
            .iter()
            .any(|claim| matches!(claim, Claim::ReadOnly(_)))
    );
    let described = summary.describe();
    assert_eq!(described["chats"]["bots"], "read_write");
    assert_eq!(described["chats"]["own_channels"], "read_write");
    assert_eq!(raised.grants.describe()["bots"], "read_write");
    assert!(summary.text_lines().iter().any(|l| l == "Bots: read_write"));
}

#[test]
fn each_send_says_nothing_is_writable_not_that_raw_calls_are_off() {
    let mut read_only = Inputs {
        confirm_sends: true,
        ..inputs()
    };
    read_only.grants.saved = Some(Scope::Read);
    read_only.grants.bots = None;
    let summary = summarize(&read_only);
    let each_send = summary
        .always_asks
        .iter()
        .find(|ask| ask.kind == AskKind::EachSend)
        .map(|ask| ask.unreachable);
    assert_eq!(each_send, Some(Some(OffReason::NothingWritable)));
    // Raw calls are on, so the raw asks stay reachable.
    assert!(
        summary
            .always_asks
            .iter()
            .filter(|ask| ask.kind != AskKind::EachSend)
            .all(|ask| ask.reachable())
    );
}

#[test]
fn the_never_row_holds_against_td_call() {
    use crate::agent::tools::td_policy::{Class, Shape, classify};
    assert!(NEVER.contains(&Never::SettingsAndLogout));
    for method in ["logOut", "deleteAccount"] {
        assert!(
            matches!(classify(method, Shape::default()), Class::Blocked(_)),
            "{method}"
        );
    }
    // Ending another session still asks; it is in Always asks, not Never.
    assert!(matches!(
        classify("terminateAllOtherSessions", Shape::default()),
        Class::Ask(_)
    ));
    let never = summarize(&inputs()).describe()["never"].to_string();
    assert!(never.contains("delete the account"), "{never}");
}

#[test]
fn all_chats_never_raises_saved_messages() {
    let grants = AccountGrants {
        saved: None,
        bots: None,
        all_chats: Some(Scope::ReadWrite),
        ..AccountGrants::default()
    };
    let summary = summarize(&Inputs {
        grants: grants.clone(),
        ..inputs()
    });
    assert_eq!(grants.scope_for(1, ChatClass::Saved), None);
    assert_eq!(summary.chats.effective.saved, None);
    assert_eq!(summary.describe()["chats"]["saved_messages"], "off");
    assert!(summary.claims.iter().all(|claim| match claim {
        Claim::ReadWrite(places) | Claim::ReadOnly(places) => {
            !places.contains(&Place::SavedMessages)
        }
        _ => true,
    }));
}
