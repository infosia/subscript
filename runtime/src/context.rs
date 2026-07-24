//! The Context: owner of every script allocation (reference-class
//! instances, array storage, string storage, coroutine frames), the
//! stdout sink, the GC roots the generated code registers, and the
//! trap state.
//!
//! # Memory model
//!
//! Every allocation is `HEADER_SIZE` bytes of header followed by the
//! payload; handles held by script code are payload pointers. The
//! header holds a state word (`LIVE_STATE` / `DEAD_STATE`) and the
//! class id; generated code reads both directly (use-after-delete
//! checks, checked `as` narrowing), so their offsets are part of the
//! runtime's ABI contract.
//!
//! Development-tier policy: `unsafeDelete` and `collect()` mark an
//! allocation dead and poison its header but keep the bytes until the
//! Context is dropped. This is what makes double delete and
//! use-after-delete *trap* instead of being undefined: a stale handle
//! still points at owned memory whose header says `DEAD_STATE`.
//!
//! # Collection
//!
//! `collect()` never runs unbidden (design invariant 2). Roots are the
//! addresses generated code registers: module-global slots
//! (`root_add`) and per-call shadow frames of managed locals
//! (`shadow_push`/`shadow_pop`). Marking is conservative: the payload
//! of every reached allocation is scanned for pointer-aligned words
//! that equal a live payload address (this covers reference-class
//! fields, array elements, array data pointers, and coroutine frame
//! slots without layout metadata). Conservative marking can retain
//! garbage; it never frees a reachable allocation.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;

use crate::trap::{TrapKind, TrapRecord};

/// Bytes between an allocation's base and its payload.
pub const HEADER_SIZE: usize = 16;
/// Header state word for a live allocation (offset -16 from payload).
pub const LIVE_STATE: u64 = 0x5355_4253_4C49_5645; // "SUBSLIVE"
/// Header state word for a deleted/collected allocation.
pub const DEAD_STATE: u64 = 0x5355_4253_4445_4144; // "SUBSDEAD"
/// Byte offset of the state word relative to the payload pointer.
pub const STATE_OFFSET: i32 = -16;
/// Byte offset of the class id relative to the payload pointer.
pub const CLASS_ID_OFFSET: i32 = -8;

/// Class id used for string allocations.
pub const CLASS_STRING: u32 = 0xFFFF_FF01;
/// Class id used for dynamic-array headers.
pub const CLASS_ARRAY: u32 = 0xFFFF_FF02;
/// Class id used for dynamic-array element storage.
pub const CLASS_ARRAY_DATA: u32 = 0xFFFF_FF03;
/// Class id used for coroutine frames.
pub const CLASS_GENERATOR: u32 = 0xFFFF_FF04;

/// Payload layout of a dynamic array (Q4): length, capacity, element
/// size, and a pointer to a separate `CLASS_ARRAY_DATA` allocation.
#[repr(C)]
struct ArrayHeader {
    len: u64,
    cap: u64,
    elem_size: u64,
    data: *mut u8,
}

struct Allocation {
    base: *mut u8,
    layout: Layout,
    payload_size: usize,
    live: bool,
    marked: bool,
}

/// A registered C-callback binding (P5.2b). The language's function value
/// is a `(code, env)` pair with the calling convention `(ctx, env,
/// args...)`; a C callback wants a bare `(fnptr, void* userdata)`. A
/// generic C-ABI trampoline ([`crate::ffi::sub_rt_cb_trampoline`]) bridges
/// the two: the record is what the trampoline receives through the C
/// `userdata` slot, so it carries everything the language convention needs
/// — the Context, the language `code`/`env`, and the *real* userdata the
/// script registered. Records live for the whole Context (the Q13
/// lifetime rule: userdata must outlive the registration that holds it).
///
/// P7.2 (§14.4): the record carries **two** userdata slots (`userdata1`,
/// `userdata2`), both delivered to the language callback. A callback-info
/// with one userdata slot binds the second as null.
#[repr(C)]
pub struct CallbackBinding {
    /// The Context the script runs under; captured at bind time.
    pub ctx: *mut Context,
    /// The language function value's code pointer (a wrapper taking
    /// `(ctx, env, args...)`, host C calling convention).
    pub code: *const u8,
    /// The language function value's environment pointer (null for a
    /// non-capturing function — the only kind usable as a C callback, C5).
    pub env: *const u8,
    /// The first userdata the script registered, passed back to the
    /// language callback unchanged.
    pub userdata1: *mut u8,
    /// The second userdata (§14.4); null when the callback-info carries
    /// only one slot.
    pub userdata2: *mut u8,
}

