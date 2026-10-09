# Curated release notes

Before tagging `vX.Y.Z`, add `release-notes/vX.Y.Z.md` with the highlights you
want at the top of the release. `.github/workflows/release.yml` puts its
Markdown under "What's new", above the generated list of merged pull requests
since the previous tag, and copies the same text into `latest.json`
(`notes_markdown`) for the product page. Without the file, the release has
only the generated list.

Write plain Markdown without a top-level heading; the workflow adds the headings.
