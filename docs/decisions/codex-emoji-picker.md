# Plain emoji keyboard

The composer has an Emoji picker with nine Unicode categories, live name/emoji search, account-local Recent and Clear recent. The full 3,944 fully-qualified Unicode 17.0 entries include skin tone, flag and ZWJ sequences. Data is derived from Unicode's emoji-test.txt and distributed with its Unicode data license; English annotation search works offline. TDLib getEmojiCategories describes sticker/animation search categories, so it is not used as a plain Unicode keyboard.

Each page renders at most 120 buttons in a height-bounded scroll container. Native TextareaState::replace replaces the current selection or inserts at the cursor, preserves the text widget's undo/IME handling, emits its normal Change event for draft/typing/suggestion updates, and returns focus to the composer. Recent stores 48 unique known emoji in existing account media preferences, with a serde default for older preference files. Account switching clears picker and search state.

Validation: catalog uniqueness/category coverage, flag/ZWJ/skin-tone search, recency dedup/cap/invalid-input handling and preference round trip/backward compatibility. Existing media preference tests, core clippy and UI compilation pass. Native keyboard glyph coverage depends on the OS emoji font; live Telegram sending is not exercised by this slice.
