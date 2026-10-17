use crate::composer::commands::{
    CommandBot, CommandMenuItem, command_menu_trigger, commands_with_username, complete_command_at,
    complete_mention, filter_command_menu_items, group_command_menu_items, inline_query_trigger,
    insert_bot_command_text, insert_switch_inline_text, mention_trigger, merge_command_menu_items,
};
use crate::telegram::envelope::BotCommand;

fn mention(text: &str) -> Option<(std::ops::Range<usize>, String)> {
    mention_trigger(text, text.len()).map(|t| (t.range, t.query.to_string()))
}

#[test]
fn mention_trigger_needs_a_word_start_and_name_chars() {
    assert_eq!(mention("hi @al"), Some((3..6, "al".into())));
    assert_eq!(mention("@"), Some((0..1, String::new())));
    assert_eq!(mention("mail@host"), None);
    assert_eq!(mention("hi @al "), None);
    assert_eq!(mention("hi @al-x"), None);
    assert_eq!(mention("no at"), None);
    assert_eq!(mention("(@al"), Some((1..4, "al".into())));
    // A leading `_` is not a mention (tdesktop wants a letter or digit).
    assert_eq!(mention("@_a"), None);
    assert_eq!(mention("a_b @a_b"), Some((4..8, "a_b".into())));
    // Up to 63 characters are scanned.
    assert!(mention(&format!("@{}", "a".repeat(63))).is_some());
    assert_eq!(mention(&format!("@{}", "a".repeat(64))), None);
}

#[test]
fn triggers_work_after_multi_byte_text() {
    let text = "日本 @ab";
    assert_eq!(mention(text), Some((7..10, "ab".into())));
    let token = mention_trigger(text, text.len() - 1).map(|t| (t.range, t.query));
    assert_eq!(token, Some((7..9, "a")));
    // Inside a CJK character: not a boundary.
    assert_eq!(mention_trigger(text, 1), None);
    assert_eq!(mention("é @a"), Some((3..5, "a".into())));
    assert_eq!(mention("👨\u{200d}👩 @x"), Some((12..14, "x".into())));
    assert_eq!(mention("日@ab"), None);
    assert_eq!(command("/a日"), Some((0..5, "a日".into())));
    assert_eq!(command("/日本@bot"), Some((0..11, "日本@bot".into())));
    let done = complete_mention("日本 @ab 終", 10, 7, "abe", "Abe");
    assert_eq!(done, Some(("日本 @abe  終".into(), 12)));
    assert_eq!(
        complete_command_at("/日 本", 0..4, "/start"),
        Some(("/start".into(), 7))
    );
}

#[test]
fn mention_trigger_follows_the_caret_not_the_end() {
    let text = "hi @al and more";
    // Caret right after "@al": the mention is found mid-text.
    let token = mention_trigger(text, 6).map(|t| (t.range, t.query));
    assert_eq!(token, Some((3..6, "al")));
    // Caret after the space that follows: no mention.
    assert_eq!(mention_trigger(text, 7), None);
    // Caret inside the name: only the text before it is the query.
    let token = mention_trigger(text, 5).map(|t| (t.range, t.query));
    assert_eq!(token, Some((3..5, "a")));
    // Not a char boundary.
    assert_eq!(mention_trigger("é@a", 1), None);
}

#[test]
fn complete_mention_replaces_the_token_before_the_caret() {
    assert_eq!(
        complete_mention("hi @al", 6, 7, "alice", "Alice"),
        Some(("hi @alice ".into(), 10))
    );
    assert_eq!(
        complete_mention("hi @al", 6, 7, "", "Al [x]"),
        Some(("hi [Al x](tg://user?id=7) ".into(), 26))
    );
    assert_eq!(complete_mention("plain", 5, 7, "alice", "Alice"), None);
    // Mid-text: the rest of the message stays after the mention.
    assert_eq!(
        complete_mention("hi @al there", 6, 7, "alice", "Alice"),
        Some(("hi @alice  there".into(), 10))
    );
}

#[test]
fn bot_command_insert_puts_the_command_first_and_replaces_a_leading_one() {
    assert_eq!(insert_bot_command_text("", "start"), "/start ");
    assert_eq!(insert_bot_command_text("   ", "start"), "/start    ");
    assert_eq!(insert_bot_command_text("hello", "start"), "/start hello");
    // An existing leading command is replaced, not stacked.
    assert_eq!(insert_bot_command_text("/help", "start"), "/start ");
    assert_eq!(insert_bot_command_text("/help me", "start"), "/start me");
    assert_eq!(
        insert_bot_command_text("/help@old_bot  me", "start"),
        "/start  me"
    );
    // Not a command token: kept.
    assert_eq!(
        insert_bot_command_text("/help!me", "start"),
        "/start /help!me"
    );
    assert_eq!(
        insert_bot_command_text("hi /help", "start"),
        "/start hi /help"
    );
}

