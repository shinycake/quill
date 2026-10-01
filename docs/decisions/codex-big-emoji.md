# Big emoji

Media settings includes a persisted Big emoji switch, enabled by default with a serde default for older preference files. The shared history text renderer enlarges unformatted messages consisting of one, two or three Unicode emoji to at least 40, 36 or 32 px; larger user-selected font sizes remain respected. Captions, sponsored content, linked previews and formatted/entity-bearing text retain their regular rendering.

Grapheme segmentation reuses the already-installed unicode-segmentation dependency (now declared directly; no package/version change). A lazily cached set from the picker catalog validates each complete grapheme, including flags, skin tones, keycaps and ZWJ sequences. Whitespace is ignored for the count; mixed text, more than three emoji and explicit text-presentation selectors are excluded.

Validation: regression checks flags/skin tones/ZWJ/keycaps, empty/whitespace/mixed text, four emoji and text presentation. Preference round trip and older-file defaults cover the switch. Core clippy and UI compilation pass.
