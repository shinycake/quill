# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The product itself is a native desktop app (macOS, Windows, Linux) whose interface follows the gpui-kit Design Guides, not Impeccable. This record covers the web surfaces about it: the product page in `site/`.

## Users

People who use Telegram on a computer every day and care how their desktop software is made: developers, Rust and Zed users, Linux users, and people who keep a chat window open all day for work and communities. They arrive from a link (Hacker News, Reddit, GitHub, a friend), give the page under a minute, and decide whether to install an unofficial client on the machine they work on. (Inferred from the README's positioning and the project's audience; not confirmed in an interview.)

## Product Purpose

Quill is an independent, open-source Telegram client for macOS, Windows and Linux, written from scratch in Rust. It signs in to a real Telegram account through Telegram's own TDLib library and draws its whole interface on the GPU with GPUI, the framework from the Zed editor. Success for the page: a visitor understands what Quill is, trusts it enough to try it, and installs it.

## Positioning

A native, GPU-drawn desktop client that is not a clone of Telegram Desktop: no browser engine, no web view, one codebase on three platforms, aiming to feel fast, behave reliably and be drivable from the keyboard. Measured against Telegram Desktop on the same Mac, it uses about a third of the memory and puts its window on screen about four times sooner.

## Operating Context

Visitors compare Quill to the official Telegram Desktop app and to other unofficial clients. They worry about account safety (bans), privacy (does this app read my messages), and installing unsigned software. The page serves macOS, Windows and Linux visitors; installation is a one-line terminal command per platform or a manual download with OS-specific first-launch steps.

## Capabilities and Constraints

- Messaging, media, voice and video messages with transcription, calls with screen sharing, group voice chats, groups and channels with admin tools and statistics, bots, secret chats, passcode lock, themes, accent colors, wallpapers, multiple accounts.
- Early, experimental software. No warranty. Users should keep an official Telegram app installed.
- Builds are not signed with paid Apple or Microsoft certificates; browser downloads need a one-time first-launch confirmation.
- No analytics in the app. The website counts visits with GoatCounter (cookieless) and says so. Besides Telegram, the app only contacts GitHub to check for updates, and that can be switched off.
- Release data (version, assets, notes) is injected at deploy time from GitHub Releases into `site/data/release.json`.
- Hosted on GitHub Pages at https://shinycake.github.io/quill/ (static files only, no server).

## Brand Commitments

- The app is always called Quill. "Telegram" is used only for the actual service, accounts, TDLib/API and official apps.
- Unofficial and not affiliated with Telegram; the page must say so plainly.
- Plain, direct, human copy; no marketing hype and no generated-sounding prose.
- Performance claims against Telegram Desktop are limited to measured memory and launch time, stated as relative comparisons (no absolute MB or seconds), each with machine, app versions, date and method. No CPU claims.
- The page lists what Quill does; it does not catalogue what is missing or why.
- Public showcase material uses no right-to-left demo text.
- No wallet or TON features are offered or promised.
- Existing icon: `site/img/icon-256.webp` (quill feather mark).

## Evidence on Hand

- Real screenshots captured from the app with demo data (`site/img/*.webp`, `docs/screenshots/`), and a hero video (`site/media/hero-*.mp4`). New captures in any theme can be produced from the app's screenshot demos.
- The open-source repository, MIT license, third-party license list.
- Memory and launch benchmarks against Telegram Desktop on an Apple M1 Pro (quill-tools/bench/RESULTS.md, 8 October 2026; launch comparison 18 October 2026).
- HDR photo support for pictures sent as files (docs/decisions/hdr-viewer-macos.md, hdr-viewer-windows.md).
- No testimonials, user counts, press, stars-as-proof or download numbers. Do not invent any.

## Product Principles

1. Trust earns the install: say what works and what the risks are before the visitor asks, and back every number with its method.
2. Show the real app; a capture of the window beats any description.
3. Respect the visitor's machine: explain exactly what an installer does before asking them to run it.
4. Native and craft-first; claim only the speed and memory that were measured.

## Accessibility & Inclusion

WCAG 2.2 AA for the page: keyboard reachable, visible focus, contrast, reduced-motion respected, meaningful alt text on every capture.