#[test]
fn overlong_leading_command_is_not_replaced() {
    let long = format!("/{}", "a".repeat(65));
    assert_eq!(
        insert_bot_command_text(&long, "start"),
        format!("/start {long}")
    );
}

#[test]
fn switch_inline_insert_text() {
    // Phase 3.2: tapping a switchInline button inserts the query.
    assert_eq!(insert_switch_inline_text("", "pic"), "pic");
    assert_eq!(insert_switch_inline_text("   ", "pic"), "pic");
    assert_eq!(insert_switch_inline_text("hello", "pic"), "hello pic");
    assert_eq!(insert_switch_inline_text("hello ", "pic"), "hello pic");
}

fn bot_command(command: &str, description: &str) -> BotCommand {
    BotCommand {
        command: command.into(),
        description: description.into(),
        is_ephemeral: false,
    }
}

fn row(command: &str, bot: Option<CommandBot>) -> CommandMenuItem {
    CommandMenuItem {
        command: command.into(),
        description: String::new(),
        global: false,
        is_ephemeral: false,
        quick_reply: None,
        bot,
    }
}

fn group_bot(user_id: i64, username: &str) -> CommandBot {
    CommandBot {
        user_id,
        username: username.into(),
        name: username.into(),
        in_group: true,
    }
}

fn command(text: &str) -> Option<(std::ops::Range<usize>, String)> {
    command_menu_trigger(text, text.len()).map(|t| (t.range, t.query.to_string()))
}

#[test]
fn command_menu_trigger_matches_a_slash_that_opens_the_field() {
    assert_eq!(command("/"), Some((0..1, String::new())));
    assert_eq!(command("/st"), Some((0..3, "st".into())));
    assert_eq!(command("/ST"), Some((0..3, "ST".into())));
    assert_eq!(command("/start@my_b"), Some((0..11, "start@my_b".into())));
    assert_eq!(command("/start@"), Some((0..7, "start@".into())));
}

#[test]
fn command_menu_trigger_rejects_everything_else() {
    assert_eq!(command(""), None);
    assert_eq!(command("hello"), None);
    assert_eq!(command("a/b"), None);
    assert_eq!(command("hello a/b"), None);
    assert_eq!(command("http://x"), None);
    assert_eq!(command("/a/b"), None);
    assert_eq!(command("//"), None);
    // tdesktop only completes commands at the very start of the field.
    assert_eq!(command("hello /st"), None);
    assert_eq!(command("hi\n/he"), None);
    // Whitespace ends the command; so does a second `@`.
    assert_eq!(command("/start "), None);
    assert_eq!(command("/start@a@b"), None);
    assert_eq!(command("/@bot"), None);
    assert_eq!(command(&format!("/{}", "a".repeat(65))), None);
}

#[test]
fn command_menu_trigger_follows_the_caret() {
    let text = "/start now";
    let token = command_menu_trigger(text, 3).map(|t| (t.range, t.query));
    assert_eq!(token, Some((0..3, "st")));
    assert_eq!(command_menu_trigger(text, 7), None);
    assert_eq!(command_menu_trigger("é/a", 1), None);
}

#[test]
fn complete_command_at_reuses_a_following_space() {
    assert_eq!(
        complete_command_at("/st", 0..3, "/start"),
        Some(("/start ".into(), 7))
    );
    // The space after the caret is reused, and the caret moves past it.
    assert_eq!(
        complete_command_at("/st now", 0..3, "/start@b"),
        Some(("/start@b".into(), 9))
    );
    assert_eq!(complete_command_at("/st", 0..9, "/start"), None);
}

#[test]
fn inline_query_trigger_parses_leading_at_token() {
    assert_eq!(inline_query_trigger("@bot"), Some(("bot", "")));
    assert_eq!(inline_query_trigger("@bot "), Some(("bot", "")));
    assert_eq!(inline_query_trigger("@bot cats"), Some(("bot", "cats")));
    assert_eq!(
        inline_query_trigger("@gif_bot_1 cute cats"),
        Some(("gif_bot_1", "cute cats"))
    );
    // Token ends at the first non-username char; the rest is query.
    assert_eq!(inline_query_trigger("@bot!"), Some(("bot", "!")));
}

#[test]
fn inline_query_trigger_rejects_non_leading_at() {
    assert_eq!(inline_query_trigger(""), None);
    assert_eq!(inline_query_trigger("@"), None);
    assert_eq!(inline_query_trigger("hi @bot"), None);
    assert_eq!(inline_query_trigger(" @bot"), None);
    assert_eq!(inline_query_trigger("@@bot"), None);
}

