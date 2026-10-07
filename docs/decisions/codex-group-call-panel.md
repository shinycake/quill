# Voice / video chats: real audio, real mute, tdesktop's window

## Two engine bugs

- **Group calls were silent.** `issue_group_sources` gave ntgcalls null
  microphone and speaker descriptions (the docs called it "no audio
  transport in this slice"). You couldn't be heard and heard no one.
  Group calls now take the system's default microphone for capture and
  the default output for playback, with the same device metadata the
  1:1 path uses (an empty macOS UID follows the current default). The
  playback "microphone" receiver carries the mixed voices, as in P2P.
- **Self-mute did nothing.** It flipped a session flag "for the next
  join". It now does two things:
  - mutes the microphone (`ntg_mute` / `ntg_unmute` on the group's
    chat, new `set_group_muted`), reapplied whenever the transport
    connects;
  - tells Telegram with `toggleGroupCallParticipantIsMuted` on yourself,
    so everyone sees the muted icon.

  A driver test covers both effects.

## The window (tdesktop `Calls::Group::Panel`)

A dark window of its own, about 420 × 640 (at least 380 × 520), on
`groupCallBg` #1a2026:
- **Top:** the title (or "Video Chat"), then the participant count, or
  "Recording mm:ss", or "Starts in…". A ⋯ menu holds:
  - Share Screen, Show/Hide Chat, Share/Copy Invite Link;
  - for admins: Revoke Invite Link, Stream With…, Edit Title,
    Start/Stop Recording, Mute New Participants, Allow Chat, End Video
    Chat.
- **Video tiles** (yours included) sit above the members, two across.
- **Members** sit on `groupCallMembersBg` #2c333d, under an "Invite
  Members" row. Each row has a 40 pt avatar, the name, and a status:
  "speaking" (#8deb90), "listening" / "wants to speak" (#61c0ff),
  "muted for you". On the right: a camera/screen icon when streaming,
  and the mic colored by state (active green, idle gray, muted by admin
  red) or a raised hand. Right-click or ⋯ opens the member's actions:
  - Allow to Speak / Lower Hand for a raised hand;
  - Mute or Mute for Me / Unmute for Me;
  - Louder / Quieter;
  - Remove (owner).
- **Bottom:** Video · the big button · Leave (`groupCallLeaveBg`). The
  big button is an 84 pt gradient disc in tdesktop's colors, with a
  halo that breathes faster while you're live:

| State | Colors | Label | Click |
| --- | --- | --- | --- |
| Live | #0dcc39 → #0bb6bd | You are Live | mute |
| Muted | #0992ef → #16ccfb | Unmute | unmute |
| Muted by admin | #9b52e9 → #eb5353 | Muted by admin | raise hand |
| Hand raised | #9b52e9 → #eb5353 | You asked to speak | — |
| Not joined | blue | Join | join |
| Scheduled | blue | Notify Me / Cancel Reminder / Start Now | as labeled |
| Connecting | gray | Connecting... | — |

Banners show a lost connection (click to rejoin) and failed requests
(click to dismiss). The title editor and the RTMP stream key render in
the window. The old modal card, its join prompt and its participant
tiles are gone.

**Window life**, as for 1:1 calls: it opens with the tracked chat, and
closing it keeps you in the chat. A 38 pt bar in the main window, in the
button's gradient, has mute, the title and count, and leave; a click
brings the window back.

Verified with the group-call demo fixture: tiles, the member list, the
member menu, live → muted. A live voice chat needs other people and
wasn't joined.
