# Export saved stories and profile music

Account export now pages the account's story archive and profile audio alongside profile photos. Short pages continue; repeated story anchors are excluded. Raw story fields survive export, and story/profile-audio files feed the existing private media exporter. Stories that disallow forwarding remain excluded. Export stays incomplete until Telegram takeout-only saved contacts and left-channel histories are supported.

Validation: the existing streaming regression now returns short, overlapping story pages, a protected story, unknown story fields, and a short profile-audio page. It verifies both exported stories, deduplicated media, continued paging and protected-media exclusion. The replacement release containing PRs 303–307 is running and signed in; its chat navigation and main menu passed native accessibility checks.
