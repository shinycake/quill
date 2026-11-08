//! The faulting thread's return addresses, read from the register state the
//! kernel hands a signal handler (macOS).
//!
//! `backtrace(3)` called inside the handler can come back empty on macOS:
//! the handler runs on the alternate signal stack, and the frame walker
//! stops at the first frame outside the thread's own stack, before it ever
//! reaches the code that faulted (issue #1389's report had no frames). This
//! walk starts from the interrupted program counter and frame pointer
//! instead, and only follows frame pointers that stay inside the faulting
//! thread's stack, so a corrupt chain ends the walk rather than faulting
//! again inside the handler.
//!
//! Only plain reads and `pthread_get_stack*_np` (which read the thread's own
//! descriptor): async-signal-safe, no allocation.

use std::ffi::c_void;

/// The leading fields of the kernel's `ucontext_t` (64-bit layout,
/// `<sys/_types/_ucontext.h>`), up to the machine context pointer.
#[repr(C)]
#[allow(
    dead_code,
    reason = "mirrors the kernel layout; only the pointer is read"
)]
struct UserContext {
    onstack: i32,
    sigmask: u32,
    stack: libc::stack_t,
    link: *mut c_void,
    mcontext_size: usize,
    mcontext: *const MachineContext,
}

/// `__darwin_mcontext64` up to the general registers: a 16-byte exception
/// state, then the thread state.
#[repr(C)]
#[allow(
    dead_code,
    reason = "mirrors the kernel layout; only the pointer is read"
)]
struct MachineContext {
    exception_state: [u64; 2],
    registers: ThreadState,
}

/// `__darwin_arm_thread_state64`.
#[cfg(target_arch = "aarch64")]
#[repr(C)]
#[allow(dead_code, reason = "mirrors the kernel layout")]
struct ThreadState {
    x: [u64; 29],
    fp: u64,
    lr: u64,
    sp: u64,
    pc: u64,
}

/// `__darwin_x86_thread_state64`: rax … r15, then rip.
#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct ThreadState {
    general: [u64; 16],
    rip: u64,
}

#[cfg(target_arch = "x86_64")]
const RBP: usize = 6;

/// Pointer authentication codes sit in the high bits of saved return
/// addresses; user addresses fit in 47 bits.
const ADDRESS_MASK: u64 = 0x0000_7fff_ffff_ffff;

/// Fills `frames` with the interrupted program counter, then the return
/// address of each frame up the faulting thread's stack, and returns how
/// many it wrote. Zero when `context` is null.
pub(super) fn collect(context: *mut c_void, frames: &mut [*mut c_void]) -> usize {
    // SAFETY: the kernel passes a valid `ucontext_t` to an `SA_SIGINFO`
    // handler, and its machine context outlives the handler; both pointers
    // are null-checked by `as_ref`.
    let Some(registers) = (unsafe {
        context
            .cast::<UserContext>()
            .as_ref()
            .and_then(|context| context.mcontext.as_ref())
            .map(|machine| &machine.registers)
    }) else {
        return 0;
    };
    let mut count = 0usize;
    let mut push = |address: u64| {
        let address = address & ADDRESS_MASK;
        if address == 0 {
            return true;
        }
        let Some(slot) = frames.get_mut(count) else {
            return false;
        };
        *slot = address as *mut c_void;
        count += 1;
        true
    };
    #[cfg(target_arch = "aarch64")]
    let frame_pointer = {
        push(registers.pc);
        // A leaf function keeps its caller in the link register only.
        push(registers.lr);
        registers.fp
    };
    #[cfg(target_arch = "x86_64")]
    let frame_pointer = {
        push(registers.rip);
        registers.general.get(RBP).copied().unwrap_or(0)
    };
    let (low, high) = thread_stack_bounds();
    let mut frame = frame_pointer;
    // Each frame record is [previous frame pointer, return address].
    while frame >= low && frame.saturating_add(16) <= high && frame % 8 == 0 {
        // SAFETY: `frame` and `frame + 8` lie inside the faulting thread's
        // stack (checked above), which is mapped and readable.
        let (previous, return_address) = unsafe {
            let record = frame as *const u64;
            (record.read(), record.add(1).read())
        };
        if !push(return_address) || previous <= frame {
            break;
        }
        frame = previous;
    }
    count
}

/// The faulting thread's stack as `[low, high)`. The handler runs on that
/// thread, so `pthread_self` is it, even on the alternate stack.
fn thread_stack_bounds() -> (u64, u64) {
    // SAFETY: both calls only read the calling thread's descriptor.
    let (top, size) = unsafe {
        let thread = libc::pthread_self();
        (
            libc::pthread_get_stackaddr_np(thread) as u64,
            libc::pthread_get_stacksize_np(thread) as u64,
        )
    };
    (top.saturating_sub(size), top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_context_gives_no_frames() {
        let mut frames = [std::ptr::null_mut(); 8];
        assert_eq!(collect(std::ptr::null_mut(), &mut frames), 0);
    }

    #[test]
    fn the_layout_matches_the_kernels() {
        // `uc_mcontext` follows onstack, sigmask, a 24-byte stack_t, uc_link
        // and uc_mcsize.
        assert_eq!(std::mem::offset_of!(UserContext, mcontext), 48);
        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(std::mem::offset_of!(ThreadState, fp), 29 * 8);
            assert_eq!(std::mem::offset_of!(ThreadState, pc), 32 * 8);
        }
        #[cfg(target_arch = "x86_64")]
        assert_eq!(std::mem::offset_of!(ThreadState, rip), 16 * 8);
    }
}
