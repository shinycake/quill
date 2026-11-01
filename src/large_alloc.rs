//! Large heap blocks on macOS come from the VM system and go back to it.
//!
//! libmalloc on current macOS keeps freed blocks of a few KB and up
//! (decoded photos, sticker frames, decode intermediates, file reads)
//! charged to the process: neither `malloc_zone_pressure_relief` nor the
//! memory-pressure handler gives the pages back, and they leave the
//! footprint only when the kernel happens to reclaim them. Opening a dozen
//! photos in the media viewer left 80–180 MB of freed memory charged to
//! Quill (measured in `docs/decisions/sweep-r3-browse-memory.md`).
//!
//! [`LargeAlloc`] serves blocks of [`LARGE`] bytes up to 1 GiB from an
//! arena of address space reserved once at the first large allocation.
//! The arena is split into size classes of powers of two (64 KiB to
//! 1 GiB), each with 8 GiB of address space cut into slots of its size.
//! A block takes a whole slot: the slot is mapped readable and writable
//! when the block is allocated and replaced with an inaccessible mapping
//! when it is freed, which drops its pages at once. Pages never touched
//! cost nothing, so a 70 KiB block in a 128 KiB slot is charged 80 KiB.
//!
//! Why this shape:
//! - **Ownership by address.** `dealloc` and `realloc` decide whether a
//!   block is the arena's from the pointer alone (one range check), and
//!   find its slot, size class and length from the address. The `Layout`
//!   the caller passes is never trusted for that, so a caller that frees
//!   with the wrong size (coreaudio-rs frees its input buffers with the
//!   last byte count, not their capacity) still frees the right thing, and
//!   a `malloc` block handed to Rust's `dealloc` still goes to `free`.
//! - **No locks.** Slots are claimed and released with atomic bit
//!   operations on fixed static bitmaps: nothing allocates, nothing waits,
//!   so a `fork` or a signal at any point cannot leave a lock held.
//! - **Graceful fallback.** When the reservation, a slot or a mapping is
//!   not available, the block comes from the system allocator instead;
//!   the arena never panics and never returns null while `System` can
//!   still serve the request.
//!
//! Smaller blocks, blocks over 1 GiB and alignments above 64 KiB stay with
//! the system allocator. TDLib and the other C and C++ libraries call
//! `malloc` themselves and never see the arena; no Quill code hands a
//! Rust allocation to C's `free` (audited for this change).
//!
//! Linux (glibc, `malloc_trim` at idle) and Windows (the heap decommits
//! large blocks on free) give large blocks back on their own.
#![cfg(target_os = "macos")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// The smallest arena block, as a power of two: 64 KiB. Below it the
/// rounding to whole pages would waste too much (a 56 px emoji frame is
/// 12 KiB; a 224 px sticker frame, 196 KiB, is served here).
const MIN_SHIFT: usize = 16;
/// Blocks this large and up are served from the arena.
pub const LARGE: usize = 1 << MIN_SHIFT;
/// The largest arena block, as a power of two: 1 GiB.
const MAX_SHIFT: usize = 30;
const CLASSES: usize = MAX_SHIFT - MIN_SHIFT + 1;
/// Address space per size class: 8 GiB (131072 slots of 64 KiB, eight of
/// 1 GiB). Only address space: it costs no memory until a block uses it.
const CLASS_SPAN: usize = 1 << 33;
const ARENA_BYTES: usize = CLASSES * CLASS_SPAN;
/// Rounding for dropping the unused tail of a block shrunk in place: the
/// arm64 page, and a multiple of the x86 one.
const PAGE: usize = 16 << 10;
/// `LargeAlloc::base` before the reservation was tried.
const UNRESERVED: usize = 0;
/// `LargeAlloc::base` once the reservation failed: everything goes to
/// `System`.
const UNAVAILABLE: usize = 1;

