# Info panels and avatars

**Contact panel.** Primary actions (Call, Video, Secret chat, Add contact or Edit profile) are labeled icon tiles under the name. Details (Bio, Username, Phone) are a bordered list, value over label; NANP numbers are grouped for reading (`+1 555 010 1031`). Destructive actions (Block / Unblock user, Delete contact) are full-width rows at the bottom in the danger color, replacing a mix of filled danger buttons and centered ghost buttons.

**Group/channel panel.** "Manage channel/group" actions are left-aligned rows (`pressable::action_row`, focusable and keyboard-activatable; kit buttons always center their label). Section "Refresh" / "Create" / "Add" text buttons become small icon buttons with tooltips. "members unknown" / "subscribers unknown" no longer render; an unknown count simply isn't shown.

**Avatars.** gpui-kit's avatar fallback, for custom sizes, sets the initials' *box* to half the avatar instead of the font size, so initials were small and off-center at every Quill size (38, 46, 96…). Photo avatars still use the kit `Avatar`. The initials fallback is drawn by Quill with the same 12-hue OkLCH identity colors (keyed by the initials, like the kit), with centered text at 40% of the size. Single-word names show one initial.

Also registers the `Lock` icon. The secret-chat glyph added in the chat-list slice had been rendering blank because only the kit's default icons are embedded.
