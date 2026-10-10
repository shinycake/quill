//! ScreenshotDemo enum.

/// Forced UI surfaces for screenshot proof (no live Telegram / no real credentials).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotDemo {
    NeedTdjson,
    WaitPhone,
    WaitCode,
    /// Sign-in polish: the country picker open with a search query.
    WaitPhoneCountry,
    /// Sign-in polish: a pasted international number, grouped as typed.
    WaitPhoneFormatted,
    /// Sign-in polish: code step with the resend countdown and "Wrong number?".
    WaitCodeResend,
    /// Sign-in polish: tdesktop's banned-number box (with Help).
    WaitPhoneBanned,
    WaitPassword,
    WaitPremium,
    /// Slice A1: injected `authorizationStateWaitOtherDeviceConfirmation`
    /// with a fake link, rendered as a real QR (no live Telegram).
    WaitQr,
    /// Unexpected `authorizationStateClosed`: the Retry card.
    ConnectionClosed,
    ReadyChats,
    ReadyTrayBehavior,
    ReadyUpdateInstall,
    ReadyUpdateChangelog,
    ReadyUpdateFailure,
    ReadyDeepLinkInfo,
    ReadyDeepLinkInvite,
    /// The share-link chat chooser (typed deep links).
    ReadyDeepLinkShare,
    /// Slice parity:platform-offline-indicator — the ReadyChats fixture
    /// with `connection` forced to `WaitingForNetwork`, so the offline
    /// banner renders below the title bar (injected, no live Telegram).
    ReadyOffline,
    /// Slice parity:platform-offline-errors — ReadyOffline fixture plus
    /// the product offline-send toast ("You're offline — will send when
    /// you reconnect") so kit notifications proof the note.
    ReadyOfflineToast,
    /// Slice parity:platform-reconnect-states — the ReadyChats fixture
    /// with `connection` forced to `Updating`, so the transitional strip
    /// renders with its per-state label ("Updating…").
    ReadyReconnecting,
    ReadyChatsComposer,
    /// Composer `#ru` with the recent-hashtag popup open.
    ReadySuggestHashtag,
    /// Composer `:fire` with the emoji suggestion strip open.
    ReadySuggestEmoji,
    ReadyUnread,
    ReadyUnreadRead,
    ReadyMedia,
    /// Composer attachment chip + outgoing photo/document (injected, no live Telegram).
    ReadySendMedia,
    /// Paste-image: composer with a pasted clipboard photo attachment chip
    /// (injected, no live Telegram). Visible outcome of Ctrl/Cmd+V image paste.
    ReadyPasteImage,
    /// Sidebar search over injected recents / `searchChats` / `searchMessages`.
    ReadySearch,
    /// In-chat search (`searchChatMessages`) + jump-to-message.
    ReadySearchInChat,
    /// Reply-to-message: composer quote + history quote strip.
    ReadyReply,
    /// Own-message edit mode + delete confirm (injected, no live Telegram).
    ReadyEditDelete,
    /// Forward select + dest picker + success (injected, no live Telegram).
    ReadyForward,
    /// B4: the share box with two destinations ticked and a comment.
    ReadyShareBox,
    /// B4: the forward bar above the destination chat's composer.
    ReadyForwardBar,
    /// B4: the composer's "send as" identity list.
    ReadySendAs,
    /// Message selection mode: check circles, selection tint and the
    /// Forward N / Delete N / Cancel header (injected, no live Telegram).
    ReadySelectMode,
    /// Selection mode over a photo and a document that is not downloaded
    /// yet: keyboard focus ring and the Download / Save buttons.
    ReadySelectKeyboard,
    /// Reply bar above the composer for a photo message, with its thumbnail.
    ReadyReplyMedia,
    /// Edit bar above the composer for an outgoing photo (caption edit).
    ReadyEditMedia,
    /// A new message revealing at the bottom of the history; freeze the
    /// frame with `QUILL_MOTION_HOLD_MS`.
    ReadyReveal,
    /// Emoji react / unreact + chips (injected, no live Telegram).
    ReadyReactions,
    /// Pin / unpin + pinned banner (injected, no live Telegram).
    ReadyPin,
    /// Mute presets + muted icon + archive section (injected, no live Telegram).
    ReadyMuteArchive,
    /// Slice CL1: chat list with a pinned chat, archived section,
    /// marked-as-unread row, and the row context menu open (injected,
    /// no live Telegram).
    ReadyChatListMenu,
    /// Slice CL2: chat list with folder tabs, All/Unread/Archived
    /// category chips, pinned + unread chats, an expanded archive
    /// section, and the Saved Messages entry (injected, no live
    /// Telegram).
    ReadyChatList,
    /// Slice CL3: chat list with the @ mention badge, the ♥ reaction
    /// badge, multi-select mode (two chats checked + the select bar),
    /// and the row menu open showing Report / Block user (injected,
    /// no live Telegram).
    ReadyChatList3,
    /// Slice CL: the floating peek preview open on "Demo chat B" with
    /// recent messages (injected, no live Telegram).
    ReadyChatPreview,
    /// Slice CL2: the archive auto-settings dialog over the
    /// `ReadyChatList` fixture (injected settings, no live Telegram).
    ReadyChatListArchive,
    /// Slice CL2: sidebar search with an empty result (injected, no
    /// live Telegram).
    ReadyChatListSearch,
    /// Archived-chats row on top of the chat list (names + muted unread
    /// badge) with story rings on avatars and three pinned chats.
    ReadyArchiveRow,
    /// Chat-list rows: the Archive's "How does it work?" box.
    ReadyArchiveHint,
    /// Chat-list rows: video chat badge and emoji status on rows.
    ReadyChatBadges,
    /// Chat-list rows: the folder editor's chat sections.
    ReadyFoldersChats,
    /// Chat-list rows: the folder editor's chat picker with search.
    ReadyFoldersChatPicker,
    /// Chat-list rows: toast after adding a chat to a folder.
    ReadyFoldersToast,
    /// Same, with `archiveCollapsed`: the slim bar.
    ReadyArchiveBar,
    /// Same, with the archive row's context menu open.
    ReadyArchiveMenu,
    /// Same, mid pinned-drag: the dragged row follows the pointer while
    /// the displaced one slides home.
    ReadyPinDrag,
    /// Chat-row swipe: "Mira Cohen" held mid-swipe (ratio 0.6) with the
    /// Mute action revealed; the list is long enough to scroll.
    ReadySwipeMute,
    /// "Noam Katz" with action Delete, held past the threshold (ratio 1.25) with the
    /// reach circle fully grown.
    ReadySwipeReached,
    /// Stories strip expanded at the top of a long chat list, swipe action
    /// Mute configured, nothing held (scripted gestures start here).
    ReadyStoriesExpanded,
    /// The list scrolled half the strip's height: the compact stack is
    /// fading in beside the search field.
    ReadyStoriesCollapsing,
    /// The list scrolled past the strip: collapsed to the small stack.
    ReadyStoriesCollapsed,
    /// Slice media-shared-gallery: per-chat shared-media gallery open on
    /// chat 11 — the Media tab shows its empty state, the Files tab two
    /// injected documents (injected `foundChatMessages` through the real
    /// reducer, no live Telegram).
    ReadySharedMedia,
    /// Peer `chatActionTyping` in the open-chat header and sidebar row.
    ReadyTyping,
    /// Chat-row polish: Draft prefix, sending / failed marks, verified /
    /// Premium / SCAM / FAKE title badges, online dot (injected, no live
    /// Telegram).
    ReadyChatRows,
    /// Non-member public channel opened from search: the bottom bar must
    /// resolve to "Join channel" (injected, no live Telegram).
    ReadyJoinBar,
    /// Batch 8: chat top bars; `QUILL_DEMO_BAR` picks the variant.
    ReadyTopBars,
    /// Chat-list search with a custom-emoji, multi-line public-chat preview
    /// and a multi-line chat preview (injected, no live Telegram).
    ReadySearchPreviews,
    /// Chat-list rows whose last message has hard newlines / a leading
    /// custom emoji (injected, no live Telegram).
    ReadyMultilineRows,
    /// Sticker panel + sticker in history (injected, no live Telegram).
    ReadyStickers,
    ReadyStickerPlayback,
    /// Voice record bar + history playback (injected, no live Telegram).
    ReadyVoice,
    /// Link entities + web page (`linkPreview`) card (injected, no live Telegram).
    ReadyLinkPreview,
    /// MED4: composer with a typed URL → detected-URL chip + preview
    /// toggle (injected, no live Telegram).
    ReadyComposerPreview,
    /// Composer core: files dragged over the chat show two drop zones
    /// (photos: quick versus without compression).
    ReadyDropZones,
    /// Composer core: the "Code Language" box over a fenced block.
    ReadyCodeLanguage,
    /// Composer core: a dragged folder offers its files or one archive.
    ReadyDropFolder,
    /// MED4: embedded-player + album `linkPreview` cards in bubbles
    /// (injected, no live Telegram).
    ReadyPreviewCards,
    /// MED4: photo messages with caption above vs below the media
    /// (`show_caption_above_media`, injected, no live Telegram).
    ReadyCaptionPosition,
    /// Saved-GIF panel + a playing animation in history (injected, no live Telegram).
    ReadyGifs,
    ReadyGifPlayback,
    /// Video bubble with Play/Pause in history (injected, no live Telegram).
    ReadyVideo,
    /// Slice bots-games: `messageGame` card with an open high-score panel
    /// (injected, no live Telegram).
    ReadyGameCard,
    /// Round video note with Play/Pause in history (injected, no live Telegram).
    ReadyVideoNote,
    /// Music file bubble with title, performer, cover, and Play/Pause.
    ReadyAudio,
    /// The top "now playing" bar over a music chat (audio, repeat all, shuffle).
    ReadyPlayerBar,
    /// Composer video attach chip plus an own-sent video playing in history.
    ReadyVideoSend,
    /// Composer video-note attach chip plus an own-sent round note in history.
    ReadyVideoNoteSend,
    /// Restored private-chat composer draft (`draftMessage`).
    ReadyDrafts,
    /// Received photo album plus an own-sent album and a multi-attach composer.
    ReadyAlbums,
    /// The composer's `@` member suggestions.
    ReadyMentions,
    /// The composer's emoji / sticker / GIF panel on the Emoji tab.
    ReadyEmojiPanel,
    /// Channel sponsored / recommended rows + report flow (injected, no live Telegram).
    /// Fixture/proof surface only; the channel opens normally in live use.
    ReadySponsored,
    /// Custom emoji rendered inline in message text (injected
    /// `textEntityTypeCustomEmoji` entity + resolved sticker fixture).
    ReadyCustomEmoji,
    /// Animated emoji suggestion above the composer (injected
    /// `animatedEmoji` answer + downloaded sticker fixture).
    ReadyAnimatedEmoji,
    ReadyEmojiPacks,
    /// Broadcast channel demo (injected, no live Telegram): the ungated demo
    /// channel (id 13) renders broadcast posts with channel author + view
    /// counts, composer hidden for the non-admin viewer, and the join/leave
    /// footer.
    ReadyChannels,
    /// Broadcast channel demo (injected, no live Telegram): the demo channel
    /// (id 13) with the viewer as an administrator
    /// (`rights.can_post_messages: true`), so the composer is visible above
    /// the broadcast posts (Phase 2.3).
    ReadyChannelsAdmin,
    /// Channel statistics demo (injected, no live Telegram): the demo
    /// channel (id 13) with `supergroupFullInfo.can_get_statistics: true`
    /// and a loaded `chatStatisticsChannel` fixture (Phase D2), so the
    /// statistics panel renders directly in the info panel.
    ReadyChannelStats,
    /// Phase D3a: synthetic invite-links + join-requests surface (no live
    /// TDLib): the demo channel (id 13) with the viewer as an admin with
    /// `can_invite_users`, a loaded invite-link list and loaded join
    /// requests, so the info-panel sections render directly.
    ReadyInviteLinks,
    /// Phase D3b: synthetic admin-management surface (no live TDLib):
    /// the demo channel (id 13) with the viewer as an admin with
    /// `can_promote_members`, a loaded administrator list (owner +
    /// two editable admins), a loaded member page for the promote
    /// picker, and the promote dialog open with a member selected, so
    /// the info-panel section and the rights checkboxes render directly.
    ReadyAdminManagement,
    /// Phase D3c: synthetic admin-log surface (no live TDLib): the demo
    /// channel (id 13) with the viewer as an administrator, plus a
    /// loaded `chatEvents` fixture covering the handled action types, so
    /// the info panel's "Recent actions" section renders directly.
    ReadyAdminLog,
    /// Admin extras (`QUILL_DEMO_ADMIN_EXTRAS=log|title|broadcast|warning|
    /// delete`; injected data, no live Telegram).
    ReadyAdminExtras,
    /// Slice G2: channel-management surface (no live TDLib): like
    /// `ReadyAdminLog` (demo channel id 13, viewer 777 is an admin), plus
    /// signature flags (`sign_messages` on, `show_message_sender` off),
    /// a seeded boost status, the `can_send_welcome_messages` right, and
    /// a loaded one-message welcome pack, so the info panel's signatures,
    /// boost, welcome-message, and recent-actions sections render
    /// directly.
    ReadyGroups2,
    /// Slice G1: synthetic group-management surface (no live TDLib):
    /// demo supergroup (id 61) with the viewer as an administrator
    /// (`can_restrict_members`, `can_invite_users`, `can_manage_tags`),
    /// a loaded `chatMembers` page (member, admin with custom title,
    /// restricted member), and the member-management dialog open on the
    /// All tab, so member rows, custom titles, and per-tab actions
    /// render directly.
    ReadyGroupManage,
    /// Slice G8: group info-edit demo (injected, no live Telegram):
    /// the `ReadyGroupManage` demo supergroup (id 61, viewer an admin
    /// with `can_change_info`) with the info panel open, so the new
    /// Edit title / Edit description / Change photo rows render
    /// directly.
    ReadyGroupInfoEdit,
    /// Slice G10: communities create dialog (injected, no live
    /// Telegram): the create dialog open over the seeded chat list, so
    /// the name field, chat picker, and hide-checkbox render directly.
    ReadyCommunityCreate,
    /// Slice G10: communities hub dialog (injected, no live Telegram):
    /// two injected communities with the hub open.
    ReadyCommunityHub,
    /// Slice G10: community info panel (injected, no live Telegram):
    /// the "Rustaceans" community with its injected full-info pack, so
    /// the name edit, counts, and chat rows render directly.
    ReadyCommunityInfo,
    /// Bot chat demo (injected, no live Telegram): private chat with a
    /// `userTypeBot` user (id 21), opened with history plus a cached
    /// `botInfo` (description + commands), so the bot panel renders under
    /// the header and the composer is visible (Phase 3.1).
    ReadyBotChat,
    /// Inline keyboard demo (injected, no live Telegram): like
    /// `ReadyBotChat`, but the bot message carries a
    /// `replyMarkupInlineKeyboard` with URL / callback / switchInline /
    /// copy-text / unknown (disabled) buttons (Phase 3.2).
    ReadyBotKeyboard,
    /// Bot command menu demo (injected, no live Telegram): like
    /// `ReadyBotChat`, plus a cached `getCommands` response (global
    /// scope) so the `/` command menu renders open above the composer
    /// with the bot-specific and "Global" sections (Phase 3.3).
    ReadyBotCommandMenu,
    /// Inline-mode demo (injected, no live Telegram): like
    /// `ReadyBotChat`, but the Demo Bot is an inline bot (`@gif`) with
    /// an injected resolved slot and a loaded results page, so the
    /// `@bot` inline-results dropdown renders open above the composer
    /// (bots slice).
    ReadyInlineResults,
    /// Bot profile actions demo (injected, no live Telegram): like
    /// `ReadyBotChat`, plus an armed `bot_start_params` entry (START
    /// button), `botInfo` with a menu button and a privacy-policy URL,
    /// and a loaded `getBotSimilarBots` answer (Slice B2).
    ReadyBotProfile,
    /// Text-entity demo (injected, no live Telegram): a message with mixed
    /// entities (bold/italic/underline/strikethrough/spoiler/code/pre, incl.
    /// nested runs) plus a photo whose caption carries entities (Phase 4.1).
    ReadyTextEntities,
    ReadyUnsupportedMessage,
    /// Expandable block quotes (injected, no live Telegram): a short quote
    /// fully visible and a long quote collapsed to 3 lines with a kit ghost
    /// "Show more" affordance (parity:msg-blockquote-expandable).
    ReadyBlockquoteExpandable,
    /// RTL composer demo: a Hebrew / mixed / multi-paragraph draft chosen by
    /// `QUILL_DEMO_RTL=he|mixed|lines` (default `he`).
    ReadyRtlComposer,
    /// RTL polish demo: Hebrew chat-list previews, search results, short and
    /// multi-line Hebrew bubbles with their time footers, a Hebrew reply and a
    /// pinned message. `QUILL_DEMO_RTL_VIEW=chat|search` (default `chat`).
    ReadyRtlPolish,
    /// Service-message tour in a group: members, pins with excerpt,
    /// photo change, calls, gifts, giveaways, topics, timers, boosts.
    ReadyServiceMessages,
    /// Private chat with the cards of the render-service-media slice:
    /// suggested photo and birthday, expired media, live locations with
    /// Stop sharing, and link previews with a View button.
    ReadyServiceMedia,
    /// Channel comments and reply threads (injected, no live Telegram):
    /// `QUILL_DEMO_THREADS_VIEW=posts` shows channel posts with comment
    /// bars, `thread` a post's comment thread in its discussion group,
    /// `group` a group message with replies.
    ReadyThreads,
    /// Forums and Saved Messages sublists (injected, no live Telegram):
    /// `QUILL_DEMO_FORUMS_SAVED_VIEW=sublists|sublist|tag|editor` shows the
    /// sublist list, one sublist, a tag filter, or the forum topic editor
    /// with its icon picker (default `sublists`).
    ReadyForumsSaved,
    /// Bubble headers and footer (injected, no live Telegram): replies with
    /// a colored sender name, a quote, a media thumbnail, a reply from another
    /// chat and a deleted original; forwards from a user, a hidden account, a
    /// channel and an imported message; "via @bot"; and the footer's
    /// "edited" / pin / views / "imported" marks.
    ReadyBubbleHeaders,
    /// Message rendering leftovers (injected, no live Telegram): grouped
    /// bubbles with joined corners, contact cards with Message / Add
    /// contact / View contact, a dice, a location with its map tile,
    /// locked paid media, a suggested profile photo with its buttons, the
    /// media viewer header and the attach menu's Contact / Location panels.
    /// `QUILL_DEMO_RENDERING_VIEW=bubbles|cards|service|viewer|contact|location`
    /// (default `bubbles`).
    ReadyRenderingLeftovers,
    /// Translation demo (injected, no live Telegram): the translate bar,
    /// translated bubbles, the translate box, the language chooser and the
    /// settings. `QUILL_DEMO_TRANSLATE_VIEW=bar|translated|box|rtl|selection|chooser|settings|skip`
    /// (default `bar`).
    ReadyTranslate,
    /// A bot chat whose reply keyboard comes from `updateChatReplyMarkup`
    /// (message outside the window), with request buttons and the share
    /// dialogs. `QUILL_DEMO_KEYBOARD_VIEW=keyboard|hidden|phone|users|chat|confirm`.
    ReadyReplyKeyboard,
    /// README showcase scene: a populated account (generated avatars and
    /// photos, a lively group conversation); `QUILL_DEMO_SHOWCASE` picks
    /// the view.
    ReadyShowcase,
    /// The message context menu over every kind of message (injected, no
    /// live Telegram). `QUILL_DEMO_MENU` picks the scenario: photo,
    /// document, downloading, video, gif, sticker, audio, audio-save-to,
    /// voice-private, uploading, group, group-audience, channel,
    /// protected, report-pick, report-sub, report-text, report-done,
    /// sticker-set, moderate.
    ReadyMessageMenu,
    /// Poll demo (injected, no live Telegram): an open regular poll with a
    /// voted option (percentage bars + counts, tapping an option flips the
    /// chosen mark locally) and a closed poll (results, no voting
    /// affordance) (Phase 4.2).
    ReadyPoll,
    /// Location / venue / contact demo (injected, no live Telegram): a
    /// plain `messageLocation` (coordinates + accuracy), a
    /// `messageLiveLocation` (live-period / expires / heading /
    /// proximity-alert state), a `messageVenue` (title + address +
    /// provider), and a `messageContact` (name + phone + vCard +
    /// `user_id`), each with a tappable "Open map" link (Phase 4.3).
    ReadyLocation,
    /// Dice demo (injected, no live Telegram): a few `messageDice` rows —
    /// 🎲 with values 4 / 6 and a 🎯 — showing the large static emoji
    /// face plus the rolled value (Phase 4.4). No roll animation.
    ReadyDice,
    /// Media viewer demo (injected, no live Telegram): the ReadyMedia
    /// photo chat with the viewer overlay open on the downloaded photo
    /// (Phase 4.5).
    ReadyMediaViewer,
    /// Video-playback demo (injected, no live Telegram): the ReadyMedia
    /// seed plus a downloaded video (message 204); the viewer opens on it
    /// with playback faked mid-track (no audio — the tick
    /// advances the elapsed label, like the seek-bars demo). The clip's
    /// 12 s duration is fixture data for the screenshot.
    /// (Parity slice 5.)
    ReadyVideoPlayback,
    ReadyVideoPip,
    /// A playing video in the viewer with a formatted caption, the saved
    /// toast and the open speed dial.
    ReadyViewerExtras,
    /// A GIF looping in the viewer, with a custom emoji in its caption.
    ReadyViewerGif,
    /// The viewer paging over the Shared Media panel's photos.
    ReadyViewerShared,
    /// Story viewer demo (injected, no live Telegram): the story tray above
    /// the chat list for "Demo chat A"/"Demo chat B" plus the story viewer
    /// overlay open on Demo chat A's downloaded photo story (Phase 9.1).
    ReadyStories,
    /// Story posting / custom-reaction slice demo (injected, no live
    /// Telegram): same seed as `ReadyStories`, but Demo chat A's photo
    /// story carries a chosen ❤ reaction, interaction counts, and
    /// deletable/repliable flags; the viewer opens with the **reaction
    /// picker** and **reply row** visible, plus seeded `availableReactions`
    /// (emoji + custom-emoji Premium tile with a local sticker thumb —
    /// Phase 9.2 / `parity:stories-custom-reactions`). The demo keeps its
    /// viewer-only shape — the posting composer is the `ReadyStoryComposer`
    /// demo (Phase 9.3).
    ReadyStoryPost,
    /// Phase 9.3: story posting composer (injected, no live Telegram) —
    /// the `ReadyStories` fixture plus the composer overlay open: a
    /// seeded photo path (the demo thumbnail), a caption draft, the
    /// privacy selector on Close friends, and a seeded
    /// `canPostStoryResultOk` so the status line shows "✓ Eligible to
    /// post".
    ReadyStoryComposer,
    /// Phase 9.5: story viewers list (injected, no live Telegram) — the
    /// `ReadyStoryPost` fixture plus a seeded `getStoryInteractions`
    /// response (two viewers, one with a ❤ reaction, one forward) with
    /// the viewers panel open on the own story.
    ReadyStoryViewers,
    /// Phase 9.5: story edit composer (injected, no live Telegram) —
    /// the seeded own photo story (id 5, chat 11) carries
    /// `can_be_edited`, `is_edited`, a `storyRepostInfo` public origin,
    /// link + suggested-reaction areas, and close-friends privacy; the
    /// composer opens in edit mode with caption + area inputs prefilled
    /// and the media path empty (keep current content).
    ReadyStoryEdit,
    /// Phase 9.7: story albums / chat page / archive (injected, no live
    /// Telegram) — the `ReadyStories` seed plus `storyAlbums`,
    /// chat-page `stories` (one pinned) and archive `stories` for chat
    /// 11, with the story page overlay open.
    ReadyStoryAlbums,
    /// Phase 9.8: clickable story areas (injected, no live Telegram) —
    /// a photo story carrying one of every `storyAreaType` (location,
    /// venue, suggested reaction, message, link, weather, gift) with
    /// the viewer open on it.
    ReadyStoryAreas,
    /// B14: a video story playing in the viewer (the generated 12 s demo
    /// clip through the native player; no live Telegram).
    ReadyStoryVideo,
    /// B14: the story viewer's close-friends editor, hide and profile
    /// actions (injected, no live Telegram).
    ReadyStoryMore,
    /// MED3 downloads-manager demo (injected, no live Telegram): the
    /// `ReadyMedia` seed plus an actively downloading document (file 24,
    /// 42% through `notes.txt`), a failed document (file 26, "Retry"
    /// chip), and a completed one (file 25, `report.pdf`) sitting in the
    /// recent list — with the downloads panel open beside the
    /// conversation, showing per-file progress, cancel, open and
    /// reveal-in-folder rows.
    ReadyDownloads,
    /// Seek-bar demo (injected, no live Telegram): a voice note playing
    /// with its seek bar mid-track (elapsed advancing via the playback
    /// tick) plus a music track paused with a remembered position, both
    /// showing elapsed/total time labels (Phase 4.6). No subprocess is
    /// spawned — playback state is faked.
    ReadySeekBars,
    /// Forum-topics demo (injected, no live Telegram): a forum supergroup
    /// whose `updateSupergroup` marks it a forum and whose `getForumTopics`
    /// response seeds three topics (General pinned + unread, Announcements
    /// with a last-message preview, Random closed), shown as the topic
    /// list (Phase 5.1).
    ReadyForumTopics,
    /// Topic-posting demo (injected, no live Telegram): the forum's General
    /// topic is open with an injected two-message history and the composer
    /// enabled — posting routes `sendMessage` with
    /// `topic_id = messageTopicForum` (parity slice 4).
    ReadyTopicPost,
    /// Subsection-tabs demos (injected): a bot with topics
    /// (`userTypeBot.has_topics`) with the tabs on Top / Bottom / Left.
    ReadyBotTopics,
    ReadyBotTopicsBottom,
    ReadyBotTopicsLeft,
    /// Contacts demo (injected, no live Telegram): the sidebar shows the
    /// **Contacts** tab (three injected contacts: Ada online, Zed last
    /// seen within a week, Noor recently) and the user info panel is open
    /// for Zed (bio from an injected `userFullInfo`) with the **Add
    /// contact** affordance (Phase 6).
    ReadyContacts,
    /// Slice A6: contacts management — the **Contacts** tab with the
    /// settings section (sync toggle on, Import contacts…, Delete
    /// synced contacts…, notice) and the user info panel open for Ada
    /// (a contact) showing **Delete contact** + **Block user**
    /// (injected, no live Telegram).
    ReadyContactsManage,
    /// Slice A6: the block-user confirm dialog open for Ada ("Are you
    /// sure you want to block Ada Lovelace?", red Block button — TGX
    /// `BlockUserConfirm`) over the contacts fixture (injected, no live
    /// Telegram).
    ReadyBlockUser,
    /// Phase 7.1: folder tabs (injected `updateChatFolders` + folder
    /// positions) with the non-default "News" folder selected, so the chat
    /// list shows only that folder's chats.
    ReadyFolders,
    /// Parity slice: folder manage dialog over the ReadyFolders fixture
    /// (injected, no live Telegram).
    ReadyFoldersManage,
    /// Shareable folders slice: Share Folder dialog: invite links over the folder fixture.
    ReadyFoldersShare,
    /// Shareable folders slice: folders in the left column ("Tabs on the left") with icons.
    ReadyFoldersSidebar,
    /// Shareable folders slice: "Add folder" for an addlist link.
    ReadyFoldersAddLink,
    /// Shareable folders slice: folder editor with the icon picker.
    ReadyFoldersIcons,
    /// Folder follow-ups: folder tag chips on chat rows.
    ReadyFoldersTags,
    /// Folder follow-ups: folder editor with the tag colour picker.
    ReadyFoldersTagColor,
    /// Folder follow-ups: right-click menu of a folder tab.
    ReadyFoldersMenu,
    /// Folder follow-ups: shared folder with the new chats bar.
    ReadyFoldersNewChats,
    /// Folder follow-ups: join dialog of a shared folder's new chats.
    ReadyFoldersNewChatsJoin,
    /// Folder follow-ups: folder limit box with the Premium upsell.
    ReadyFoldersLimit,
    /// Folder follow-ups: remove a shared folder and choose chats to leave.
    ReadyFoldersDelete,
    /// Parity slice: chat-list avatars (injected, no live Telegram) — the
    /// chat list mixes photo avatars (private chat A, the demo channel)
    /// and colored-initial fallbacks (private chat B, a basic group, the
    /// discussion supergroup); the demo channel (id 13) is open with its
    /// header photo, @username, subscriber count, description snippet, and
    /// a "Discuss" link to the injected discussion group (id 16).
    ReadyChatAvatars,
    /// Notification settings demo (injected, no live Telegram): the open
    /// chat has a custom notification sound (`getSavedNotificationSounds`
    /// fixture) and the per-chat notifications panel is open with the sound
    /// picker expanded (parity slice: notification sounds).
    ReadyNotificationSound,
    /// Notification settings (parity cluster notify-os): the defaults
    /// dialog with the flash/bounce switch, the Events section and the
    /// folder-counter switch (injected, English fixtures).
    ReadyNotifyOs,
    /// Folder tabs with unread-chat counters, one of them muted-only
    /// (parity cluster notify-os, "Include muted chats in folder counters").
    ReadyFolderBadges,
    /// Phase A1: slow-mode enforcement (injected, no live Telegram) — a
    /// dedicated supergroup (id 17) with `slow_mode_delay: 30` and
    /// `slow_mode_delay_expires_in: 25.0`, the viewer a plain member (no
    /// bypass), opened with two messages. The composer shows the
    /// "Slow mode · wait Ns" countdown and blocks sends until expiry.
    ReadySlowMode,
    /// Phase B1: secret chat lifecycle (injected, no live Telegram) — a
    /// Ready secret chat (id 41) with Zed (user 41): `updateSecretChat`
    /// (Ready) → `updateNewChat` (`chatTypeSecret`) → history, opened
    /// with three E2E messages. The chat-list row shows the 🔒 badge and
    /// the composer is live (a Pending chat would show "Waiting for Zed
    /// to come online…" and a Closed chat "Secret chat closed" instead).
    ReadySecretChat,
    /// Slice P1 payments demo (injected, no live Telegram): a bot chat
    /// with a `messageInvoice` (Buy button), a `messagePaymentSuccessful`
    /// row, a paid invoice with a receipt link, and a seeded
    /// `paymentForm` with the checkout dialog open (regular provider,
    /// order fields, a saved credential, terms).
    ReadyPayments,
    /// Slice `parity:bots-payment-recurring`: the ⭐ Subscriptions dialog
    /// open over the ReadyChats fixture — fixture `starSubscriptions`
    /// (active channel, canceled bot, expired channel rows), dialog open
    /// (injected, no live Telegram).
    ReadySubscriptions,
    /// Stars balance + transaction history dialog (fixture, no purchases).
    ReadyStars,
    /// Received gifts grid with the details of one gift (fixture).
    ReadyGifts,
    /// Read-only Premium features explainer (fixture).
    ReadyPremium,
    /// Gift/giveaway cards in a chat (fixture).
    ReadyGiftCards,
    ReadyMarketplaceGift,
    /// Phase B2: key verification UI (injected, no live Telegram) — the
    /// same Ready secret chat as `ReadySecretChat` but with a real
    /// 36-byte `key_hash` (deterministic fixture), and the partner's
    /// info panel open showing the "Encryption key" 12×12 fingerprint
    /// grid plus the verification copy.
    ReadyKeyVerification,
    /// Phase S1: "New secret chat" contact picker (injected, no live
    /// Telegram) — the Ready secret chat fixture plus the contacts
    /// fixture, with the sidebar "🔒 New secret chat" picker open.
    /// `session.contacts` is assigned directly (the fixture equivalent
    /// of a `getContacts` answer) so the picker has eligible rows.
    ReadySecretPicker,
    /// Phase S2: inline-bot warning banner (injected, no live Telegram)
    /// — the Ready secret chat fixture with a pending
    /// `SwitchInline` alert above the composer.
    ReadySecretBotAlert,
    /// Phase S2: storage-usage overlay (injected, no live Telegram) —
    /// fixture `getStorageStatistics` stats with the "Secret media and
    /// files" category, dialog open.
    ReadyStorageUsage,
    /// Settings → Appearance: the Appearance dialog open over the
    /// ReadyChats fixture (injected, no live Telegram).
    ReadyAppearance,
    /// Appearance slice: Appearance with Telegram wallpapers and interface scale.
    ReadyAppearanceWallpapers,
    /// Per-chat theme and wallpaper picker open over a private chat.
    ReadyChatLook,
    /// A private chat wearing an emoji theme and its own pattern wallpaper.
    ReadyChatTheme,
    /// A `bg/` link preview (pattern wallpaper).
    ReadyBackgroundLink,
    /// Ready chat draft with typos underlined (red wavy).
    ReadySpellcheck,
    /// Multi-line draft: typos underlined; link, mention, hashtag, command and code skipped.
    ReadySpellcheckPanel,
    /// Appearance dialog with the Spelling / Check spelling row visible.
    ReadySpellcheckToggle,
    /// Slice `parity:platform-custom-keybindings`: the Appearance dialog
    /// open on the Keyboard shortcuts section (injected, no live Telegram).
    ReadyKeybindings,
    /// Slice parity:auth-multi-account (UI): the Accounts dialog open
    /// over the ReadyChats fixture (injected, no live Telegram). The
    /// account list reads the real local registry (read-only).
    ReadyAccounts,
    /// Slice A3: Active Sessions overlay (injected, no live Telegram) —
    /// fixture `getActiveSessions` sessions (current device + two other
    /// sessions + one incomplete login attempt), dialog open.
    ReadySessions,
    /// Slice A4: Connected Websites overlay (injected, no live Telegram)
    /// — fixture `getConnectedWebsites` websites, dialog open.
    ReadyWebSessions,
    ReadySessionToggles,
    /// Phase B3: self-destructing media (injected, no live Telegram) —
    /// a Ready *private* (1:1 cloud) chat with Zed: an incoming photo
    /// with a live 60s `messageSelfDestructTypeTimer` countdown, an
    /// outgoing `messageSelfDestructTypeImmediately` ("view once")
    /// photo, and a pending photo attachment with the composer's timer
    /// picker on 30s. Private chat — not a secret chat — because TDLib
    /// only accepts per-media `self_destruct_type` in
    /// `chatTypePrivate` chats (schema 1.8.67 lines 6117/6128,
    /// "private chats only").
    ReadySelfDestruct,
    /// Phase C1: call signaling UI (injected, no live Telegram) — an
    /// incoming `callStatePending` voice call from Zed, so the overlay
    /// renders the ringing incoming-call card (Accept / Decline, clock
    /// ticking). Signaling only: the card carries the honest
    /// no-audio-transport note.
    ReadyCall,
    /// Swap prompt: an active outgoing voice call with Zed plus an
    /// incoming pending video call from Ada, so the state machine
    /// raises the swap prompt and the kit dialog renders ("End &
    /// answer" / "Decline"). Injected, no live Telegram.
    ReadyCallSwap,
    /// Phase C1b: a *connected* (`callStateReady`) incoming video call
    /// from Zed, so the overlay renders the video-stage placeholder
    /// grid (remote + local tiles), the 📹 "Video call" kind line, the
    /// duration clock, Mute / Hang up, and the honest no-video-transport
    /// note. Signaling only — no live Telegram, no media.
    ReadyCallVideo,
    /// Phase C2i: connected video call with the 1:1 screen-share send
    /// toggle engaged — the overlay shows "Sharing your screen", the
    /// "Stop sharing" button, and the local tile's screen-share
    /// status. Injected demo state, no live Telegram, no real
    /// capture.
    ReadyCallScreenShare,
    /// Phase C2l: connected video call while the *peer* shares their
    /// screen — the overlay renders the screen-share tile as the main
    /// video-stage tile with the badge, the local camera as the PiP.
    /// Injected demo state, no live Telegram, no real frames.
    ReadyCallScreenShareReceive,
    /// Phase C2c: connected voice call with microphone/speaker choices.
    ReadyCallDevices,
    /// Phase C2d: Ready voice call while the audio driver reconnects.
    ReadyCallReconnecting,
    /// Phase B4: chat-level auto-delete / self-destruct timer (injected,
    /// no live Telegram) — the Ready secret chat with Zed (id 41) with
    /// `message_auto_delete_time` 3600, one message carrying a live
    /// `auto_delete_in` countdown, a
    /// `messageChatSetMessageAutoDeleteTime` service row, and the timer
    /// picker expanded under the header.
    ReadyChatTtl,
    /// Notifications and mute: the open chat's Mute submenu with the
    /// Custom duration row expanded, Unmute and Disable sound (injected,
    /// no live Telegram).
    ReadyMuteCustom,
    /// Auto-delete in a regular chat: the header menu's picker with the
    /// Custom stepper expanded (injected, no live Telegram).
    ReadyAutoDelete,
    /// Phase C3a: a joined group voice chat (injected, no live
    /// Telegram) — the overlay renders the title, participant grid
    /// (speaking / muted / hand-raised badges), E2E verification
    /// emojis, self controls, admin controls, and the always-visible
    /// honest audio-state note (Phase C2g: transport-aware).
    ReadyGroupCall,
    /// Phase C2f: group-call invite picker — the Ready voice chat with
    /// the invite panel open (two contacts seeded), per-participant
    /// volume steppers and owner-gated Ban buttons (injected, no live
    /// Telegram).
    ReadyGroupCallInvite,
    /// Phase C2f: incoming `messageGroupCall` invitation in a Ready
    /// group chat's history, with Accept / Decline (injected, no live
    /// Telegram).
    ReadyGroupCallInvitation,
    /// Phase C2h: group-call management surface (injected, no live
    /// Telegram) — the Ready voice chat with the rename dialog's
    /// sibling state: invite link fetched (Copy/Revoke), recording
    /// indicator live, RTMP URL + key fetched, and two in-call chat
    /// messages with the composer.
    ReadyGroupCallManage,
    /// Calls polish: a pinned video tile plus paused camera and screen
    /// streams in a joined voice chat (injected, no live Telegram).
    ReadyGroupCallPolish,
    /// Calls polish: the "Join as" picker on an unjoined voice chat
    /// (injected, no live Telegram).
    ReadyGroupCallJoinAs,
    /// "Notify me when a scheduled video chat starts" (injected, no
    /// live Telegram) — a scheduled (not yet started) video chat, so
    /// the overlay renders the "Scheduled voice chat" card with the
    /// "Starts in …" line, the admin "Start now" button, and the
    /// "Notify me when it starts" toggle
    /// (`toggleVideoChatEnabledStartNotification`, :14282).
    ReadyGroupCallScheduled,
    /// Phase C2i: Recent-calls tab — server-side `searchCallMessages`
    /// history (missed / declined / answered) + call settings
    /// (injected, no live Telegram).
    ReadyCallsSettings,
    /// Slice S3: Privacy overlay (Settings → Privacy) — five rules,
    /// read-date setting, blocked list (injected, no live Telegram).
    ReadyPrivacy,
    /// B13: the Gifts privacy editor — who can show gifts, the gift icon
    /// switch and the accepted gift types (injected, no live Telegram).
    ReadyPrivacyGifts,
    /// Who can call me: the editor with its Always/Never allow lists
    /// (injected, no live Telegram).
    ReadyPrivacyCalls,
    /// Settings with the help rows and the version footer (injected, no
    /// live Telegram).
    ReadySettingsHelp,
    /// Settings > Ask a Question (injected, no live Telegram).
    ReadyAskQuestion,
    /// B13: the session details view (application, system, IP address,
    /// location) with Terminate (injected, no live Telegram).
    ReadySessionDetails,
    /// B13: the warning before opening an executable file.
    ReadyFileOpenConfirm,
    /// Slice A2: two-step verification overlay — password set with
    /// recovery email (injected `passwordState`, no live Telegram).
    Ready2faManage,
    /// Slice A2: two-step verification overlay — recovery email pending
    /// confirmation (injected `passwordState` with
    /// `recovery_email_address_code_info`, no live Telegram).
    ReadyRecoveryEmail,
    /// Batch 4/6: New-login alert strip over the chat list (injected unconfirmed session, no live Telegram).
    ReadyNewLogin,
    /// Batch 4/6: "New Login Prevented" box after "No, it's not me!" (injected, no live Telegram).
    ReadyLoginPrevented,
    /// Batch 4/6: Server service notification popup (injected, no live Telegram).
    ReadyServiceNotice,
    /// Batch 4/6: Terms of Service prompt with the age check (injected, no live Telegram).
    ReadyTerms,
    /// Batch 4/6: Local storage: ticked types, clear confirmation and limits (injected, no live Telegram).
    ReadyLocalStorage,
    /// Batch 4/6: Two-step "Forgot password?" code step (injected, no live Telegram).
    Ready2faForgot,
    /// Batch 4/6: Two-step password reset waiting period (injected, no live Telegram).
    Ready2faReset,
    /// Batch 4/6: Login email code step (injected, no live Telegram).
    ReadyLoginEmail,
    /// Slice A9: account lifecycle dialog — injected `accountTtl` (180
    /// days) + `passwordState` with a password set, dialog open (no live
    /// Telegram).
    ReadyAccountLifecycle,
    /// M2: rich message demo (injected, no live Telegram) — the demo bot
    /// chat with an injected `messageRichMessage` (headings, styled
    /// paragraphs, list, collapsible, inline document, table, divider,
    /// `pageBlockButtonRow` with URL + callback buttons) plus a message
    /// whose `ephemeral_content` overrides the regular content.
    ReadyRichMessage,
    /// M2: rich editor demo (injected, no live Telegram) — the demo bot
    /// chat with the composer in rich mode (markup text, block buttons,
    /// live block preview).
    ReadyRichEditor,
    /// Slice msg-richtext-ai-tools: the same rich editor with a short
    /// draft so the AI ghost buttons (✨ Fix / Rewrite / Create / Fix rich /
    /// Rewrite rich) sit in frame. Injected Ready session, no live Telegram.
    ReadyRichAiTools,
    /// Slice msg-richtext-premium-gate: multi-line composer so the ⛶ Rich
    /// editor button is visible, status note shows the non-Premium refusal
    /// ("Rich messages require Telegram Premium"). Editor stays closed.
    /// Injected Ready session, no live Telegram.
    ReadyRichPremiumGate,
    /// Slice A5: profile management (injected, no live Telegram) — the
    /// "Edit profile" dialog open on the current user (id 777) with a
    /// seeded name, bio, usernames and profile-photo id.
    ReadyProfileEdit,
    /// B10: profile and contact panels (`QUILL_DEMO_PROFILE=contact|self|
    /// edit-contact|birthday|channel|share|gallery|similar`; injected data,
    /// no live Telegram).
    ReadyProfilePanels,
    /// Member moderation (`QUILL_DEMO_MODERATION=members|restrict|ban|
    /// remove|delete|leave|pick|confirm|blocked`; injected data, no live
    /// Telegram).
    ReadyMemberModeration,
    /// B7: group and channel settings dialog
    /// (`QUILL_DEMO_GROUP_ADMIN=group|channel|basic|reactions|discussion|
    /// linked|confirm`; injected data, no live Telegram).
    ReadyGroupAdminSettings,
    /// Slice A5: like `ReadyProfileEdit`, but the username field holds a
    /// freshly-checked value with a seeded `checkChatUsernameResultOk`
    /// verdict — intended for a taller capture
    /// (`QUILL_DEMO_WINDOW_SIZE`) so the username section is visible.
    ReadyUsername,
    /// Slice parity:platform-shortcuts-reference: the keyboard shortcuts
    /// reference dialog open over the demo chat list (injected, no live
    /// Telegram).
    ReadyShortcuts,
    /// `parity:proxy-settings`: proxy list / editor / link confirmation
    /// (`QUILL_DEMO_PROXY=list|edit|link|link-bad`; injected data, no live
    /// Telegram, no real proxy).
    ReadyProxy,
    /// Scheduled messages; `QUILL_DEMO_SCHEDULED=button|picker|list|list-selected|reminder|reminder-list`
    /// (composer button, date+time picker, list with Send now / Reschedule,
    /// Saved Messages reminder wording; injected, no live Telegram).
    ReadyScheduled,
    /// Find in history: the "Jump to date" calendar box.
    ReadyJumpDate,
    /// Find in history: the in-chat "From:" member picker.
    ReadySearchFrom,
    /// Find in history: a chosen member's messages with "N of M".
    ReadySearchFromHits,
    /// Find in history: global search narrowed by the filter bar.
    ReadySearchFilters,
    /// Search upgrades: the empty search with Frequent contacts + Recent.
    ReadySearchFrequent,
    /// Search upgrades: a hashtag in the Public posts scope.
    ReadySearchPublic,
    /// Local passcode: the settings dialog with a passcode set (auto-lock,
    /// Touch ID rows).
    ReadyPasscodeSettings,
    /// Local passcode: the create form with a mismatch error.
    ReadyPasscodeCreate,
    /// Local passcode: the lock screen after a wrong passcode.
    ReadyLockScreen,
    /// Avatar click in a group (injected, no live Telegram): a member's
    /// profile open as the modal layer over the group history, as after
    /// clicking the avatar next to their message.
    ReadyAvatarProfile,
}
