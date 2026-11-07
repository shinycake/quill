//! Quill: loads a framework from the dyld shared cache one image at a time.
//!
//! One `dlopen` of a framework with hundreds of dependencies holds dyld's
//! loader lock while it maps them all, then the Objective-C runtime lock
//! while it registers them all. Any thread that needs either lock waits:
//! the main thread does on a first `objc_msgSend` cache miss, a Swift
//! metadata lookup or `NSVersionOfRunTimeLibrary`. Loading WritingToolsUI
//! (about 460 images) from a worker after a cold start held main for about
//! 340 ms.
//!
//! So `load` reads the dependency graph from the images' load commands in
//! the shared cache and opens the images dependencies first, one per
//! `dlopen`. Each holds the locks only for its own image: in the same test
//! the longest main-thread wait was about 35 ms, and the load took as long.
//! Upward links are left out, as dyld loads them with the image that names
//! them. Anything this cannot read (no cache, an image outside it, an
//! unknown layout) is left to the final `dlopen` of the framework itself.

use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_int, c_void},
};

const MH_MAGIC_64: u32 = 0xfeed_facf;
const HEADER_SIZE: usize = 32;
const SIZEOFCMDS: usize = 20;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
/// Threads that read one level of the graph's headers. After a cold start
/// each header is its own disk read: one thread took 125 ms for
/// WritingToolsUI's, this many 35 ms.
const READERS: usize = 8;
const RTLD_LAZY: c_int = 1;

/// `dyld_shared_cache_dylib_text_info` from dyld's private header.
#[repr(C)]
struct TextInfo {
    version: u64,
    _load_address_unslid: u64,
    text_segment_size: u64,
    _dylib_uuid: [u8; 16],
    path: *const c_char,
    /// From version 2.
    text_segment_offset: u64,
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn _dyld_get_shared_cache_uuid(uuid: *mut [u8; 16]) -> bool;
    fn _dyld_get_shared_cache_range(length: *mut usize) -> *const c_void;
    fn dyld_shared_cache_iterate_text(
        uuid: *const [u8; 16],
        // Gets a `*const TextInfo`.
        callback: &block2::DynBlock<dyn Fn(*const c_void) + '_>,
    ) -> c_int;
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(index: u32) -> *const c_char;
}

/// An image's `__TEXT` segment, mapped in the shared cache for the life of
/// the process. Reading it may read the disk.
type Text = &'static [u8];

struct Image<'a> {
    path: &'a [u8],
    text: Text,
    deps: Vec<usize>,
}

/// Loads `root` and what it links, one image per `dlopen`, dependencies
/// first. Call off the main thread. An image that fails to load is left to
/// the image that links it, which fails the same way.
pub(crate) fn load(root: &CStr) {
    let order = load_order(root).unwrap_or_else(|| vec![root.to_owned()]);
    for path in order {
        // SAFETY: a NUL-terminated path; the handle is never closed.
        let _ = unsafe { dlopen(path.as_ptr(), RTLD_LAZY) };
    }
}

/// `root` and the images it loads that are not loaded yet, each after the
/// images it links. `None` when `root` is not in the shared cache.
fn load_order(root: &CStr) -> Option<Vec<CString>> {
    let cache = cache_images()?;
    let loaded = loaded_images();
    let root_path = root.to_bytes();
    let mut images = vec![Image {
        path: root_path,
        text: *cache.get(root_path)?,
        deps: Vec::new(),
    }];
    let mut index = HashMap::from([(root_path, 0usize)]);
    let mut level = vec![0usize];
    while !level.is_empty() {
        let commands: Vec<(usize, Text)> = level
            .iter()
            .filter_map(|&i| Some((i, load_commands(images.get(i)?.text)?)))
            .collect();
        let mut next = Vec::new();
        for (i, commands) in commands {
            let mut deps = Vec::new();
            for dep in dependencies(commands) {
                if loaded.contains(dep) {
                    continue;
                }
                let Some(&text) = cache.get(dep) else {
                    continue;
                };
                let j = *index.entry(dep).or_insert_with(|| {
                    images.push(Image {
                        path: dep,
                        text,
                        deps: Vec::new(),
                    });
                    next.push(images.len() - 1);
                    images.len() - 1
                });
                deps.push(j);
            }
            if let Some(image) = images.get_mut(i) {
                image.deps = deps;
            }
        }
        read_ahead(next.iter().filter_map(|&i| Some(images.get(i)?.text)));
        level = next;
    }
    Some(dependencies_first(&images))
}

/// The images' paths, depth first from the first, each after its
/// dependencies.
fn dependencies_first(images: &[Image]) -> Vec<CString> {
    let mut seen = vec![false; images.len()];
    let mut order = Vec::with_capacity(images.len());
    let mut stack = vec![(0usize, 0usize)];
    if let Some(first) = seen.first_mut() {
        *first = true;
    }
    while let Some(top) = stack.last_mut() {
        let (i, next) = *top;
        top.1 += 1;
        match images.get(i).and_then(|image| image.deps.get(next)) {
            Some(&dep) => {
                if let Some(seen) = seen.get_mut(dep)
                    && !*seen
                {
                    *seen = true;
                    stack.push((dep, 0));
                }
            }
            None => {
                stack.pop();
                if let Some(image) = images.get(i) {
                    order.extend(CString::new(image.path).ok());
                }
            }
        }
    }
    order
}