/// The script execution context.
///
/// `repr(C)` with a fixed prefix that generated code reads directly
/// (the runtime's ABI contract):
///
/// | offset | field | read by |
/// |---|---|---|
/// | 0 | trap flag (`u32`) | every emitted trap check |
/// | 4 | reload epoch (`u32`) | coroutine resume, hot-reload mode |
/// | 8 | function table (`*const *const u8`) | script calls, hot-reload mode |
/// | 16 | module-global block (`*mut u8`) | global access, hot-reload mode |
///
/// Everything past the prefix is opaque to generated code. The three
/// hot-reload fields are read only by code lowered in reload mode; the
/// AOT tier and the plain dev-JIT run never touch them.
#[repr(C)]
pub struct Context {
    trap_flag: u32,
    reload_epoch: u32,
    fn_table: *const *const u8,
    globals: *mut u8,
    script_depth: u32,
    allocations: HashMap<usize, Allocation>,
    stdout: Vec<u8>,
    trap: Option<TrapRecord>,
    interned: HashMap<(usize, usize), usize>,
    shadow: Vec<(usize, usize)>,
    roots: Vec<(usize, usize)>,
    callbacks: Vec<Box<CallbackBinding>>,
}

impl Context {
    /// Creates an empty context.
    #[must_use]
    pub fn new() -> Box<Context> {
        Box::new(Context {
            trap_flag: 0,
            reload_epoch: 0,
            fn_table: std::ptr::null(),
            globals: std::ptr::null_mut(),
            script_depth: 0,
            allocations: HashMap::new(),
            stdout: Vec::new(),
            trap: None,
            interned: HashMap::new(),
            shadow: Vec::new(),
            roots: Vec::new(),
            callbacks: Vec::new(),
        })
    }

    /// Byte offset of the trap flag inside the context (ABI contract
    /// with generated code).
    #[must_use]
    pub fn trap_flag_offset() -> usize {
        // repr(C): trap_flag is the first field.
        0
    }

    /// Byte offset of the reload epoch (ABI contract with generated
    /// code lowered in hot-reload mode).
    #[must_use]
    pub fn reload_epoch_offset() -> usize {
        4
    }

    /// Byte offset of the per-function indirection table pointer (ABI
    /// contract with generated code lowered in hot-reload mode).
    #[must_use]
    pub fn fn_table_offset() -> usize {
        8
    }

    /// Byte offset of the module-global block pointer (ABI contract
    /// with generated code lowered in hot-reload mode).
    #[must_use]
    pub fn globals_offset() -> usize {
        16
    }

    // ----- hot-reload state (dev tier) -----

    /// Points the indirection table at `table`, an array of code
    /// addresses indexed by the lowering's function slot numbers.
    ///
    /// Storing the pointer is safe; generated code dereferences it, so
    /// the array must stay alive and correctly sized for as long as
    /// script code compiled in reload mode can run.
    pub fn set_fn_table(&mut self, table: *const *const u8) {
        self.fn_table = table;
    }

    /// Points module-global storage at `base`, a block laid out by the
    /// lowering. It must stay alive for the rest of the session: the
    /// collector's registered root ranges point into it.
    pub fn set_globals(&mut self, base: *mut u8) {
        self.globals = base;
    }

    /// The current reload epoch. Coroutine frames record the epoch
    /// they were created in; a resume across a swap traps.
    #[must_use]
    pub fn reload_epoch(&self) -> u32 {
        self.reload_epoch
    }

    /// Advances the reload epoch, invalidating every coroutine frame
    /// created before the swap.
    pub fn bump_reload_epoch(&mut self) {
        self.reload_epoch = self.reload_epoch.wrapping_add(1);
    }

    /// Marks entry into script code (the host called into the script).
    pub fn enter_script(&mut self) {
        self.script_depth = self.script_depth.saturating_add(1);
    }

    /// Marks return from script code.
    pub fn exit_script(&mut self) {
        self.script_depth = self.script_depth.saturating_sub(1);
    }

    /// Number of host-to-script calls currently on the stack. A hot
    /// reload is applied only at zero (the frame-boundary rule).
    #[must_use]
    pub fn script_depth(&self) -> u32 {
        self.script_depth
    }

    // ----- trap state -----

