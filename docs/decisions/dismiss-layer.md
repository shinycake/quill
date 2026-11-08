# Layers that close on an outside click: `DismissLayer`

## What was wrong

In the story viewer, a click on the story (its picture, pause, mute) also
closed the viewer. The same thing could happen in every layer Quill draws by
hand that closes when you click outside it.

GPUI hit-tests every hitbox under the pointer, topmost first, and stops at
the first one that occludes (`.occlude()`). Each `on_click` fires when its
own hitbox is hovered. These layers were built as a full-window backdrop
that closes on click, with the panel drawn beside it (or inside it). When
the panel did not occlude, the backdrop under it was hovered too, so a click
on the panel closed the layer. When the backdrop did not occlude, a click
outside the panel also reached whatever was drawn behind it. The same
applies to hover and scroll. Each layer had to remember the right
combination, and several did not:

| Layer | What a click did |
| --- | --- |
| Story composer | a click on the panel's padding, labels or empty space closed it and dropped the draft |
| Story page (albums, archive) | a click inside the panel closed it |
| Privacy editor and exceptions | a click inside the dialog closed it |
| Group call stream boxes | a click outside closed the box and also pressed the call control under it |
| Call device menu | a click outside closed the menu and pressed the call button under it; its own toggle reopened it |
| Share QR, color/emoji/gift panel | a click outside closed the panel and pressed the box control under it (Close shut the whole box) |
| Status toast | a click dismissed the toast and also hit what was under it (over the media viewer it closed the viewer) |
| Custom emoji card | clicks, hover and scroll went through to the messages under it |

The story viewer itself is fixed in its own change. Kit dialogs, sheets,
popovers and menus were already correct: they occlude their content.

## What changed

`src/ui/dismiss_layer.rs` adds `DismissLayer`, the one arrangement that is
correct. The layer covers the window and occludes, so nothing behind it is
reached. A backdrop over it takes the dismissing click. Content goes in only
through `content()` (which makes the element occlude) or `anchored()`
(which wraps it in an occluding box), so a click, hover or scroll on the
content never reaches the backdrop. A press on the content released outside
does not dismiss, matching a click outside a tdesktop layer or popup.

Every hand-built dismiss layer now uses it: the menus (message, chat row,
archive, folder tab, playlist, story), the reaction selector in the main and
chat windows, the link popup, the profile and earn layers, and the layers in
the table above. The toast and the custom emoji card occlude.

## The guard

`scripts/check-click-through.py` (CI step "click through") fails on any
`div()` under `src/ui` that covers its parent and takes a click or a press
itself, outside `dismiss_layer.rs`. Elements that cover their parent for
another reason say so with `// click-through-ok: <reason>`: the media viewer
backdrop (the media and every control over it occlude, `chrome.rs`), the
long-press chat preview catcher, the power-saving cover and the story viewer
backdrop.

Tests: `dismiss_layer.rs` (content clicks, outside clicks, a drag out of the
content, hover and scroll, for both content kinds) and
`click_through_tests.rs` (the story composer, story page, privacy editor,
the toast over the media viewer, the Share QR panel and the call device
menu). Each fails on the old arrangement.
