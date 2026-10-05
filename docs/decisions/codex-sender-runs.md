# Sender runs in group chats

Consecutive messages from one sender form a run. The sender's name heads the run, and the avatar now sits beside its **last** message, where it used to sit on the first. Earlier rows in the run get a transparent spacer of the same width, so their bubbles align. A run start gets a little extra top spacing; rows inside a run sit tight. The kit's avatar slot painted a muted circle behind its content, so Quill now supplies a transparent `MessageAvatar` slot; real avatars paint their own background.

The slow-mode fixture adds a second consecutive message from one member to show a run.
