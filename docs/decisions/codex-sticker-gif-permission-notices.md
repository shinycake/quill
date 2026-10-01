# Sticker/GIF permission denials

Classify the pinned TDLib MessageContent.cpp permission errors and Telegram's documented CHAT_SEND_STICKERS_FORBIDDEN / CHAT_SEND_GIFS_FORBIDDEN errors before dropping native text. Both send request errors and updateMessageSendFailed surface explicit permission notices through the existing kit notification path. Forwarding, inline sends and retries use the same shared path. Failed asynchronous rows retain their retry eligibility and are never marked sent. Unknown errors keep existing handling.

Telegram decides permission: no client inference from default group permissions that could incorrectly block administrators or miss individual restrictions. Classification retains only safe enum values, never raw messages. Regression checks all four known errors, direct/forward/inline/retry sends, async failure rows, non-retryable preservation, unknown errors and fetch-error isolation. Local core tests/clippy and UI compile pass; live denied sends remain unverified.

Server error reference: https://core.telegram.org/method/messages.sendMedia and https://core.telegram.org/method/messages.sendInlineBotResult.
