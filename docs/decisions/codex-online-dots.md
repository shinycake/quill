# codex/online-dots — presence on chat list avatars

- A private chat whose user is online gets a 13 px success-colored dot at the avatar's
  bottom-right, ringed in the sidebar color.
- `Session::chat_peer_online` reads the user's `userStatusOnline`, which `updateUser` and
  `updateUserStatus` already keep current. It is false for bots and the account's own chat.
  Covered by a driver test.
