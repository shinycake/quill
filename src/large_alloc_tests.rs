//! Edge cases of the macOS large-block allocator. The arena is the test
//! binary's global allocator too, so `Vec` exercises it directly.

use super::{
    CLASSES, LARGE, LargeAlloc, MAX_SHIFT, PAGE, Slot, arena, class_of, owner, reserve, served,
    slots,
};
use std::alloc::{GlobalAlloc, Layout};

/// Whether any page of the range is in memory. Darwin's `mincore` reports
/// unmapped and inaccessible pages as not resident rather than failing.
fn resident(ptr: *const u8, len: usize) -> bool {
    const MINCORE_INCORE: i8 = 1;
    // One flag byte per page; 4 KiB is the smallest page size.
    let mut pages = vec![0_i8; len.div_ceil(4096)];
    // SAFETY: `mincore` only reports on the range, writing one byte per
    // page into `pages`, which has room for the smallest page size.
    let ok = unsafe { libc::mincore(ptr as *const libc::c_void, len, pages.as_mut_ptr().cast()) };
    ok == 0 && pages.iter().any(|flags| flags & MINCORE_INCORE != 0)
}

fn in_arena(ptr: *const u8) -> bool {
    matches!(owner(ptr as *mut u8), Some(Ok(_)))
}

fn touch(bytes: &mut [u8]) {
    bytes.iter_mut().step_by(4096).for_each(|b| *b = 1);
}

fn layout(size: usize, align: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap()
}

/// The finding: a freed decoded photo stayed resident (and charged to
/// Quill's footprint) after it was dropped. Now its pages are gone.
#[test]
fn a_freed_full_size_photo_is_unmapped() {
    let len = 2560 * 1920 * 4;
    let mut photo = vec![0_u8; len];
    touch(&mut photo);
    let ptr = photo.as_ptr();
    assert!(in_arena(ptr));
    assert!(resident(ptr, len));
    drop(photo);
    assert!(!resident(ptr, len), "freed photo pages are still resident");
}

#[test]
fn size_classes_cover_64_kib_to_1_gib() {
    assert_eq!(class_of(0), None, "zero-size requests go to System");
    assert_eq!(class_of(LARGE - 1), None);
    assert_eq!(class_of(LARGE), Some(0));
    assert_eq!(class_of(LARGE + 1), Some(1));
    assert_eq!(class_of(1 << MAX_SHIFT), Some(CLASSES - 1));
    assert_eq!(class_of((1 << MAX_SHIFT) + 1), None);
    assert_eq!(class_of(usize::MAX), None, "no overflow at the top");
    assert_eq!(slots(CLASSES - 1), 8);
}

#[test]
fn growing_from_malloc_into_the_arena_keeps_the_contents() {
    let mut bytes: Vec<u8> = (0..LARGE / 2).map(|i| (i % 251) as u8).collect();
    assert!(!in_arena(bytes.as_ptr()));
    bytes.extend((0..LARGE * 2).map(|i| (i % 241) as u8));
    assert!(in_arena(bytes.as_ptr()));
    assert!(
        bytes
            .iter()
            .take(LARGE / 2)
            .enumerate()
            .all(|(i, b)| *b == (i % 251) as u8)
    );
    assert_eq!(bytes.last(), Some(&(((LARGE * 2 - 1) % 241) as u8)));
}

#[test]
fn shrinking_out_of_the_arena_keeps_the_contents_and_frees_the_slot() {
    let mut bytes: Vec<u8> = (0..LARGE * 3).map(|i| (i % 239) as u8).collect();
    let old = bytes.as_ptr();
    assert!(in_arena(old));
    bytes.truncate(1000);
    bytes.shrink_to_fit();
    assert!(!in_arena(bytes.as_ptr()));
    assert!(bytes.iter().enumerate().all(|(i, b)| *b == (i % 239) as u8));
    assert!(!resident(old, LARGE * 3), "the old slot still holds pages");
}

#[test]
fn realloc_within_a_size_class_stays_in_place_and_drops_the_tail() {
    let a = LargeAlloc;
    let start = layout(LARGE + 4096, 8);
    // SAFETY: non-zero layouts; every block is freed with its last layout.
    unsafe {
        let ptr = a.alloc(start);
        assert!(in_arena(ptr));
        std::ptr::write_bytes(ptr, 0xAB, start.size());
        let grown = a.realloc(ptr, start, 2 * LARGE);
        assert_eq!(grown, ptr, "same 128 KiB slot");
        std::ptr::write_bytes(grown.add(start.size()), 0xCD, 2 * LARGE - start.size());
        let bigger = layout(2 * LARGE, 8);
        let shrunk = a.realloc(grown, bigger, LARGE + 100);
        assert_eq!(shrunk, ptr);
        assert_eq!(*shrunk, 0xAB);
        assert_eq!(*shrunk.add(LARGE + 99), 0xAB);
        let tail = (LARGE + 100).next_multiple_of(PAGE);
        assert!(!resident(shrunk.add(tail), 2 * LARGE - tail));
        a.dealloc(shrunk, layout(LARGE + 100, 8));
    }
}

