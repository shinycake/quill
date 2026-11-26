// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL: the snapshot textures of one renderer, shared by the Metal, wgpu
//! and DirectX 11 backends (docs/decisions/snapshot-transitions.md).
//!
//! Each live slot owns one texture, its sides rounded up to 256 and at
//! most the backend's limit. A released texture is kept for the next
//! capture while another snapshot lives, and freed with the last one. What
//! the renderer could not draw is recorded here, as [`SnapshotLosses`], for
//! the window to take at its next frame.

use std::collections::HashMap;

use super::SnapshotSlot;
use crate::{DevicePixels, Size};

/// The largest side any backend gives a snapshot texture.
pub const SNAPSHOT_MAX_SIDE: u32 = 16384;

/// What a renderer lost since the window last took its losses
/// (`PlatformWindow::take_snapshot_losses`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SnapshotLosses {
    /// Nothing.
    #[default]
    None,
    /// These snapshots could not be drawn; the rest are whole. Their owners
    /// paint live: capturing the same snapshot again would fail again.
    Slots(Vec<SnapshotSlot>),
    /// Every texture went (the device was lost or replaced); capturing
    /// again draws them on the new one.
    All,
}

impl SnapshotLosses {
    /// `slot` lost besides.
    pub fn add_slot(&mut self, slot: SnapshotSlot) {
        match self {
            Self::None => *self = Self::Slots(vec![slot]),
            Self::Slots(slots) => slots.push(slot),
            Self::All => {}
        }
    }

    /// Both losses.
    pub fn merge(&mut self, other: Self) {
        match other {
            Self::None => {}
            Self::All => *self = Self::All,
            Self::Slots(slots) => {
                for slot in slots {
                    self.add_slot(slot);
                }
            }
        }
    }
}

/// The snapshot textures of one renderer, `T` being the backend's texture
/// (with whatever views it keeps).
pub struct SnapshotPool<T> {
    textures: HashMap<SnapshotSlot, T>,
    /// Released textures kept for the next capture, with their sizes.
    spare: Vec<(u32, u32, T)>,
    /// The sizes of the live textures.
    sizes: HashMap<SnapshotSlot, (u32, u32)>,
    losses: SnapshotLosses,
    max_side: u32,
}

impl<T> Default for SnapshotPool<T> {
    fn default() -> Self {
        Self::new(SNAPSHOT_MAX_SIDE)
    }
}

impl<T> SnapshotPool<T> {
    /// A pool whose textures are at most `max_side` texels a side (the
    /// backend's limit, never past [`SNAPSHOT_MAX_SIDE`]).
    pub fn new(max_side: u32) -> Self {
        Self {
            textures: HashMap::new(),
            spare: Vec::new(),
            sizes: HashMap::new(),
            losses: SnapshotLosses::None,
            max_side: max_side.min(SNAPSHOT_MAX_SIDE),
        }
    }

    /// The largest side a snapshot may have
    /// (`PlatformWindow::snapshot_max_side`).
    pub fn max_side(&self) -> u32 {
        self.max_side
    }

    /// The texture size for a snapshot of `texels`: each side rounded up to
    /// 256, `None` when a side is empty or past the limit.
    pub fn rounded(&self, texels: Size<DevicePixels>) -> Option<(u32, u32)> {
        let side = |side: i32| {
            let side = u32::try_from(side).ok().filter(|side| *side > 0)?;
            Some(side.next_multiple_of(256).min(self.max_side)).filter(|rounded| *rounded >= side)
        };
        Some((side(texels.width.0)?, side(texels.height.0)?))
    }

    /// The texture `slot` draws into, if it has one.
    pub fn get(&self, slot: SnapshotSlot) -> Option<&T> {
        self.textures.get(&slot)
    }

    /// The size of the texture `slot` draws into, if it has one.
    pub fn size_of(&self, slot: SnapshotSlot) -> Option<(u32, u32)> {
        self.sizes.get(&slot).copied()
    }

    /// Whether no snapshot holds a texture.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Returns the textures of `slots`, whose handles dropped, to the spare
    /// list; frees every spare once no snapshot is left.
    pub fn release(&mut self, slots: &[SnapshotSlot]) {
        for slot in slots {
            self.retire(*slot);
        }
        if self.textures.is_empty() {
            self.spare.clear();
        }
    }

    fn retire(&mut self, slot: SnapshotSlot) {
        if let (Some(texture), Some((width, height))) =
            (self.textures.remove(&slot), self.sizes.remove(&slot))
        {
            self.spare.push((width, height, texture));
        }
    }

    /// The texture a snapshot of `texels` in `slot` draws into: its own
    /// when the size matches and `fits` accepts it, else a spare that does,
    /// else one `make` builds at the rounded size.
    pub fn target(
        &mut self,
        slot: SnapshotSlot,
        texels: Size<DevicePixels>,
        fits: impl Fn(&T) -> bool,
        make: impl FnOnce(u32, u32) -> anyhow::Result<T>,
    ) -> anyhow::Result<&T> {
        let Some((width, height)) = self.rounded(texels) else {
            anyhow::bail!("a snapshot of {texels:?} texels");
        };
        let own = self.sizes.get(&slot) == Some(&(width, height))
            && self.textures.get(&slot).is_some_and(&fits);
        if !own {
            self.retire(slot);
            let spare = self
                .spare
                .iter()
                .position(|(w, h, texture)| (*w, *h) == (width, height) && fits(texture));
            let texture = match spare {
                Some(index) => self.spare.swap_remove(index).2,
                None => make(width, height)?,
            };
            self.textures.insert(slot, texture);
            self.sizes.insert(slot, (width, height));
        }
        self.textures
            .get(&slot)
            .ok_or_else(|| anyhow::anyhow!("snapshot texture missing"))
    }

    /// `slot` could not be drawn: its texture goes, and the window learns it.
    pub fn lose(&mut self, slot: SnapshotSlot) {
        self.textures.remove(&slot);
        self.sizes.remove(&slot);
        self.losses.add_slot(slot);
    }

    /// Every texture went with the device: the pool empties, and the window
    /// learns every snapshot is lost.
    pub fn lose_all(&mut self) {
        self.textures.clear();
        self.sizes.clear();
        self.spare.clear();
        self.losses = SnapshotLosses::All;
    }

    /// What was lost since the last call.
    pub fn take_losses(&mut self) -> SnapshotLosses {
        std::mem::take(&mut self.losses)
    }
}
