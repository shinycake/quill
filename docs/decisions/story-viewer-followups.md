# Story viewer follow-ups: retries, the walk, the live header, preloads

Five small fixes left over from the reviews of #937, #947 and #950.

## A card or reply story that failed once

**tdesktop:** `Stories::sendResolveRequests` resolves the stories that cards and reply headers show. When a request fails, `finalizeResolve` marks the story deleted, so the card stops waiting (`data/data_stories.cpp:842-888`).

**Quill before:** since #950, a failure that may pass (a 500, a timeout or FLOOD_WAIT) leaves the story retryable behind a backoff. But the card and reply prefetch (`maybe_fetch_replied_messages`) marked a story as attempted before `get_story` ran. That caused two problems:
- After a 500, the card stayed on Loading for the rest of the session.
- If the backoff held the request back, so nothing was sent, the card still never asked.

**Now:**
- A story counts as attempted only when its `getStory` actually went out.
- A failure that may pass takes the story off the attempted list again.
- Card candidates leave out stories known to be gone and stories still waiting, so the card asks again once the wait is over.

## A new Ready keeps the per-story waits

**Quill before:** a reconnect cleared the tray's retry clock but not the per-story waits. Neighbours that failed while offline stayed held back for up to 5 minutes.

**Now:** `reset_active_stories_paging` also clears `story_retry`.

## A neighbour waiting out a failed fetch blocked the walk

**tdesktop:** a failed lookup ends as deleted (`finalizeResolve`, `data_stories.cpp:842-888`), so nothing waits on that story. Quill keeps it retryable, so it is passed over only while it waits.

**Quill before:** `story_sibling_key` counted every Loading story as available. A neighbour whose fetch failed and was waiting therefore made Next at the end of a poster close the viewer.

**Now:** while its wait runs, that neighbour is passed over like a gone one. Once the wait ends it counts again.

## The live header behind a Picture-in-Picture stream

**tdesktop:** `Header::setVideoStreamViewers` (`media/stories/media_stories_header.cpp:544-572`) applies to the story the viewer shows. A stream in PiP plays in its own window.

**Quill before:** a live story kept in PiP stays `stories.live.watching` while the viewer shows another story. The other story then got the LIVE badge, the viewer count, the sound toggle and the comment field.

**Now:** these apply only while the watched stream is the current viewer story (`live_story_shown`).

## An automatic download over a story preload

**tdesktop:** each preload owns its requests and cancels only those. `VideoPreload::~VideoPreload` runs `cancelAllRequests`, and `PhotoPreload` cancels its own media view (`data/data_media_preload.cpp:54-56, 147-151`). The media's own download is never cancelled by a preload.

**Quill before:** the download intent records one origin per file. An automatic download (`download_file`) that found a running or ended preload sent nothing. The prefix limit stayed, and dropping the preload later cancelled the file the automatic path wanted.

**Now:** an automatic download over a preload sends `downloadFile` for the whole file at its own priority. TDLib replaces the running request's limit and priority, and the intent turns automatic, so dropping the preload no longer cancels it.