    /// Records a trap. The first trap wins; later ones are ignored
    /// (generated code unwinds after the first, but runtime functions
    /// invoked on the unwind path stay callable).
    pub fn trap(&mut self, kind: TrapKind, message: impl Into<String>, pos_id: u32) {
        if self.trap.is_none() {
            self.trap = Some(TrapRecord::new(kind, message, pos_id));
        }
        self.trap_flag = 1;
    }

    /// The recorded trap, if any.
    #[must_use]
    pub fn trap_record(&self) -> Option<&TrapRecord> {
        self.trap.as_ref()
    }

    /// Clears the trap record and lowers the trap flag, so the next
    /// host call into script starts from a clean state
    /// (`specs/blocks/compiler.md` §8.2: a trap does not end the dev
    /// session).
    ///
    /// Only trap *reporting* state is touched. Allocations, globals,
    /// roots, the stdout sink, and the reload epoch are all untouched,
    /// so nothing a trap protected against becomes reachable again: a
    /// deleted allocation stays poisoned and a stale coroutine stays
    /// stale (its frame epoch still differs, so resuming it traps
    /// again).
    ///
    /// The caller must be at a host↔script boundary — no generated
    /// code on the stack. Generated code reads the flag after every
    /// fault-capable call and unwinds on it; clearing while a script
    /// frame is live would resume a run that has already given up.
    /// [`Context::script_depth`] is the check.
    pub fn clear_trap(&mut self) {
        self.trap = None;
        self.trap_flag = 0;
    }

    /// True when a trap is pending.
    #[must_use]
    pub fn trapped(&self) -> bool {
        self.trap_flag != 0
    }

    // ----- stdout sink -----

    /// Appends `bytes` and a trailing newline to the stdout sink
    /// (print never writes to the process stdout).
    pub fn print_line(&mut self, bytes: &[u8]) {
        self.stdout.extend_from_slice(bytes);
        self.stdout.push(b'\n');
    }

