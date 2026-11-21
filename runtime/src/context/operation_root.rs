//! The root that a runtime operation holds while a script callback runs
//! (`compiler.md` §345 rule 1).
//!
//! A runtime operation that reads a managed reference after a callback
//! returns keeps that reference in a fixed range of words. An
//! [`OperationRoot`] pushes that range as a shadow frame and pops it when
//! the guard drops. Thus the root ends when the operation ends, by a
//! return or by a trap.

use std::cell::Cell;

use super::Context;

/// A shadow frame over a fixed range of 8-byte words that a runtime
/// operation holds for its scope (`compiler.md` §345 rule 1). The frame
/// pops when the guard drops.
#[must_use = "the root ends when the guard drops"]
pub(crate) struct OperationRoot {
    ctx: *mut Context,
}

impl OperationRoot {
    /// Pushes a shadow frame of `words` 8-byte slots at `base`.
    ///
    /// # Safety
    ///
    /// `ctx` is a live Context that outlives the guard. The `words` slots
    /// at `base` stay readable until the guard drops. Each frame that is
    /// pushed after this one pops before this guard drops.
    pub(crate) unsafe fn push(ctx: *mut Context, base: *const u8, words: usize) -> Self {
        // SAFETY: the caller keeps the range readable until the guard drops
        // (section 263).
        unsafe { (*ctx).shadow_push(base as usize, words) };
        Self { ctx }
    }

    /// Pushes a shadow frame over the slots of `cells`.
    ///
    /// # Safety
    ///
    /// As [`OperationRoot::push`]; `cells` stays alive until the guard drops.
    pub(crate) unsafe fn cells(ctx: *mut Context, cells: &[Cell<usize>]) -> Self {
        // SAFETY: `Cell<usize>` has the layout of `usize`, so the slice is
        // `cells.len()` 8-byte slots; the caller keeps it alive.
        unsafe { Self::push(ctx, cells.as_ptr().cast(), cells.len()) }
    }
}

impl Drop for OperationRoot {
    fn drop(&mut self) {
        // SAFETY: `push` requires the Context to outlive the guard, and
        // the frames pushed after this one popped first.
        unsafe { (*self.ctx).shadow_pop() };
    }
}

/// The bits of a callback argument `v` of at most eight bytes, zero
/// extended, for a root slot. A slot is a conservative root, so a scalar
/// in it retains nothing that a handle does not name.
pub(crate) fn root_word<T: Copy>(v: &T) -> usize {
    let mut word = 0usize;
    let size = std::mem::size_of::<T>().min(std::mem::size_of::<usize>());
    // SAFETY: `size` bytes are readable at `v` and writable at `word`.
    unsafe {
        std::ptr::copy_nonoverlapping(
            (v as *const T).cast::<u8>(),
            (&mut word as *mut usize).cast::<u8>(),
            size,
        )
    };
    word
}

impl Context {
    /// The count of shadow frames that generated code and runtime
    /// operations hold now.
    #[must_use]
    pub fn shadow_depth(&self) -> usize {
        self.shadow.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_root_holds_a_frame_until_it_drops() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let slots = [Cell::new(0usize), Cell::new(0usize)];
        assert_eq!(ctx.shadow_depth(), 0);
        {
            // SAFETY: `slots` outlives the guard; no other frame is pushed.
            let _root = unsafe { OperationRoot::cells(p, &slots) };
            // SAFETY: `p` is live.
            assert_eq!(unsafe { (*p).shadow_depth() }, 1);
        }
        assert_eq!(ctx.shadow_depth(), 0);
    }

    #[test]
    fn operation_root_keeps_the_handle_in_its_slot_through_a_collection() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let held = ctx.alloc(16, 1, 0);
        let dropped = ctx.alloc(16, 1, 0);
        let slot = [Cell::new(root_word(&(held as usize)))];
        {
            // SAFETY: `slot` outlives the guard; no other frame is pushed.
            let _root = unsafe { OperationRoot::cells(p, &slot) };
            // SAFETY: `p` is live.
            unsafe { (*p).collect() };
            // SAFETY: `p` is live.
            assert!(unsafe { (*p).is_live(held as usize) });
            // SAFETY: `p` is live.
            assert!(!unsafe { (*p).is_live(dropped as usize) });
        }
        ctx.collect();
        assert!(!ctx.is_live(held as usize));
    }

    #[test]
    fn root_word_zero_extends_a_narrow_value() {
        assert_eq!(root_word(&0xffu8), 0xff);
        assert_eq!(root_word(&-1i32), 0xffff_ffff);
        assert_eq!(root_word(&usize::MAX), usize::MAX);
    }
}