#[test]
fn merge_command_menu_items_prefers_specific_descriptions() {
    let specific = vec![bot_command("start", "Start the bot")];
    let global = vec![
        bot_command("start", "Global start"),
        bot_command("settings", "Tweak the bot"),
    ];
    let items = merge_command_menu_items(&specific, &global);
    assert_eq!(items.len(), 2);
    // Bot-specific rows first; the duplicate keeps the bot-specific
    // description.
    assert_eq!(
        items.first(),
        Some(&CommandMenuItem {
            command: "start".into(),
            description: "Start the bot".into(),
            global: false,
            is_ephemeral: false,
            quick_reply: None,
            bot: None,
        })
    );
    assert_eq!(
        items.get(1),
        Some(&CommandMenuItem {
            command: "settings".into(),
            description: "Tweak the bot".into(),
            global: true,
            is_ephemeral: false,
            quick_reply: None,
            bot: None,
        })
    );
}

#[test]
fn merge_command_menu_items_empty_sides() {
    assert!(merge_command_menu_items(&[], &[]).is_empty());
    let items = merge_command_menu_items(&[bot_command("start", "")], &[]);
    assert_eq!(items.len(), 1);
    assert!(items.iter().all(|item| !item.global));
}

#[test]
fn merge_command_menu_items_keeps_ephemeral_flag() {
    // The `is_ephemeral` flag is what the menu icon reads: it must survive
    // the merge from both the bot-specific and the global side.
    let mut secret = bot_command("secret", "Only you see this");
    secret.is_ephemeral = true;
    let mut gsettings = bot_command("gsettings", "Global ephemeral");
    gsettings.is_ephemeral = true;
    let items = merge_command_menu_items(&[secret, bot_command("start", "")], &[gsettings]);
    let flags: Vec<(&str, bool, bool)> = items
        .iter()
        .map(|i| (i.command.as_str(), i.is_ephemeral, i.global))
        .collect();
    assert_eq!(
        flags,
        vec![
            ("secret", true, false),
            ("start", false, false),
            ("gsettings", true, true)
        ]
    );
}

#[test]
fn filter_command_menu_items_matches_prefix_case_insensitively() {
    let mut settings = row("settings", None);
    settings.global = true;
    let items = vec![row("start", None), settings, row("help", None)];
    let names = |prefix: &str| -> Vec<String> {
        filter_command_menu_items(&items, prefix)
            .iter()
            .map(|item| item.command.clone())
            .collect()
    };
    assert_eq!(names(""), vec!["start", "settings", "help"]);
    assert_eq!(names("st"), vec!["start"]);
    // Capitalized input still matches lowercase commands.
    assert_eq!(names("ST"), vec!["start"]);
    assert!(names("zzz").is_empty());
}

#[test]
fn group_rows_list_every_bot_in_the_given_order() {
    let rows = group_command_menu_items(&[
        (
            group_bot(1, "first_bot"),
            vec![bot_command("start", "A"), bot_command("help", "B")],
        ),
        (group_bot(2, "second_bot"), vec![bot_command("start", "C")]),
    ]);
    let listed: Vec<(i64, &str)> = rows
        .iter()
        .filter_map(|r| r.bot.as_ref().map(|b| (b.user_id, r.command.as_str())))
        .collect();
    assert_eq!(listed, vec![(1, "start"), (1, "help"), (2, "start")]);
    assert!(rows.iter().all(|r| !r.global && r.quick_reply.is_none()));
}

#[test]
fn group_rows_show_and_filter_by_command_at_bot() {
    let rows = group_command_menu_items(&[
        (group_bot(1, "first_bot"), vec![bot_command("start", "")]),
        (group_bot(2, "second_bot"), vec![bot_command("start", "")]),
    ]);
    assert!(commands_with_username(&rows, ""));
    let texts: Vec<String> = rows.iter().map(|r| r.command_text(true)).collect();
    assert_eq!(texts, vec!["/start@first_bot", "/start@second_bot"]);
    let by_bot = |prefix: &str| -> Vec<i64> {
        filter_command_menu_items(&rows, prefix)
            .iter()
            .filter_map(|r| r.bot.as_ref().map(|b| b.user_id))
            .collect()
    };
    assert_eq!(by_bot("start"), vec![1, 2]);
    assert_eq!(by_bot("start@sec"), vec![2]);
    assert_eq!(by_bot("START@FIRST"), vec![1]);
    // The bot name is part of the match, so a bare bot name finds nothing.
    assert!(by_bot("first").is_empty());
}

#[test]
fn private_bot_rows_only_carry_the_bot_name_when_typed() {
    let private = CommandBot {
        in_group: false,
        ..group_bot(5, "solo_bot")
    };
    let items = vec![row("start", Some(private))];
    assert!(!commands_with_username(&items, "st"));
    assert!(commands_with_username(&items, "start@so"));
    assert!(!commands_with_username(&items, "@so"));
    let first = items.first().map(|r| r.command_text(false));
    assert_eq!(first.as_deref(), Some("/start"));
    assert_eq!(filter_command_menu_items(&items, "start@so").len(), 1);
    assert!(filter_command_menu_items(&items, "start@zz").is_empty());
}

#[test]
fn command_text_without_a_username_stays_plain() {
    let nameless = group_bot(3, "");
    assert_eq!(row("start", Some(nameless)).command_text(true), "/start");
    assert_eq!(row("/start", None).command_text(true), "/start");
}