/// The process allocator on macOS.
pub struct LargeAlloc {
    /// The arena's first byte (aligned to `LARGE`), or `UNRESERVED` /
    /// `UNAVAILABLE`.
    base: AtomicUsize,
    /// One bit per slot, set while the slot holds a block.
    used: [AtomicU64; first_word(CLASSES)],
}

#[global_allocator]
static GLOBAL: LargeAlloc = LargeAlloc::new();

/// Slots in size class `class`.
const fn slots(class: usize) -> usize {
    CLASS_SPAN >> (MIN_SHIFT + class)
}

/// Bitmap words for size class `class` (one bit per slot).
const fn words(class: usize) -> usize {
    slots(class).div_ceil(64)
}

/// Index of size class `class`'s first word in `LargeAlloc::used`.
const fn first_word(class: usize) -> usize {
    let mut word = 0;
    let mut earlier = 0;
    while earlier < class {
        word += words(earlier);
        earlier += 1;
    }
    word
}

/// The size class serving `size` bytes, if the arena serves that size.
fn class_of(size: usize) -> Option<usize> {
    if size < LARGE {
        return None;
    }
    let shift = size.checked_next_power_of_two()?.trailing_zeros() as usize;
    shift
        .checked_sub(MIN_SHIFT)
        .filter(|class| *class < CLASSES)
}

/// Whether the arena serves `layout`: every slot starts on a multiple of
/// `LARGE`, so alignments up to that are met.
fn served(layout: Layout) -> Option<usize> {
    if layout.align() > LARGE {
        return None;
    }
    class_of(layout.size())
}

fn slot_len(class: usize) -> usize {
    1 << (MIN_SHIFT + class)
}

/// An arena slot: where it starts and which bit tracks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    class: usize,
    index: usize,
}

impl Slot {
    fn addr(self, base: usize) -> usize {
        base + self.class * CLASS_SPAN + (self.index << (MIN_SHIFT + self.class))
    }

    fn len(self) -> usize {
        slot_len(self.class)
    }

    fn mask(self) -> u64 {
        1 << (self.index % 64)
    }

    /// The slot that starts at `addr`, if `addr` is in the arena. `None`
    /// means the block is not the arena's.
    fn containing(base: usize, addr: usize) -> Option<(Slot, bool)> {
        let offset = addr.checked_sub(base).filter(|o| *o < ARENA_BYTES)?;
        let class = offset / CLASS_SPAN;
        let within = offset % CLASS_SPAN;
        let slot = Slot {
            class,
            index: within >> (MIN_SHIFT + class),
        };
        let at_start = within & (slot.len() - 1) == 0;
        Some((slot, at_start))
    }
}

impl LargeAlloc {
    const fn new() -> Self {
        Self {
            base: AtomicUsize::new(UNRESERVED),
            used: [const { AtomicU64::new(0) }; first_word(CLASSES)],
        }
    }

    /// The arena's base address, reserving it on first use. `None` when the
    /// reservation failed (now or before).
    fn arena(&self) -> Option<usize> {
        match self.base.load(Ordering::Acquire) {
            UNRESERVED => {}
            UNAVAILABLE => return None,
            base => return Some(base),
        }
        let reserved = reserve(ARENA_BYTES + LARGE);
        let value = reserved.map_or(UNAVAILABLE, |raw| raw.next_multiple_of(LARGE));
        match self
            .base
            .compare_exchange(UNRESERVED, value, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => (value != UNAVAILABLE).then_some(value),
            Err(winner) => {
                // Another thread reserved first: give ours back, use theirs.
                if let Some(raw) = reserved {
                    unmap(raw, ARENA_BYTES + LARGE);
                }
                (winner != UNAVAILABLE).then_some(winner)
            }
        }
    }

    /// The arena's base if it was reserved, without reserving it.
    fn arena_if_reserved(&self) -> Option<usize> {
        match self.base.load(Ordering::Acquire) {
            UNRESERVED | UNAVAILABLE => None,
            base => Some(base),
        }
    }

    fn word(&self, slot: Slot) -> Option<&AtomicU64> {
        self.used.get(first_word(slot.class) + slot.index / 64)
    }
}

