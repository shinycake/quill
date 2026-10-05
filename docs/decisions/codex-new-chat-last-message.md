# Starting preview from `updateNewChat.chat.last_message`

`chat.last_message` on `updateNewChat` (and `chat` responses, parsed by the same `parse_new_chat`) was ignored. A newly loaded chat therefore had no preview, time or receipt until TDLib happened to send an `updateChatLastMessage`, which TDLib reports only for later changes. Telegram X seeds the preview from the chat object. Flagged by both core audits (#317, #324).

The payload now carries `last_message` (boxed). It is applied through a new `Session::set_chat_last_message`, which `updateChatLastMessage` also uses, so both paths set identical fields. It never replaces a newer last message already known from an `updateChatLastMessage` that arrived first. Regression test: `state::tests::chat_list::new_chat_carries_its_last_message_preview`.