#[test]
fn alignments_up_to_64_kib_come_from_the_arena_and_above_from_system() {
    let a = LargeAlloc;
    for align in [1, 16, 4096, 16 << 10, LARGE] {
        let l = layout(LARGE * 2, align);
        assert!(served(l).is_some());
        // SAFETY: a non-zero layout, freed with the same layout.
        unsafe {
            let ptr = a.alloc(l);
            assert!(in_arena(ptr));
            assert_eq!(ptr as usize % align, 0);
            a.dealloc(ptr, l);
        }
    }
    let l = layout(LARGE * 2, LARGE * 4);
    assert!(served(l).is_none());
    // SAFETY: as above.
    unsafe {
        let ptr = a.alloc(l);
        assert!(!ptr.is_null() && !in_arena(ptr));
        assert_eq!(ptr as usize % (LARGE * 4), 0);
        let grown = a.realloc(ptr, l, LARGE * 8);
        assert!(!in_arena(grown));
        assert_eq!(grown as usize % (LARGE * 4), 0);
        a.dealloc(grown, layout(LARGE * 8, LARGE * 4));
    }
}

#[test]
fn zeroed_blocks_are_zero_even_in_a_reused_slot() {
    let a = LargeAlloc;
    let l = layout(LARGE * 5, 1);
    // SAFETY: non-zero layouts, freed with the same layout.
    unsafe {
        let dirty = a.alloc(l);
        std::ptr::write_bytes(dirty, 0xFF, l.size());
        a.dealloc(dirty, l);
        for _ in 0..4 {
            let clean = a.alloc_zeroed(l);
            assert!(in_arena(clean));
            let bytes = std::slice::from_raw_parts(clean, l.size());
            assert!(bytes.iter().all(|b| *b == 0));
            a.dealloc(clean, l);
        }
    }
}

/// Ownership comes from the address, not the layout: a `malloc` block
/// freed with a large layout goes to `free`, and an arena block freed with
/// the wrong (small) size still releases its whole slot.
#[test]
fn dealloc_trusts_the_address_not_the_layout() {
    let a = LargeAlloc;
    // SAFETY: `malloc` returns a block `System` (which is `malloc` on
    // macOS for this alignment) can free; the arena block is freed once.
    unsafe {
        let foreign = libc::malloc(LARGE * 4).cast::<u8>();
        assert!(!foreign.is_null() && !in_arena(foreign));
        a.dealloc(foreign, layout(LARGE * 4, 1));

        let l = layout(LARGE * 4, 1);
        let ptr = a.alloc(l);
        std::ptr::write_bytes(ptr, 1, l.size());
        a.dealloc(ptr, layout(100, 1));
        assert!(!resident(ptr, l.size()), "wrong size left pages behind");
    }
}

#[test]
fn interior_pointers_are_left_alone() {
    let a = LargeAlloc;
    let l = layout(LARGE * 2, 1);
    // SAFETY: the interior free is ignored by design; the block is then
    // freed properly.
    unsafe {
        let ptr = a.alloc(l);
        ptr.write(42);
        a.dealloc(ptr.add(PAGE), layout(LARGE, 1));
        assert_eq!(ptr.read(), 42, "an interior free must not drop the block");
        a.dealloc(ptr, l);
    }
}

/// A full size class falls back to `System` instead of failing.
#[test]
fn a_full_size_class_falls_back_to_system() {
    let a = LargeAlloc;
    let l = layout(1 << MAX_SHIFT, 1);
    // SAFETY: untouched 1 GiB blocks cost address space only; each is
    // freed with its layout.
    unsafe {
        let blocks: Vec<*mut u8> = (0..slots(CLASSES - 1) + 1).map(|_| a.alloc(l)).collect();
        assert!(blocks.iter().all(|p| !p.is_null()));
        assert!(blocks.iter().any(|p| !in_arena(*p)), "no fallback");
        for p in blocks {
            a.dealloc(p, l);
        }
    }
}

#[test]
fn failures_return_none_or_null_without_panicking() {
    assert_eq!(reserve(usize::MAX / 2), None);
    let a = LargeAlloc;
    // 64 TiB: past the arena, and more than `System` will hand out.
    // `black_box` keeps LLVM from folding the doomed `malloc` away in
    // release builds.
    let big = layout(std::hint::black_box(1 << 46), 1);
    // SAFETY: a valid, non-zero layout.
    let ptr = std::hint::black_box(unsafe { a.alloc(big) });
    if !ptr.is_null() {
        // SAFETY: allocated just above with `big`.
        unsafe { a.dealloc(ptr, big) };
    }
    assert!(ptr.is_null());
    assert!(arena().is_some(), "the arena reserved its address space");
}

/// Many threads allocating, filling, checking and freeing at once never
/// get the same slot.
#[test]
fn threads_never_share_a_slot() {
    let workers: Vec<_> = (1..=8_u8)
        .map(|id| {
            std::thread::spawn(move || {
                for round in 0..200_usize {
                    let len = LARGE + (round * 7919 + usize::from(id) * 104_729) % (LARGE * 6);
                    let mut block = vec![id; len];
                    block.iter_mut().step_by(4096).for_each(|b| *b = id);
                    std::thread::yield_now();
                    assert!(block.iter().all(|b| *b == id), "slot shared");
                    block.resize(len * 2, id);
                    assert!(block.iter().all(|b| *b == id), "slot shared");
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn slots_map_back_to_their_address() {
    let base = 1 << 40;
    for class in [0, 3, CLASSES - 1] {
        let slot = Slot { class, index: 5 };
        let addr = slot.addr(base);
        assert_eq!(Slot::containing(base, addr), Some((slot, true)));
        assert_eq!(Slot::containing(base, addr + 16), Some((slot, false)));
    }
    assert_eq!(Slot::containing(base, base - 1), None);
    assert_eq!(
        Slot::containing(base, base + CLASSES * super::CLASS_SPAN),
        None
    );
}
