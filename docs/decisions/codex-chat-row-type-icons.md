# Chat rows: type icons and photo thumbnails in previews (tdesktop)

- **Type icons.** tdesktop's dialog rows put a small group / channel / bot
  icon before the chat name. Rows now show `Users` for groups,
  `Megaphone` for channels and `Bot` for bot chats, in the muted color.
- **Preview thumbnails.** tdesktop shows a tiny rounded thumbnail of the
  last message's media before the preview text. A chat now keeps its last
  photo's minithumbnail (`last_preview_thumb`; inline JPEG, no download;
  never for secret or spoiler photos), and the row draws it at 18 px.
  Video/GIF thumbnails need a download and are left for a follow-up.

Verified live.
