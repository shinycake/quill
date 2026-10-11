# Emoji set style picker (parity:emoji-set-style) - not implemented

## What tdesktop does

Settings > Chat Settings has "Manage emoji sets" (`lng_emoji_manage_sets`, `Ui::Emoji::ManageSetsBox` in
`chat_helpers/emoji_sets_manager.cpp`). It lists four sets: Mac (built in, id 0), Android, Twemoji and
JoyPixels. Mac is the bundled default. The other three are blobs of webp sprite sheets (about 6-8 MB each)
downloaded on demand through MTProto from posts 3223-3225 of the `@tdhbcfiles` channel (`MTP::DedicatedLoader`
via `Storage::CloudBlob`), unpacked to the data folder, and then lib_ui's `emoji_config` draws every emoji
in the app from those sprites (`SwitchToSet`, `CurrentSetId`). It is not an `emoji_sets` cloud config.

## Why Quill does not implement it

- Quill draws emoji as ordinary text through the system colour emoji font (CoreText, DirectWrite, fontconfig).
  GPUI has no hook to substitute glyph runs with bitmap sprites, so every text surface (bubbles, dialogs,
  composer, previews, reactions) would need inline-image text layout. That is engine work in GPUI, not a
  settings feature, and it would cost shaping, selection and RTL behaviour that Quill already relies on.
- The sets come from third-party art (Noto, Twemoji, JoyPixels). Bundling is out; the runtime source is a
  Telegram channel blob that only an MTProto dedicated-file loader can fetch, and the `@tdhbcfiles` post ids
  are internal to tdesktop's update infrastructure, not a documented public API.
- The Mac set is already what each OS shows natively, so the picker would be a no-op on the default.

## Minimal path if revisited

1. Add inline-image glyph support to the vendored GPUI text system (sprite atlas lookup per grapheme cluster).
2. Add a `DedicatedLoader` equivalent in the TDLib layer to fetch the set posts, then a persisted
   `emoji_set` setting and a picker box in the chat settings.

## Verification

Research only: read `emoji_sets_manager.cpp`, `emoji_sets_manager.h`, `storage_cloud_blob.h`, and Quill's
emoji rendering paths. No code changed, so no gate beyond the docs-only checks.