/// `VM_MEMORY_APPLICATION_SPECIFIC_1` (`mach/vm_statistics.h`) in the
/// form an anonymous mapping takes as its fd: shows as "Memory Tag 240"
/// in vmmap.
const VM_TAG: libc::c_int = 240 << 24;

/// Reserve `len` bytes of inaccessible address space anywhere.
fn reserve(len: usize) -> Option<usize> {
    // SAFETY: an anonymous private mapping at no fixed address; nothing
    // existing is touched.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            VM_TAG,
            0,
        )
    };
    (ptr != libc::MAP_FAILED).then_some(ptr as usize)
}

/// Replace `addr..addr + len`, which lies inside the arena and belongs to
/// the caller, with fresh zero pages of `prot`. The pages it held are
/// dropped. Never unmaps: the range stays reserved, so no other mapping
/// can land in the arena.
///
/// On failure the range keeps its old mapping: XNU's `vm_map_enter` with
/// `VM_FLAGS_OVERWRITE` (what `MAP_FIXED` becomes) restores the entries it
/// removed when the new mapping cannot be made. The callers rely on that.
/// Linux does not promise it (a failed `MAP_FIXED` may leave the range
/// unmapped), which is one reason this module is macOS only.
fn remap(addr: usize, len: usize, prot: libc::c_int) -> bool {
    // SAFETY: `MAP_FIXED` replaces whatever is mapped at `addr..addr+len`.
    // That range is inside the arena reservation and owned by the caller
    // (a slot whose bit it set, or the unused tail of its own block), so
    // nothing anyone else uses is replaced.
    let ptr = unsafe {
        libc::mmap(
            addr as *mut libc::c_void,
            len,
            prot,
            libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_FIXED,
            VM_TAG,
            0,
        )
    };
    ptr as usize == addr
}

fn unmap(addr: usize, len: usize) {
    // SAFETY: `addr..addr + len` is a reservation this thread made and
    // never published (it lost the race to install the arena).
    unsafe {
        libc::munmap(addr as *mut libc::c_void, len);
    }
}

impl LargeAlloc {
    /// Claim a free slot of size class `class`.
    fn claim(&self, class: usize) -> Option<Slot> {
        let count = slots(class);
        for word_index in 0..words(class) {
            let word = self.used.get(first_word(class) + word_index)?;
            // Bits past the class's last slot (classes of under 64 slots).
            let valid = match count.saturating_sub(word_index * 64) {
                n if n >= 64 => u64::MAX,
                n => (1_u64 << n) - 1,
            };
            loop {
                let free = !word.load(Ordering::Relaxed) & valid;
                if free == 0 {
                    break;
                }
                let bit = free.trailing_zeros() as usize;
                let mask = 1_u64 << bit;
                if word.fetch_or(mask, Ordering::AcqRel) & mask == 0 {
                    return Some(Slot {
                        class,
                        index: word_index * 64 + bit,
                    });
                }
            }
        }
        None
    }

    /// Give a slot's bit back. Its mapping must already be inaccessible.
    fn release(&self, slot: Slot) {
        if let Some(word) = self.word(slot) {
            word.fetch_and(!slot.mask(), Ordering::Release);
        }
    }

    /// A block of size class `class` from the arena: fresh zero pages.
    /// `None` when the arena, a slot or the mapping is not available.
    fn arena_alloc(&self, class: usize) -> Option<*mut u8> {
        let base = self.arena()?;
        let slot = self.claim(class)?;
        let addr = slot.addr(base);
        if remap(addr, slot.len(), libc::PROT_READ | libc::PROT_WRITE) {
            Some(addr as *mut u8)
        } else {
            // The slot is still inaccessible (XNU restored its old mapping,
            // see `remap`); nothing to undo but the bit.
            self.release(slot);
            None
        }
    }