    /// Takes the captured stdout bytes.
    #[must_use]
    pub fn take_stdout(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stdout)
    }

    /// Borrows the captured stdout bytes without draining the sink.
    #[must_use]
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout
    }

    // ----- allocation -----

    /// Allocates `size` payload bytes tagged `class_id`; returns the
    /// zeroed payload pointer, or null after recording an
    /// allocation-failure trap.
    pub fn alloc(&mut self, size: usize, class_id: u32, pos_id: u32) -> *mut u8 {
        let total = HEADER_SIZE.saturating_add(size.max(1));
        let Ok(layout) = Layout::from_size_align(total, 16) else {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes is not representable"),
                pos_id,
            );
            return std::ptr::null_mut();
        };
        // SAFETY: `layout` has non-zero size (>= HEADER_SIZE + 1).
        let base = unsafe { alloc_zeroed(layout) };
        if base.is_null() {
            self.trap(
                TrapKind::AllocationFailure,
                format!("allocation of {size} bytes failed"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: `base` is a fresh allocation of at least HEADER_SIZE
        // bytes; the header writes stay inside it.
        unsafe {
            (base as *mut u64).write(LIVE_STATE);
            (base.add(8) as *mut u32).write(class_id);
        }
        // SAFETY: HEADER_SIZE <= total.
        let payload = unsafe { base.add(HEADER_SIZE) };
        self.allocations.insert(
            payload as usize,
            Allocation {
                base,
                layout,
                payload_size: size,
                live: true,
                marked: false,
            },
        );
        payload
    }

    /// Marks the allocation at `payload` dead. Development-tier
    /// semantics of `unsafeDelete` (Q6): double delete and unknown
    /// pointers trap; the bytes are retained (poisoned) so stale
    /// handles trap instead of reading freed memory.
    pub fn delete(&mut self, payload: usize, pos_id: u32) {
        match self.allocations.get_mut(&payload) {
            None => {
                self.trap(
                    TrapKind::InvalidDelete,
                    "unsafeDelete of a pointer the Context does not own",
                    pos_id,
                );
            }
            Some(a) if !a.live => {
                self.trap(
                    TrapKind::DoubleDelete,
                    "unsafeDelete of an already-deleted allocation",
                    pos_id,
                );
            }
            Some(a) => {
                a.live = false;
                // SAFETY: `base` is owned by this context and at least
                // HEADER_SIZE bytes; poisoning the state word makes the
                // emitted use-after-delete checks fire.
                unsafe { (a.base as *mut u64).write(DEAD_STATE) };
            }
        }
    }

    /// True when `payload` is a live allocation (test/inspection aid).
    #[must_use]
    pub fn is_live(&self, payload: usize) -> bool {
        self.allocations.get(&payload).is_some_and(|a| a.live)
    }

    /// Number of live allocations (test/inspection aid).
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.allocations.values().filter(|a| a.live).count()
    }

    // ----- roots and collection -----

    /// Registers a permanent root range: `words` consecutive 8-byte
    /// slots at `base`, conservatively scanned for managed handles.
    /// One word for a scalar managed global; several for a global
    /// aggregate (e.g. a `FixedArray` of references) whose interior
    /// holds handles.
    pub fn root_add(&mut self, base: usize, words: usize) {
        self.roots.push((base, words));
    }

    /// Pushes a shadow frame: `slots` consecutive 8-byte slots at
    /// `base`, each holding a managed local (or null).
    pub fn shadow_push(&mut self, base: usize, slots: usize) {
        self.shadow.push((base, slots));
    }

    /// Pops the most recent shadow frame.
    pub fn shadow_pop(&mut self) {
        self.shadow.pop();
    }

    /// Explicitly invoked collection (Q7): frees every allocation not
    /// reachable from the registered roots. Never runs unbidden.
    pub fn collect(&mut self) {
        let mut work: Vec<usize> = Vec::new();
        for &(base, words) in &self.roots {
            for i in 0..words {
                // SAFETY: root ranges are addresses of live global
                // slots registered by generated code; reading their
                // words is valid for the duration of the script run.
                work.push(unsafe { ((base + i * 8) as *const usize).read_unaligned() });
            }
        }
        for &(base, slots) in &self.shadow {
            for i in 0..slots {
                // SAFETY: shadow frames are live stack ranges registered
                // by the running generated code.
                work.push(unsafe { ((base + i * 8) as *const usize).read_unaligned() });
            }
        }
        work.extend(self.interned.values().copied());

        while let Some(addr) = work.pop() {
            let Some(a) = self.allocations.get_mut(&addr) else {
                continue;
            };
            if !a.live || a.marked {
                continue;
            }
            a.marked = true;
            let payload = addr as *const u8;
            let words = a.payload_size / 8;
            for i in 0..words {
                // SAFETY: the payload is owned by this context and at
                // least `payload_size` bytes; reads stay inside it.
                let w = unsafe { (payload.add(i * 8) as *const usize).read_unaligned() };
                work.push(w);
            }
        }

        for a in self.allocations.values_mut() {
            if a.live && !a.marked {
                a.live = false;
                // SAFETY: as in `delete`: poison the retained header.
                unsafe { (a.base as *mut u64).write(DEAD_STATE) };
            }
            a.marked = false;
        }
    }

    // ----- strings (Q5) -----

    /// Allocates an immutable string; payload = `[len: u64][bytes]`.
    /// Returns the payload pointer (the string handle).
    pub fn alloc_str(&mut self, bytes: &[u8], pos_id: u32) -> *mut u8 {
        let p = self.alloc(8 + bytes.len(), CLASS_STRING, pos_id);
        if p.is_null() {
            return p;
        }
        // SAFETY: `p` points at a fresh allocation of 8 + len bytes.
        unsafe {
            (p as *mut u64).write(bytes.len() as u64);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(8), bytes.len());
        }
        p
    }

    /// Reads the bytes of a string handle. The borrow is tied to
    /// `&self`: string storage lives as long as the context and is
    /// immutable, and `&self` prevents freeing while the slice is
    /// alive.
    ///
    /// # Safety
    ///
    /// `handle` must be a string payload produced by [`Context::alloc_str`]
    /// on this context and still owned by it.
    #[must_use]
    pub unsafe fn str_bytes(&self, handle: *const u8) -> &[u8] {
        // SAFETY: caller guarantees `handle` is a live string payload;
        // its first 8 bytes are the length of the following bytes.
        unsafe {
            let len = (handle as *const u64).read() as usize;
            std::slice::from_raw_parts(handle.add(8), len)
        }
    }

    /// The address of a string handle's UTF-8 bytes (the C `const char*`
    /// half of a `(ptr, len)` string view; the length is
    /// [`Context::str_bytes`]`.len()`, also `sub_rt_str_len`). Skips the
    /// 8-byte length prefix of the string payload.
    ///
    /// # Safety
    ///
    /// `handle` must be a live string payload of this context.
    #[must_use]
    pub unsafe fn str_data(&self, handle: *const u8) -> *const u8 {
        // SAFETY: the bytes follow the 8-byte length prefix.
        unsafe { handle.add(8) }
    }

    /// The address of a dynamic array's element storage (the C `const T*`
    /// half of a `(ptr, count)` descriptor; the count is
    /// [`Context::array_len`]). Null for an array that has never grown.
    ///
    /// # Safety
    ///
    /// `handle` must be a live array payload of this context.
    #[must_use]
    pub unsafe fn array_data(&self, handle: *const u8) -> *const u8 {
        // SAFETY: caller guarantees an array payload.
        unsafe { (*(handle as *const ArrayHeader)).data }
    }

    /// Registers a C-callback binding and returns a stable pointer to it
    /// (P5.2b). The pointer is what a boundary marshaler stores in a C
    /// `void* userdata` slot; the generic trampoline
    /// ([`crate::ffi::sub_rt_cb_trampoline`]) reads the binding back
    /// through it. Bindings live for the whole Context (the Q13 lifetime
    /// rule), so the pointer stays valid for every later callback.
    ///
    /// Both userdata slots (§14.4) are stored and delivered to the language
    /// callback; a one-slot callback-info passes `userdata2` as null.
    pub fn bind_callback(
        &mut self,
        code: *const u8,
        env: *const u8,
        userdata1: *mut u8,
        userdata2: *mut u8,
    ) -> *mut u8 {
        let ctx: *mut Context = self;
        let mut rec = Box::new(CallbackBinding {
            ctx,
            code,
            env,
            userdata1,
            userdata2,
        });
        let ptr: *mut CallbackBinding = &mut *rec;
        self.callbacks.push(rec);
        ptr as *mut u8
    }

    /// Interns a string literal by its static data address; repeated
    /// executions of the same literal reuse one allocation. Interned
    /// strings are collection roots.
    ///
    /// # Safety
    ///
    /// `ptr` must point at `len` readable bytes that outlive the
    /// context (the code generator emits them as module data).
    pub unsafe fn intern_literal(&mut self, ptr: *const u8, len: usize, pos_id: u32) -> *mut u8 {
        if let Some(&p) = self.interned.get(&(ptr as usize, len)) {
            return p as *mut u8;
        }
        // SAFETY: caller guarantees `ptr`/`len` is readable.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        let p = self.alloc_str(bytes, pos_id);
        if !p.is_null() {
            self.interned.insert((ptr as usize, len), p as usize);
        }
        p
    }

    // ----- arrays (Q4) -----

    /// Allocates an empty dynamic array with `elem_size`-byte elements.
    pub fn array_new(&mut self, elem_size: usize, pos_id: u32) -> *mut u8 {
        let p = self.alloc(std::mem::size_of::<ArrayHeader>(), CLASS_ARRAY, pos_id);
        if p.is_null() {
            return p;
        }
        // SAFETY: `p` is a fresh allocation of ArrayHeader size.
        unsafe {
            (p as *mut ArrayHeader).write(ArrayHeader {
                len: 0,
                cap: 0,
                elem_size: elem_size as u64,
                data: std::ptr::null_mut(),
            });
        }
        p
    }

    /// Array length as `i32`.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    #[must_use]
    pub unsafe fn array_len(&self, handle: *const u8) -> i32 {
        // SAFETY: caller guarantees an array payload.
        unsafe { (*(handle as *const ArrayHeader)).len as i32 }
    }

    /// Appends one element (copied from `src`); returns the new length,
    /// or -1 after a trap.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context; `src`
    /// must be readable for the array's element size.
    pub unsafe fn array_push(&mut self, handle: *mut u8, src: *const u8, pos_id: u32) -> i32 {
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == h.cap {
            let new_cap = if h.cap == 0 { 4 } else { h.cap * 2 };
            let elem = h.elem_size as usize;
            let new_data = self.alloc(new_cap as usize * elem, CLASS_ARRAY_DATA, pos_id);
            if new_data.is_null() {
                return -1;
            }
            // Re-borrow after alloc (`self` was mutably borrowed).
            // SAFETY: as above.
            let h = unsafe { &mut *(handle as *mut ArrayHeader) };
            if !h.data.is_null() {
                // SAFETY: old data holds `len * elem` initialized bytes;
                // new data is at least twice as large.
                unsafe {
                    std::ptr::copy_nonoverlapping(h.data, new_data, h.len as usize * elem);
                }
                let old = h.data as usize;
                // Retire the old storage (internal, so not a trap path).
                if let Some(a) = self.allocations.get_mut(&old) {
                    a.live = false;
                    // SAFETY: poisons the retained header, as in `delete`.
                    unsafe { (a.base as *mut u64).write(DEAD_STATE) };
                }
            }
            // SAFETY: as above.
            let h = unsafe { &mut *(handle as *mut ArrayHeader) };
            h.data = new_data;
            h.cap = new_cap;
        }
        // SAFETY: as above.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        let elem = h.elem_size as usize;
        // SAFETY: `data` has capacity for `cap` elements and len < cap;
        // `src` is readable for `elem` bytes per the caller contract.
        unsafe {
            std::ptr::copy_nonoverlapping(src, h.data.add(h.len as usize * elem), elem);
        }
        h.len += 1;
        h.len as i32
    }

    /// Removes the last element, copying it to `dst`. Traps on empty.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context; `dst`
    /// must be writable for the array's element size.
    pub unsafe fn array_pop(&mut self, handle: *mut u8, dst: *mut u8, pos_id: u32) {
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &mut *(handle as *mut ArrayHeader) };
        if h.len == 0 {
            self.trap(TrapKind::EmptyPop, "pop() on an empty array", pos_id);
            return;
        }
        h.len -= 1;
        let elem = h.elem_size as usize;
        // SAFETY: the removed slot holds an initialized element; `dst`
        // is writable per the caller contract.
        unsafe {
            std::ptr::copy_nonoverlapping(h.data.add(h.len as usize * elem), dst, elem);
        }
    }

    /// Returns the address of element `idx`, or null after an
    /// out-of-bounds trap.
    ///
    /// # Safety
    ///
    /// `handle` must be an array payload owned by this context.
    pub unsafe fn array_elem_ptr(&mut self, handle: *mut u8, idx: i32, pos_id: u32) -> *mut u8 {
        // SAFETY: caller guarantees an array payload.
        let h = unsafe { &*(handle as *const ArrayHeader) };
        if idx < 0 || idx as u64 >= h.len {
            let len = h.len;
            self.trap(
                TrapKind::IndexOutOfBounds,
                format!("index {idx} out of bounds for array length {len}"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
        // SAFETY: 0 <= idx < len <= cap.
        unsafe { h.data.add(idx as usize * h.elem_size as usize) }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        for a in self.allocations.values() {
            // SAFETY: `base`/`layout` came from `alloc_zeroed` in
            // `Context::alloc` and are freed exactly once, here.
            unsafe { dealloc(a.base, a.layout) };
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("trap_flag", &self.trap_flag)
            .field("allocations", &self.allocations.len())
            .field("stdout_len", &self.stdout.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The emitted-C SsArrayHeader (codegen/src/cemit.rs, §10a) mirrors this
    // layout; a reorder here is caught by this test.
    #[test]
    fn array_header_offsets_match_the_abi_contract() {
        assert_eq!(core::mem::offset_of!(ArrayHeader, len), 0);
        assert_eq!(core::mem::offset_of!(ArrayHeader, cap), 8);
        assert_eq!(core::mem::offset_of!(ArrayHeader, elem_size), 16);
        assert_eq!(core::mem::offset_of!(ArrayHeader, data), 24);
        assert_eq!(core::mem::size_of::<ArrayHeader>(), 32);
    }

    #[test]
    fn trap_flag_is_at_offset_zero() {
        let ctx = Context::new();
        let base = &*ctx as *const Context as usize;
        let flag = &ctx.trap_flag as *const u32 as usize;
        assert_eq!(flag - base, Context::trap_flag_offset());
    }

    #[test]
    fn reload_prefix_offsets_match_the_abi_contract() {
        let ctx = Context::new();
        let base = &*ctx as *const Context as usize;
        assert_eq!(
            &ctx.reload_epoch as *const u32 as usize - base,
            Context::reload_epoch_offset()
        );
        assert_eq!(
            &ctx.fn_table as *const *const *const u8 as usize - base,
            Context::fn_table_offset()
        );
        assert_eq!(
            &ctx.globals as *const *mut u8 as usize - base,
            Context::globals_offset()
        );
    }

    #[test]
    fn reload_epoch_and_script_depth_track_swaps_and_entries() {
        let mut ctx = Context::new();
        assert_eq!(ctx.reload_epoch(), 0);
        ctx.bump_reload_epoch();
        assert_eq!(ctx.reload_epoch(), 1);
        assert_eq!(ctx.script_depth(), 0);
        ctx.enter_script();
        ctx.enter_script();
        assert_eq!(ctx.script_depth(), 2);
        ctx.exit_script();
        ctx.exit_script();
        ctx.exit_script();
        assert_eq!(ctx.script_depth(), 0);
    }

    #[test]
    fn fn_table_and_globals_pointers_round_trip() {
        let mut ctx = Context::new();
        let table: [*const u8; 2] = [std::ptr::null(), std::ptr::null()];
        let mut block = [0u8; 16];
        ctx.set_fn_table(table.as_ptr());
        ctx.set_globals(block.as_mut_ptr());
        assert_eq!(ctx.fn_table, table.as_ptr());
        assert_eq!(ctx.globals, block.as_mut_ptr());
    }

    #[test]
    fn print_appends_bytes_and_newline_to_the_sink() {
        let mut ctx = Context::new();
        ctx.print_line(b"hello");
        ctx.print_line(b"x");
        assert_eq!(ctx.take_stdout(), b"hello\nx\n");
        assert!(ctx.take_stdout().is_empty());
    }

    #[test]
    fn alloc_is_zeroed_tagged_and_live() {
        let mut ctx = Context::new();
        let p = ctx.alloc(24, 3, 0);
        assert!(!p.is_null());
        assert!(ctx.is_live(p as usize));
        // SAFETY: p is a fresh 24-byte payload with a 16-byte header.
        unsafe {
            assert_eq!((p.offset(STATE_OFFSET as isize) as *const u64).read(), LIVE_STATE);
            assert_eq!((p.offset(CLASS_ID_OFFSET as isize) as *const u32).read(), 3);
            for i in 0..24 {
                assert_eq!(p.add(i).read(), 0);
            }
        }
    }

    #[test]
    fn delete_poisons_and_double_delete_traps() {
        let mut ctx = Context::new();
        let p = ctx.alloc(8, 1, 0);
        ctx.delete(p as usize, 5);
        assert!(!ctx.is_live(p as usize));
        // SAFETY: bytes are retained after delete (dev-tier policy).
        unsafe {
            assert_eq!((p.offset(STATE_OFFSET as isize) as *const u64).read(), DEAD_STATE);
        }
        assert!(!ctx.trapped());
        ctx.delete(p as usize, 6);
        assert!(ctx.trapped());
        let r = ctx.trap_record().expect("trap recorded");
        assert_eq!(r.kind, TrapKind::DoubleDelete);
        assert_eq!(r.pos_id, 6);
    }

    #[test]
    fn delete_of_unowned_pointer_traps() {
        let mut ctx = Context::new();
        ctx.delete(0x1000, 1);
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::InvalidDelete));
    }

    #[test]
    fn clear_trap_resets_reporting_state_and_nothing_else() {
        let mut ctx = Context::new();
        let kept = ctx.alloc(8, 1, 0);
        let deleted = ctx.alloc(8, 1, 0);
        ctx.delete(deleted as usize, 0);
        ctx.print_line(b"before");
        ctx.bump_reload_epoch();

        ctx.trap(TrapKind::EmptyPop, "pop() on an empty array", 3);
        assert!(ctx.trapped());
        ctx.clear_trap();

        // Reporting state is gone...
        assert!(!ctx.trapped());
        assert!(ctx.trap_record().is_none());
        assert_eq!(ctx.trap_flag, 0, "the offset-0 flag is the cleared bit");
        // ...and nothing else moved.
        assert!(ctx.is_live(kept as usize));
        assert!(!ctx.is_live(deleted as usize), "a deleted allocation stays dead");
        assert_eq!(ctx.reload_epoch(), 1, "staleness survives the clear");
        assert_eq!(ctx.stdout_bytes(), b"before\n");

        // A later fault records normally (the first-trap-wins rule is
        // per uncleared run, not per Context).
        ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 9);
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::DivisionByZero, 9))
        );
    }

    #[test]
    fn clear_trap_on_an_untrapped_context_is_a_no_op() {
        let mut ctx = Context::new();
        ctx.clear_trap();
        assert!(!ctx.trapped());
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn first_trap_wins() {
        let mut ctx = Context::new();
        ctx.trap(TrapKind::EmptyPop, "first", 1);
        ctx.trap(TrapKind::DivisionByZero, "second", 2);
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::EmptyPop);
        assert_eq!(r.pos_id, 1);
    }

    #[test]
    fn collect_frees_unreachable_and_keeps_rooted() {
        let mut ctx = Context::new();
        let kept = ctx.alloc(8, 1, 0);
        let dropped = ctx.alloc(8, 1, 0);
        let mut slot: usize = kept as usize;
        let slot_ptr: *mut usize = &mut slot;
        ctx.root_add(slot_ptr as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(kept as usize));
        assert!(!ctx.is_live(dropped as usize));
        // Dropping the last reference frees the rest on the next
        // collect. Written through the registered pointer — the same
        // way generated code updates its shadow slots.
        // SAFETY: `slot` is alive for the whole test.
        unsafe { slot_ptr.write(0) };
        ctx.collect();
        assert!(!ctx.is_live(kept as usize));
    }

    #[test]
    fn collect_traces_through_payload_words() {
        let mut ctx = Context::new();
        let inner = ctx.alloc(8, 1, 0);
        let outer = ctx.alloc(8, 1, 0);
        // outer.field0 = inner
        // SAFETY: outer payload is 8 writable bytes.
        unsafe { (outer as *mut usize).write(inner as usize) };
        let mut slot: usize = outer as usize;
        ctx.root_add(&mut slot as *mut usize as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(outer as usize));
        assert!(ctx.is_live(inner as usize));
    }

    #[test]
    fn root_ranges_scan_every_word() {
        // A two-word root range (e.g. a global FixedArray of two
        // references): both interior handles must survive collection.
        let mut ctx = Context::new();
        let a = ctx.alloc(8, 1, 0);
        let b = ctx.alloc(8, 1, 0);
        let range = [a as usize, b as usize];
        ctx.root_add(range.as_ptr() as usize, 2);
        ctx.collect();
        assert!(ctx.is_live(a as usize));
        assert!(ctx.is_live(b as usize));
    }

    #[test]
    fn shadow_frames_root_locals_and_pop_unroots_them() {
        let mut ctx = Context::new();
        let p = ctx.alloc(8, 1, 0);
        let slots = [p as usize];
        ctx.shadow_push(slots.as_ptr() as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(p as usize));
        ctx.shadow_pop();
        ctx.collect();
        assert!(!ctx.is_live(p as usize));
    }

    #[test]
    fn strings_alloc_read_and_intern() {
        let mut ctx = Context::new();
        let h = ctx.alloc_str(b"alpha-beta", 0);
        // SAFETY: h is a live string handle from this context.
        unsafe {
            assert_eq!(ctx.str_bytes(h), b"alpha-beta");
        }
        static LIT: &[u8] = b"hello";
        // SAFETY: LIT is 'static.
        let a = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        // SAFETY: as above.
        let b = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
        assert_eq!(a, b, "literal interning reuses one allocation");
        // Interned literals survive collection with no other roots.
        ctx.collect();
        assert!(ctx.is_live(a as usize));
    }

    #[test]
    fn arrays_push_index_pop_and_traps() {
        let mut ctx = Context::new();
        let h = ctx.array_new(4, 0);
        // SAFETY: h is a live array handle; sources/dests are valid.
        unsafe {
            assert_eq!(ctx.array_len(h), 0);
            for v in [10i32, 20, 30, 40, 50] {
                let n = ctx.array_push(h, &v as *const i32 as *const u8, 1);
                assert!(n > 0);
            }
            assert_eq!(ctx.array_len(h), 5);
            let p2 = ctx.array_elem_ptr(h, 2, 2);
            assert_eq!((p2 as *const i32).read(), 30);
            let mut out: i32 = 0;
            ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 3);
            assert_eq!(out, 50);
            assert_eq!(ctx.array_len(h), 4);
            // OOB traps and returns null.
            assert!(ctx.array_elem_ptr(h, 4, 9).is_null());
        }
        let r = ctx.trap_record().expect("oob trap");
        assert_eq!(r.kind, TrapKind::IndexOutOfBounds);
        assert_eq!(r.pos_id, 9);
    }

    #[test]
    fn empty_pop_traps() {
        let mut ctx = Context::new();
        let h = ctx.array_new(4, 0);
        let mut out: i32 = 0;
        // SAFETY: h is a live array handle; dst is valid.
        unsafe { ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 7) };
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::EmptyPop));
    }

    #[test]
    fn array_data_is_reached_by_conservative_marking() {
        let mut ctx = Context::new();
        let h = ctx.array_new(8, 0);
        let inner = ctx.alloc(8, 1, 0);
        // SAFETY: valid array handle and element source.
        unsafe {
            let v = inner as usize;
            ctx.array_push(h, &v as *const usize as *const u8, 0);
        }
        let mut slot: usize = h as usize;
        ctx.root_add(&mut slot as *mut usize as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(h as usize));
        assert!(ctx.is_live(inner as usize), "element reached via data pointer");
    }
}
