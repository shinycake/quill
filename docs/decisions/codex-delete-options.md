# Delete for everyone, delete dissolve, spoiler cover (tdesktop)

**Delete options.** tdesktop's `DeleteMessagesBox` asks "Do you want to
delete this message?" and adds a revoke checkbox, checked by default. In a
private chat it reads "Also delete for {name}", elsewhere "Delete for
everyone". The checkbox shows only when TDLib's message properties report
`can_be_deleted_for_all_users`, and never in Saved Messages. Quill's menu
Delete now opens that dialog (`open_delete_dialog`). `delete_confirmed`
revokes only when the box was offered and left checked. Before this,
outgoing messages were always revoked and incoming ones never were.

**Dissolve.** tdesktop runs `Ui::ThanosEffect` on deleted messages:
400–600 ms, during which the bubble crumbles into particles that drift
away. Quill snapshots the messages when the user confirms
(`ui/vanish.rs`). Once TDLib drops them from history, the snapshot is
merged back as a ghost row for 600 ms. The ghost fades, and 90 grains in
the bubble's color sweep left to right, drifting up and outward. The
ghost is held for at most 10 s if the server never removes it. The
history rows hash includes the animation clock, so rows rebuild on each
tick only while something is dissolving.

**Spoiler media.**
- The cover now matches Telegram: the minithumbnail is blurred and
  lightly shaded, and dense fine grains (about one per 55 px², capped)
  twinkle and drift.
- A revealed spoiler opens the media viewer on the next click, like
  tdesktop. The viewer now includes spoiler media; only secret media stays
  out.

Verified live in Saved Messages: the dialog without a checkbox (Saved
Messages), my own test text dissolving, and spoiler reveal followed by the
viewer.
