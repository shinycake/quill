//! Parses TDLib objects for bots: inline queries, callback buttons, commands, games and login URLs.
use crate::ids::{ChatId, UserId};
use crate::telegram::envelope::*;
use serde_json::Value;

/// The bots domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_bots_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        // Bots slice: `inlineQueryResults` — the `getInlineQueryResults`
        // answer (schema 1.8.67, line 7716). Missing fields degrade to
        // empty strings; thumbnails are dropped (no URL on the wire).
        // Slice S9: `inlineQueryResultAnimation` entries (schema line
        // 7658) additionally yield parsed `AnimationItem`s (+ files) for
        // the GIF panel search; Loop 3's summaries are untouched.
        "inlineQueryResults" => {
            let mut results = Vec::new();
            let mut animations = Vec::new();
            let mut files: Vec<ParsedFile> = Vec::new();
            if let Some(entries) = value.get("results").and_then(Value::as_array) {
                for entry in entries {
                    results.push(parse_inline_query_result(entry));
                    if entry.get("@type").and_then(Value::as_str)
                        == Some("inlineQueryResultAnimation")
                    {
                        let (item, item_files) = parse_animation_value(entry.get("animation"));
                        files.extend(item_files);
                        if let Some(item) = item {
                            animations.push(item);
                        }
                    }
                }
            }
            files.retain(|file| file.id.0 != 0);
            Ok(EnvelopePayload::Bots(BotsPayload::InlineQueryResults(
                InlineQueryResultsPage {
                    inline_query_id: value
                        .get("inline_query_id")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    button: value
                        .get("button")
                        .filter(|button| !button.is_null())
                        .map(parse_inline_query_results_button),
                    results,
                    animations,
                    files,
                    next_offset: value
                        .get("next_offset")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                },
            )))
        }
        "callbackQueryAnswer" => Ok(EnvelopePayload::Bots(BotsPayload::CallbackQueryAnswer(
            parse_callback_query_answer(value),
        ))),
        // Slice bots-games: `getGameHighScores` answer (TDLib 1.8.67,
        // `schema/td_api.tl:13174`).
        "gameHighScores" => Ok(EnvelopePayload::Bots(BotsPayload::GameHighScores(
            parse_game_high_scores(value),
        ))),
        // B1: `getLoginUrlInfo` answers (TDLib 1.8.67, `schema/td_api.tl:3862`
        // / `:3869`).
        "loginUrlInfoOpen" => Ok(EnvelopePayload::Bots(BotsPayload::LoginUrlInfo(
            LoginUrlInfo::Open {
                url: json_field_str(value, "url"),
            },
        ))),
        "loginUrlInfoRequestConfirmation" => Ok(EnvelopePayload::Bots(BotsPayload::LoginUrlInfo(
            LoginUrlInfo::RequestConfirmation {
                domain: json_field_str(value, "domain"),
                request_write_access: value
                    .get("request_write_access")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        ))),
        "updateChatReplyMarkup" => {
            let message = match value.get("reply_markup_message") {
                Some(m) if !m.is_null() => Some(parse_message(m)?),
                _ => None,
            };
            Ok(EnvelopePayload::Bots(BotsPayload::UpdateChatReplyMarkup {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: message.as_ref().map(|m| m.id),
                reply_markup: message.and_then(|m| m.reply_markup),
            }))
        }
        "botCommands" => Ok(EnvelopePayload::Bots(BotsPayload::BotCommands {
            bot_user_id: UserId(int53(value.get("bot_user_id"))?),
            commands: value
                .get("commands")
                .and_then(Value::as_array)
                .map(|commands| {
                    commands
                        .iter()
                        .filter_map(|command| parse_bot_command(Some(command)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        // Mini apps (docs/decisions/codex-miniapp-webview.md).
        "webAppInfo" => Ok(EnvelopePayload::Bots(BotsPayload::WebAppInfo {
            launch_id: int53_or_zero(value.get("launch_id")),
            url: web_app_url(value),
        })),
        "webAppUrl" => Ok(EnvelopePayload::Bots(BotsPayload::WebAppUrl {
            url: json_field_str(value, "url"),
        })),
        "mainWebApp" => Ok(EnvelopePayload::Bots(BotsPayload::MainWebApp {
            url: web_app_url(value),
        })),
        "foundWebApp" => Ok(EnvelopePayload::Bots(BotsPayload::FoundWebApp(
            parse_found_web_app(value),
        ))),
        "attachmentMenuBot" => match parse_attachment_menu_bot(value) {
            Some(bot) => Ok(EnvelopePayload::Bots(BotsPayload::AttachmentMenuBot(bot))),
            None => Err(ParseError::MissingField),
        },
        "updateAttachmentMenuBots" => Ok(EnvelopePayload::Bots(
            BotsPayload::UpdateAttachmentMenuBots(parse_attachment_menu_bots(value)),
        )),
        "updateWebAppMessageSent" => Ok(EnvelopePayload::Bots(
            BotsPayload::UpdateWebAppMessageSent {
                launch_id: int53_or_zero(value.get("web_app_launch_id")),
            },
        )),
        "customRequestResult" => Ok(EnvelopePayload::Bots(BotsPayload::CustomRequestResult {
            result: json_field_str(value, "result"),
        })),
        _ => return Ok(None),
    };
    payload.map(Some)
}
