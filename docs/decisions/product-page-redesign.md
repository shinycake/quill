# Product page redesign (2026-10-22)

The page in `site/` (see `codex-product-page.md`) gets new content, an icon-only palette, a deslop pass and a search layer.

## Direction

A from-scratch redesign (a "QSL card" look with ultramarine, vermilion and Archivo) was built first and rejected by the owner, who preferred the existing page and asked to keep to the icon's colours. The page therefore keeps its structure and look, with these changes:

- **Colour:** only the app icon's blues. Dark mode uses sky #5fc7fb for accents with dark text on blue buttons; light mode uses deep #0a6fb8 with white text. The purple and teal hero glows, the purple gradient on the headline and the orange focus rings in the dialog drawings are gone.
- **Deslop pass** (Impeccable detector plus the copywriting rules): no eyebrow label above the headline, no gradient text, no coloured glow shadows. Feature icons sit beside their titles instead of in tiles above them. The faint text colours now pass 4.5:1. The screenshot gallery drops its fake window buttons, since the captures have their own.

## Content changes

- **Measured numbers.** A line under the hero buttons states memory and launch time against Telegram Desktop and links to a "Lighter and faster, measured" section with the machine, versions, dates and method.
  - Memory: `quill-tools/bench/RESULTS.md`, 8 October 2026, Telegram Desktop 7.2.9, median of three runs.
  - Launch: the outside-in window timer from 18 October 2026, Telegram Desktop 7.3.1.
  - The page leaves out CPU, because Quill's idle CPU is higher, and it says Windows and Linux haven't been measured.
- **An HDR section** covers HDR photos sent as files, with the platform and format details from `hdr-viewer-macos.md` and `hdr-viewer-windows.md`. Its captures come from the `ready-hdr-file-viewer` and `ready-hdr-files` demos.
- **"What's missing" is gone**, along with `site/data/missing.json` and its nav link.
- **New screenshots** come from the screenshot demos in light and dark: chat (both themes), channel, polls, reactions, player, accent colour, Appearance settings and the media viewer, as 2000 px WebP files in the existing gallery tabs.
- **Headline animation:** on load, the white feather from the app icon writes the headline (about 7 s). The nib sweeps across each letter and reveals it as it goes, the ink starts glossy and dries, and the pen lifts between words and lines with a shadow that grows as it rises. Then it flies to the header, curling and turning as it shrinks, and becomes the feather of the logo. Until it lands, the header shows `icon-base-256.webp`, the icon with the feather removed and the Q's ring closed. The animation is skipped with reduced motion or a window resize, and the heading keeps its full text for search and screen readers.
- **Hero video:** re-recorded with `scripts/hero/record.sh` (Cap Studio, the `ready-showcase` mock data, a current release build). `tour.sh` now targets a window under the menu bar of a 1512×982-point laptop screen and the current chat-list layout, which has an Archived row at the top.
- **Questions** gain "How is Quill different from Telegram Desktop?" and "Is Quill free?".

## Search and AI search

- **Page metadata:** the title, description, canonical URL, Open Graph and Twitter tags are new.
- **JSON-LD graph:** `SoftwareApplication`, `Organization`, `WebSite` and `FAQPage`. Google retired FAQ rich results, so the FAQ markup only describes the visible questions.
- **`site/sitemap.xml` and `site/llms.txt`:** the second is a plain-text summary with the measured numbers and their method.
- **No robots.txt:** crawlers only read it at a domain's root, and `shinycake.github.io/quill/` is a subpath, so a robots file here would do nothing.

## Visit counting

The page loads GoatCounter's script, which counts visits to `quill.goatcounter.com`. GoatCounter sets no cookies and stores no personal data, and the page's questions section says the site counts visits. The `quill` code must be registered at goatcounter.com before any counts show up; until then the script's requests fail without affecting the page.