/// Every image in the shared cache, by install name.
fn cache_images() -> Option<HashMap<Vec<u8>, Text>> {
    let mut uuid = [0u8; 16];
    let mut length = 0usize;
    // SAFETY: each writes to the local it is given.
    let (found, base) = unsafe {
        (
            _dyld_get_shared_cache_uuid(&mut uuid),
            _dyld_get_shared_cache_range(&mut length),
        )
    };
    if !found || base.is_null() {
        return None;
    }
    // SAFETY: dyld maps the whole range for the life of the process.
    let cache: &'static [u8] = unsafe { std::slice::from_raw_parts(base.cast(), length) };
    let images = Mutex::new(HashMap::new());
    let block = block2::StackBlock::new(|info: *const c_void| {
        // SAFETY: dyld passes a valid record, or null.
        let Some(info) = (unsafe { info.cast::<TextInfo>().as_ref() }) else {
            return;
        };
        if info.version < 2 || info.path.is_null() {
            return;
        }
        let text = usize::try_from(info.text_segment_offset)
            .ok()
            .zip(usize::try_from(info.text_segment_size).ok())
            .and_then(|(start, size)| cache.get(start..start.checked_add(size)?));
        if let Some(text) = text {
            // SAFETY: a NUL-terminated path, valid during the call.
            let path = unsafe { CStr::from_ptr(info.path) }.to_bytes().to_vec();
            images.lock().insert(path, text);
        }
    });
    // SAFETY: `uuid` names the mapped cache; the block runs during the call.
    if unsafe { dyld_shared_cache_iterate_text(&uuid, &block) } != 0 {
        return None;
    }
    Some(images.into_inner())
}

/// The load commands after the 64-bit Mach-O header at the start of `text`.
fn load_commands(text: Text) -> Option<Text> {
    if read_u32(text, 0)? != MH_MAGIC_64 {
        return None;
    }
    let size = usize::try_from(read_u32(text, SIZEOFCMDS)?).ok()?;
    text.get(HEADER_SIZE..HEADER_SIZE.checked_add(size)?)
}

/// The install names an image links, upward links left out.
fn dependencies(commands: Text) -> Vec<Text> {
    let mut deps = Vec::new();
    let mut offset = 0usize;
    while let Some(cmd) = read_u32(commands, offset)
        && let Some(size) = read_u32(commands, offset + 4).and_then(|s| usize::try_from(s).ok())
        && size >= 8
        && let Some(body) = commands.get(offset..offset.saturating_add(size))
    {
        if matches!(cmd, LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB) {
            deps.extend(dylib_name(body));
        }
        offset += size;
    }
    deps
}

/// The name in a `dylib_command`.
fn dylib_name(body: Text) -> Option<Text> {
    let name = body.get(usize::try_from(read_u32(body, 8)?).ok()?..)?;
    name.get(..name.iter().position(|&b| b == 0)?)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let word = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_ne_bytes(word.try_into().ok()?))
}

fn loaded_images() -> HashSet<Vec<u8>> {
    // SAFETY: reads dyld's image count.
    let count = unsafe { _dyld_image_count() };
    (0..count)
        .filter_map(|i| {
            // SAFETY: an index below the count; null if that image was
            // unloaded meanwhile.
            let name = unsafe { _dyld_get_image_name(i) };
            if name.is_null() {
                return None;
            }
            // SAFETY: a NUL-terminated path owned by dyld.
            Some(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec())
        })
        .collect()
}

/// Reads the next level's headers on several threads at once, so their
/// disk reads overlap instead of waiting on each other.
fn read_ahead(texts: impl Iterator<Item = Text>) {
    let texts: Vec<Text> = texts.collect();
    let per_reader = texts.len().div_ceil(READERS).max(1);
    std::thread::scope(|scope| {
        for chunk in texts.chunks(per_reader) {
            let read = move || {
                for &text in chunk {
                    std::hint::black_box(load_commands(text).and_then(|commands| commands.last()));
                }
            };
            if std::thread::Builder::new()
                .spawn_scoped(scope, read)
                .is_err()
            {
                read();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A framework the test binary does not link, with dependencies of its
    /// own.
    const ROOT: &CStr = c"/System/Library/Frameworks/AVKit.framework/Versions/A/AVKit";

    #[test]
    fn opens_each_image_once_after_the_images_it_links() {
        let (Some(cache), Some(order)) = (cache_images(), load_order(ROOT)) else {
            panic!("AVKit is not in the shared cache");
        };
        let position: HashMap<&[u8], usize> = order
            .iter()
            .enumerate()
            .map(|(i, path)| (path.to_bytes(), i))
            .collect();
        assert_eq!(position.len(), order.len(), "an image is opened twice");
        assert!(order.len() > 1, "AVKit's dependencies are left out");
        assert_eq!(order.last().map(CString::as_c_str), Some(ROOT));
        let links: Vec<Vec<usize>> = order
            .iter()
            .map(|path| {
                let commands = cache.get(path.to_bytes()).copied().and_then(load_commands);
                let deps = commands.map(dependencies).unwrap_or_default();
                deps.iter().filter_map(|dep| position.get(dep).copied()).collect()
            })
            .collect();
        for (i, deps) in links.iter().enumerate() {
            for &j in deps {
                // Images that link each other in a loop load together.
                assert!(
                    j < i || reaches(&links, j, i),
                    "{:?} is opened before {:?}",
                    order[i],
                    order[j]
                );
            }
        }
    }

    fn reaches(links: &[Vec<usize>], from: usize, to: usize) -> bool {
        let mut seen = HashSet::from([from]);
        let mut stack = vec![from];
        while let Some(i) = stack.pop() {
            if i == to {
                return true;
            }
            stack.extend(links[i].iter().filter(|&&j| seen.insert(j)));
        }
        false
    }
}
