# The 1:1 call window (tdesktop `Calls::Panel`)

A call used to be a light modal card over the chat. Its controls
overflowed the card, internal notes showed ("Waiting for audio
transport", "tiles fill in as cameras stream"), and the device lists
sat inline. There was no sound at all, so an incoming call rang
silently.

**Window.** tdesktop runs a call in a dark window of its own:
- 720 × 540, at least 380 × 520, with transparent title chrome;
- the background is a radial gradient of the peer's profile accent
  palette (`PanelBackground`), or `callBgOpaque` #1b1f23 without one;
- the photo is 160 pt (100 pt next to a camera preview), the name 21 pt
  semibold white, the status 14 pt #aaabac;
- the four E2E emoji sit in a pill at the top, with tdesktop's tooltip.

Status follows `lng_call_status_*`:
- outgoing: "requesting..." (not created), "waiting...", "ringing..."
  (received by the peer);
- incoming: "is calling you...";
- then "exchanging encryption keys...", "connecting..." until the media
  connects, then the duration (m:ss);
- at the end: "hanging up...", "failed to connect", "call ended",
  "line busy".

A ring turns around the photo while connecting (`callConnectingRadial`).

**Buttons** follow tdesktop's `CallButton`: 68 × 79, a 44 pt disc and an
11 pt label.
- Translucent white (`callIconBg`) is on; solid white with a dark icon
  (`callIconBgActive`) is off: a muted mic, the camera off.
- Answer is green #66c95b with a breathing outer ring; hang-up is red
  #d75a5a with the handset turned 135°.

| Moment | Buttons |
| --- | --- |
| Ringing in | Start Video · Decline · Accept |
| In a call | Start/Stop Video · Screencast (once connected) · Mute/Unmute · End Call |
| Line busy | Cancel · Redial |

The window lingers on "call ended" for 1.6 s, then closes.

**Video.** The peer's camera (or screen share) fills the window, with
top and bottom shades under the name and buttons. Your camera floats in a
rounded 160 × 110 corner above the buttons.

**Window life.** It opens (focused) when a call appears and comes to the
front once when a call rings in. Closing it doesn't end the call (as in
tdesktop): the main window's call bar (`Calls::TopBar`) brings it back.
The bar is 38 pt in the accent color (gray when muted), with mute, the
name and status, and hang up.

The old modal now only asks for a rating after a call, or reports a call
that failed to start.

**Sounds.** Telegram's call sounds are GPL; Quill is MIT. On macOS Quill
plays the system's FaceTime sounds at runtime:
- the Opening ringtone, looping, for a call ringing in;
- the ringback (`vc~ringing`), looping once the peer's phone rings;
- `vc~invitation-accepted` when the media connects;
- `vc~ended`, or the CEPT busy tone for a declined outgoing call;
- the mute / unmute clicks.

Nothing plays where these files don't exist. `CallSummary.busy` marks an
outgoing call declined before it connected.

Verified with the demo fixtures (a real incoming-call window, the
connected video call, the bar reopening a closed window). A live call
needs a second person and wasn't placed. The engine path (ntgcalls) is
unchanged; group calls are next.