    /// Free `slot` (starting at `addr`): drop its pages, then let it be
    /// claimed again.
    fn arena_free(&self, slot: Slot, addr: usize) {
        if remap(addr, slot.len(), libc::PROT_NONE) {
            self.release(slot);
        }
        // Else the pages stay mapped (XNU restored the old mapping, see
        // `remap`) and the slot stays claimed: a leak, never a slot handed
        // out twice.
    }

    /// The arena slot holding `ptr`, if `ptr` is an arena block. `Err(())`
    /// when `ptr` is inside the arena but not at a slot's start: a caller
    /// bug. `dealloc` leaks it; `realloc` returns null, which the caller
    /// treats as allocation failure (Rust's collections abort).
    fn owner(&self, ptr: *mut u8) -> Option<Result<Slot, ()>> {
        let base = self.arena_if_reserved()?;
        let (slot, at_start) = Slot::containing(base, ptr as usize)?;
        Some(if at_start { Ok(slot) } else { Err(()) })
    }
}

// SAFETY: arena blocks are whole slots of at least the requested size,
// aligned to `LARGE` (requests aligned to more go to `System`), mapped
// readable and writable for as long as their slot's bit is set, and zero
// when handed out. Each slot's bit is set by exactly one claimer and
// cleared only after its mapping was replaced, so no two live blocks
// share a slot. Every other request goes to `System` with the caller's
// layout, and a block is returned to `System` only when its address is
// outside the arena, which means `System` allocated it.
unsafe impl GlobalAlloc for LargeAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(ptr) = served(layout).and_then(|class| self.arena_alloc(class)) {
            return ptr;
        }
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // Arena blocks are fresh mappings, which are zero.
        if let Some(ptr) = served(layout).and_then(|class| self.arena_alloc(class)) {
            return ptr;
        }
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match self.owner(ptr) {
            Some(Ok(slot)) => self.arena_free(slot, ptr as usize),
            Some(Err(())) => {}
            // SAFETY: outside the arena, so `System` allocated `ptr`, and
            // the caller passes the layout it was allocated with.
            None => unsafe { System.dealloc(ptr, layout) },
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `new_size`, rounded up to the
        // alignment, does not overflow `isize`, which makes this valid.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new_class = served(new_layout);
        let old = match self.owner(ptr) {
            Some(Ok(slot)) => slot,
            Some(Err(())) => return std::ptr::null_mut(),
            None => {
                // A `System` block. Moves into the arena when it grows
                // past `LARGE`; otherwise stays with `System`.
                if let Some(new) = new_class.and_then(|class| self.arena_alloc(class)) {
                    // SAFETY: `ptr` holds `layout.size()` bytes and `new`
                    // at least `new_size`; they are distinct blocks.
                    unsafe {
                        std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                        System.dealloc(ptr, layout);
                    }
                    return new;
                }
                // SAFETY: `ptr` is `System`'s, with `layout`.
                return unsafe { System.realloc(ptr, layout, new_size) };
            }
        };
        if new_class == Some(old.class) {
            // Same slot. Growing needs nothing (the slot is mapped
            // whole); shrinking drops the pages past the new end. If that
            // remap fails, XNU keeps the old tail mapped (see `remap`) and
            // the pages simply stay.
            let keep = new_size.next_multiple_of(PAGE);
            if let Some(tail) = old.len().checked_sub(keep).filter(|t| *t > 0) {
                remap(
                    ptr as usize + keep,
                    tail,
                    libc::PROT_READ | libc::PROT_WRITE,
                );
            }
            return ptr;
        }
        // SAFETY: `new_layout` is valid (above) and has a non-zero size
        // (`GlobalAlloc::realloc` requires `new_size > 0`).
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            // Never read past the slot, even if `layout` is wrong.
            let copy = layout.size().min(new_size).min(old.len());
            // SAFETY: `ptr` is a live slot of `old.len()` bytes, `new` a
            // distinct block of at least `new_size`.
            unsafe { std::ptr::copy_nonoverlapping(ptr, new, copy) };
            self.arena_free(old, ptr as usize);
        }
        new
    }
}

#[cfg(test)]
#[path = "large_alloc_tests.rs"]
mod tests;
