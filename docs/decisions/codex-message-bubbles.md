# Message bubbles: inline text flow, in-bubble time, day separators

**Local time.** Message times, receipts and scheduled/payment stamps were formatted in UTC on purpose (deterministic CI), so every user outside UTC saw wrong times. `quill::local_time` now formats over an explicit UTC offset (pure, unit-tested) and the app reads the OS offset for each instant (`localtime_r`/`tm_gmtoff`, so DST is honored). Non-Unix targets fall back to UTC until a Windows zone lookup is added.

**Inline text.** Each entity run used to be its own flex item, so a bold word or link in the middle of a sentence broke onto its own line. A paragraph is now one `StyledText` with highlight ranges, mono font overrides for `code`, and `InteractiveText` click ranges for links and spoilers. Quotes and `pre` blocks are still block elements. Paragraphs with resolved custom-emoji images keep the per-run layout, because images can't live inside a text run.

**Time inside the bubble.** The footer (time + receipt) is painted inside the bubble's bottom-right corner. When the bubble ends with text, the last paragraph reserves trailing invisible space (em-spaces, so it scales with font size). The footer then shares the last line when it fits and wraps to its own line when it doesn't, with no text measuring. Otherwise (media, keyboards, reactions) the footer gets a right-aligned row.

**Message actions.** The permanent "•••" row inside every bubble is now a small hover-revealed button in the bubble corner. Right-click opens the same menu.

**Day separators.** A centered pill ("Today", "Yesterday", weekday, "12 March", "12 March 2025") precedes the first message of each local day.

**Row measurement bug.** The virtualized list caches row heights. A row that grows in place (a photo finishing its download, an edit, new reactions) was clipped. Rows whose inputs changed are now remeasured, and so is everything when file readiness changes. The media `extra` container is now an explicit flex column; as a block container, it let a photo's caption collapse to zero height.

Out of scope: edge-to-edge media with an overlaid time pill, grouped-bubble corner radii, composer and chat-list redesign (separate slices).
