//! Service text wording: checklists, polls, suggested posts, bots, calls and expired media.
use super::*;

wording!(
    checklist_tasks_done,
    BY_DANA,
    r#"{"@type":"messageChecklistTasksDone","checklist_message_id":5,"marked_as_done_task_ids":[1,2],"marked_as_not_done_task_ids":[]}"#,
    "Dana Cole marked 2 tasks as done."
);
wording!(
    checklist_task_undone,
    BY_ME,
    r#"{"@type":"messageChecklistTasksDone","checklist_message_id":5,"marked_as_done_task_ids":[],"marked_as_not_done_task_ids":[1]}"#,
    "You marked task as not done."
);
wording!(
    checklist_tasks_added,
    BY_DANA,
    r#"{"@type":"messageChecklistTasksAdded","checklist_message_id":5,"tasks":[{"@type":"checklistTask","id":3,"text":{"text":"Buy milk","entities":[]}},{"@type":"checklistTask","id":4,"text":{"text":"Pack bags","entities":[]}}]}"#,
    "Dana Cole added Buy milk and Pack bags to the list."
);
wording!(
    poll_option_added,
    BY_DANA,
    r#"{"@type":"messagePollOptionAdded","poll_message_id":5,"option_id":"3","text":{"text":"Pizza","entities":[]}}"#,
    "Dana Cole added \"Pizza\" to the poll."
);
wording!(
    poll_option_removed,
    BY_ME,
    r#"{"@type":"messagePollOptionDeleted","poll_message_id":5,"option_id":"3","text":{"text":"Pizza","entities":[]}}"#,
    "You removed \"Pizza\" from the poll."
);
wording!(
    suggested_post_approval_failed,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostApprovalFailed","suggested_post_message_id":5,"price":{"@type":"suggestedPostPriceStar","star_count":10}}"#,
    "Transaction failed."
);
wording!(
    suggested_post_approved,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostApproved","suggested_post_message_id":5,"price":{"@type":"suggestedPostPriceStar","star_count":10},"send_date":1}"#,
    "Agreement reached!"
);
wording!(
    suggested_post_declined,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostDeclined","suggested_post_message_id":5,"comment":""}"#,
    "The post was rejected."
);
wording!(
    suggested_post_paid,
    BY_DANA,
    r#"{"@type":"messageSuggestedPostPaid","suggested_post_message_id":5,"star_amount":{"@type":"starAmount","star_count":40,"nanostar_count":0},"gram_amount":0}"#,
    "Dana Cole has received 40 Stars for publishing post."
);
wording!(
    suggested_post_refunded_user,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostRefunded","suggested_post_message_id":5,"reason":{"@type":"suggestedPostRefundReasonPaymentRefunded"}}"#,
    "User refunded the Stars so that post was deleted."
);
wording!(
    suggested_post_refunded_admin,
    IN_CHANNEL,
    r#"{"@type":"messageSuggestedPostRefunded","suggested_post_message_id":5,"reason":{"@type":"suggestedPostRefundReasonPostDeleted"}}"#,
    "Admin deleted the post early so that the price was refunded to the user."
);
wording!(
    contact_registered,
    BY_DANA,
    r#"{"@type":"messageContactRegistered"}"#,
    "Dana Cole joined Telegram"
);
wording!(
    users_shared,
    BY_ME,
    r#"{"@type":"messageUsersShared","users":[{"@type":"sharedUser","user_id":2,"first_name":"Omar","last_name":"Haddad"},{"@type":"sharedUser","user_id":3,"first_name":"Lea","last_name":"Stern"}],"button_id":1}"#,
    "You shared Omar Haddad and Lea Stern with Design Club"
);
wording!(
    chat_shared,
    BY_ME,
    r#"{"@type":"messageChatShared","chat":{"@type":"sharedChat","chat_id":-2002,"title":"Launch Channel"},"button_id":1}"#,
    "You shared Launch Channel with Design Club"
);
wording!(
    bot_allowed_on_website,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonConnectedWebsite","domain_name":"example.org"}}"#,
    "You allowed this bot to message you when you logged in on example.org."
);
wording!(
    bot_allowed_from_attach_menu,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonAddedToAttachmentMenu"}}"#,
    "You allowed this bot to message you when you added it to your attachment menu."
);
wording!(
    bot_allowed_from_web_app,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonLaunchedWebApp","web_app":{"@type":"webApp","short_name":"shop","title":"Shop"}}}"#,
    "You allowed this bot to message you when you opened Shop."
);
wording!(
    bot_allowed_by_request,
    BY_ME,
    r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonAcceptedRequest"}}"#,
    "You allowed this bot to message you in its web-app."
);
wording!(
    web_app_data_sent,
    BY_ME,
    r#"{"@type":"messageWebAppDataSent","button_text":"Order"}"#,
    "Data from the \"Order\" button was transferred to the bot."
);
wording!(
    web_app_data_received,
    BY_DANA,
    r#"{"@type":"messageWebAppDataReceived","button_text":"Order","data":"{}"}"#,
    "Data from the \"Order\" button was received."
);
wording!(
    passport_data_sent,
    BY_ME,
    r#"{"@type":"messagePassportDataSent","types":[{"@type":"passportElementTypePersonalDetails"},{"@type":"passportElementTypeIdentityCard"},{"@type":"passportElementTypeBankStatement"},{"@type":"passportElementTypePhoneNumber"}]}"#,
    "Design Club received the following documents: personal details, proof of identity, proof of address, phone number"
);
wording!(
    passport_data_received,
    BY_DANA,
    r#"{"@type":"messagePassportDataReceived","elements":[],"credentials":{}}"#,
    "Telegram Passport data received"
);
wording!(
    proximity_traveler_is_me,
    BY_ME,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":9},"watcher_id":{"@type":"messageSenderUser","user_id":1},"distance":250}"#,
    "You are now within 250 meters from Dana Cole"
);
wording!(
    proximity_watcher_is_me,
    BY_DANA,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":1},"watcher_id":{"@type":"messageSenderUser","user_id":9},"distance":1500}"#,
    "Dana Cole is now within 1.5 km from you"
);
wording!(
    proximity_between_others,
    BY_DANA,
    r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":1},"watcher_id":{"@type":"messageSenderUser","user_id":2},"distance":1}"#,
    "Dana Cole is now within 1 meter from Omar Haddad"
);
wording!(
    video_chat_started,
    BY_DANA,
    r#"{"@type":"messageVideoChatStarted","group_call_id":5}"#,
    "Dana Cole started a video chat"
);
wording!(
    live_stream_started,
    IN_CHANNEL,
    r#"{"@type":"messageVideoChatStarted","group_call_id":5}"#,
    "Live stream started"
);
wording!(
    video_chat_ended,
    BY_DANA,
    r#"{"@type":"messageVideoChatEnded","duration":3725}"#,
    "Dana Cole ended the video chat (62 minutes)"
);
wording!(
    video_chat_ended_short,
    BY_DANA,
    r#"{"@type":"messageVideoChatEnded","duration":45}"#,
    "Dana Cole ended the video chat (45 seconds)"
);
wording!(
    live_stream_finished,
    IN_CHANNEL,
    r#"{"@type":"messageVideoChatEnded","duration":7300}"#,
    "Live stream finished (2 hours)"
);
wording!(
    video_chat_invite,
    BY_DANA,
    r#"{"@type":"messageInviteVideoChatParticipants","group_call_id":5,"user_ids":[2,3]}"#,
    "Dana Cole invited Omar Haddad and Lea Stern to the video chat"
);
wording!(
    expired_photo,
    BY_DANA,
    r#"{"@type":"messageExpiredPhoto"}"#,
    "Expired photo"
);
wording!(
    expired_video,
    BY_DANA,
    r#"{"@type":"messageExpiredVideo"}"#,
    "Expired video"
);
wording!(
    expired_round_video,
    BY_DANA,
    r#"{"@type":"messageExpiredVideoNote"}"#,
    "Round message expired"
);
wording!(
    expired_voice_message,
    BY_DANA,
    r#"{"@type":"messageExpiredVoiceNote"}"#,
    "Voice message expired"
);
wording!(
    stake_dice_won,
    BY_DANA,
    r#"{"@type":"messageStakeDice","value":6,"stake_gram_amount":100,"prize_gram_amount":300}"#,
    "Dana Cole won 2 Grams"
);
wording!(
    stake_dice_lost,
    BY_ME,
    r#"{"@type":"messageStakeDice","value":1,"stake_gram_amount":100,"prize_gram_amount":0}"#,
    "You lost 1 Gram"
);
wording!(
    story_mention,
    BY_DANA,
    r#"{"@type":"messageStory","story_poster_chat_id":1,"story_id":4,"via_mention":true}"#,
    "Mentioned in a story"
);
wording!(
    paid_media,
    BY_DANA,
    r#"{"@type":"messagePaidMedia","star_count":25,"media":[],"caption":{"text":"","entities":[]}}"#,
    "Paid media \u{B7} 25 Stars"
);
wording!(
    auto_delete_set_by_other,
    BY_DANA,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":86400,"from_user_id":1}"#,
    "Dana Cole set messages to auto-delete in 1 day"
);
wording!(
    auto_delete_set_by_me,
    BY_ME,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":604800,"from_user_id":9}"#,
    "You set messages to auto-delete in 1 week"
);
wording!(
    auto_delete_off,
    BY_DANA,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":0,"from_user_id":1}"#,
    "Dana Cole disabled the auto-delete timer"
);
wording!(
    auto_delete_in_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":0}"#,
    "Messages in this channel will be automatically deleted after 1 hour"
);
wording!(
    screenshot_by_other,
    BY_DANA,
    r#"{"@type":"messageScreenshotTaken"}"#,
    "Dana Cole took a screenshot!"
);
wording!(
    screenshot_by_me,
    BY_ME,
    r#"{"@type":"messageScreenshotTaken"}"#,
    "You took a screenshot!"
);
