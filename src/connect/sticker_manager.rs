//! Connect driver: the stickers manager box. Its thumbnails download on
//! their own, and its changes (removals, then the new order) are sent when
//! the box closes, one request after the other.
use super::*;
use crate::ids::RequestId;
use crate::state::{CommitStep, ManagerCommit, RequestPurpose, StickersPurpose};
use crate::telegram::requests::{reorder_installed_mask_sets, reorder_installed_sticker_sets};

impl<S: JsonSender> ConnectDriver<S> {
    /// The manager box opened or closed.
    pub fn set_sticker_manager_open(&mut self, open: bool) {
        self.session.stickers.manager.open = open;
        if !open {
            self.session.stickers.manager.covers_requested.clear();
        }
    }

    /// Download the first sticker of every listed set, once each.
    fn ensure_manager_covers(&mut self) -> Result<(), ConnectSendError> {
        let stickers = &self.session.stickers;
        let mut asked = Vec::new();
        let mut files = Vec::new();
        for set in stickers
            .stickers
            .sets
            .iter()
            .chain(&stickers.stickers.trending)
            .chain(&stickers.stickers.archived)
            .chain(&stickers.masks.installed)
            .chain(&stickers.masks.archived)
        {
            if stickers.manager.covers_requested.contains(&set.id) || asked.contains(&set.id) {
                continue;
            }
            asked.push(set.id);
            files.extend(
                set.covers
                    .iter()
                    .filter_map(|cover| cover.display_file_id()),
            );
        }
        self.session.stickers.manager.covers_requested.extend(asked);
        self.ensure_media_files(&files)
    }

    /// The trending sets on screen were seen: tell TDLib, and drop their
    /// unread dots.
    pub fn view_shown_trending_stickers(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let ids: Vec<_> = self
            .session
            .stickers
            .stickers
            .trending
            .iter_mut()
            .filter(|set| !set.is_viewed)
            .map(|set| {
                set.is_viewed = true;
                set.id
            })
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        self.sticker_request(RequestPurpose::ViewTrendingStickerSets, |id| {
            crate::telegram::requests::view_trending_sticker_sets(id, &ids)
        })
    }

    /// Queue what a closed box changed: the sets to remove and the order of
    /// the rest. Nothing is sent for an untouched box.
    pub fn commit_sticker_manager(
        &mut self,
        masks: bool,
        removed: Vec<i64>,
        order: Option<Vec<i64>>,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if removed.is_empty() && order.is_none() {
            return Ok(());
        }
        let order = order.map(|ids| ids.into_iter().filter(|id| !removed.contains(id)).collect());
        self.session
            .stickers
            .manager
            .commits
            .push_back(ManagerCommit {
                masks,
                removals: removed.into_iter().filter(|id| *id > 0).collect(),
                order,
            });
        self.advance_manager_commit()
    }

    fn manager_change_pending(&self, masks: bool) -> bool {
        if masks {
            return self.session.mask_change_pending();
        }
        self.session.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
                    | RequestPurpose::ReorderInstalledStickerSets
            )
        })
    }

    /// Send the next step of the oldest saved change once the one before it
    /// has been answered, whether it worked or not.
    pub(crate) fn advance_manager_commit(&mut self) -> Result<(), ConnectSendError> {
        loop {
            let Some(front) = self.session.stickers.manager.commits.front() else {
                return Ok(());
            };
            let masks = front.masks;
            if self.manager_change_pending(masks) {
                return Ok(());
            }
            let step = self
                .session
                .stickers
                .manager
                .commits
                .front_mut()
                .map_or(CommitStep::Done, ManagerCommit::next_step);
            let sent = match step {
                CommitStep::Done => {
                    self.session.stickers.manager.commits.pop_front();
                    continue;
                }
                CommitStep::Remove(id) if masks => self.change_mask_set(id, false, false),
                CommitStep::Remove(id) => self.manage_sticker_set(id, false, false),
                CommitStep::Reorder(ids) if masks => self
                    .mask_request(StickersPurpose::ReorderInstalledMaskSets.into(), |id| {
                        reorder_installed_mask_sets(id, &ids)
                    }),
                CommitStep::Reorder(ids) => self
                    .sticker_request(RequestPurpose::ReorderInstalledStickerSets, |id| {
                        reorder_installed_sticker_sets(id, &ids)
                    }),
            };
            return match sent {
                Ok(Some(_)) => Ok(()),
                // Nothing went out (the same change was already pending):
                // go on with the next step.
                Ok(None) => continue,
                Err(err) => {
                    self.session.stickers.manager.commits.clear();
                    Err(err)
                }
            };
        }
    }

    /// Called after every answer: thumbnails and the saved changes.
    pub(crate) fn pump_sticker_manager(&mut self) {
        let _ = self.advance_manager_commit();
        if self.session.stickers.manager.open {
            let _ = self.ensure_manager_covers();
        }
    }
}
