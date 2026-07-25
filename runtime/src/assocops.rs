//! `Map` / `Set` operations (stdlib.md §10, Q24): the shared hash
//! container implementation both execution tiers call through
//! `sub_rt_map_*` / `sub_rt_set_*`.
//!
//! A container is a Context-managed [`AssocHeader`] pointing at two more
//! Context allocations: an insertion-ordered entry vector and an
//! open-addressed bucket index. Entries store concrete key/value bytes
//! inline; the compiler supplies their monomorphized widths and the key
//! kind. Every key/value slot begins on an eight-byte boundary, so the
//! Context collector's conservative word scan sees managed handles in
//! container storage on both tiers.
//!
//! Growth and rehash happen only in [`insert`], reached only by script
//! `Map.set` / `Set.add`. Read, delete, iteration, and clear never
//! allocate. Deleting zeroes the entry bytes so removed managed values
//! stop retaining their referents; clear eagerly retires both backing
//! allocations.

use crate::context::{
    Context, CLASS_MAP, CLASS_MAP_DATA, CLASS_MAP_INDEX, CLASS_SET,
};
use crate::trap::TrapKind;

const EMPTY: u64 = 0;
const TOMBSTONE: u64 = u64::MAX;
const INITIAL_ORDER_CAP: usize = 4;
const INITIAL_BUCKET_CAP: usize = 8;
const ENTRY_PREFIX: usize = 16; // hash, active

/// The runtime equality/hash kind of a monomorphized key.
///
/// Codes are an ABI contract with `compiler::hir::AssocKeyKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyKind {
    /// Integer-like bits: sized integers, boolean, enum, and `Date`.
    Bits,
    /// IEEE `f32` under SameValueZero (Q24): every NaN payload is one
    /// key, and `-0` is stored, hashed and compared as `+0`.
    F32,
    /// IEEE `f64` under SameValueZero (Q24): every NaN payload is one
    /// key, and `-0` is stored, hashed and compared as `+0`.
    F64,
    /// A string handle, compared and hashed by UTF-8 content.
    Str,
    /// A reference-class handle, compared and hashed by identity.
    Ref,
}

impl KeyKind {
    /// Decodes the stable compiler/runtime ABI code.
    #[must_use]
    pub fn from_u32(value: u32) -> Option<KeyKind> {
        Some(match value {
            0 => KeyKind::Bits,
            1 => KeyKind::F32,
            2 => KeyKind::F64,
            3 => KeyKind::Str,
            4 => KeyKind::Ref,
            _ => return None,
        })
    }
}

/// Managed payload of a `Map` or `Set`.
///
/// All fields are eight bytes so both child handles are visible to the
/// collector's aligned word scan.
#[repr(C)]
pub(crate) struct AssocHeader {
    len: u64,
    order_len: u64,
    order_cap: u64,
    bucket_cap: u64,
    tombstones: u64,
    key_size: u64,
    value_size: u64,
    entry_stride: u64,
    key_kind: u64,
    entries: *mut u8,
    buckets: *mut u8,
    iteration_depth: u64,
}

fn round_word(value: usize) -> Option<usize> {
    value.checked_add(7).map(|v| v & !7)
}

fn entry_stride(key_size: usize, value_size: usize) -> Option<usize> {
    ENTRY_PREFIX
        .checked_add(round_word(key_size)?)
        .and_then(|v| v.checked_add(round_word(value_size)?))
}

fn shape_ok(kind: KeyKind, key_size: usize) -> bool {
    match kind {
        KeyKind::Bits => matches!(key_size, 1 | 2 | 4 | 8),
        KeyKind::F32 => key_size == 4,
        KeyKind::F64 | KeyKind::Str | KeyKind::Ref => key_size == 8,
    }
}

fn allocation_size(
    ctx: &mut Context,
    count: usize,
    stride: usize,
    pos_id: u32,
) -> Option<usize> {
    match count.checked_mul(stride) {
        Some(size) => Some(size),
        None => {
            ctx.trap(
                TrapKind::AllocationFailure,
                "Map/Set backing-storage size is not representable",
                pos_id,
            );
            None
        }
    }
}

/// Allocates an empty `Map`/`Set` header. Backing storage is lazy: the
/// first `set`/`add` performs the only permitted initial growth.
pub(crate) fn new(
    ctx: &mut Context,
    key_size: usize,
    value_size: usize,
    kind: KeyKind,
    is_set: bool,
    pos_id: u32,
) -> *mut u8 {
    let Some(stride) = entry_stride(key_size, value_size) else {
        ctx.trap(
            TrapKind::AllocationFailure,
            "Map/Set entry layout is not representable",
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if !shape_ok(kind, key_size) {
        ctx.trap(
            TrapKind::Internal,
            format!("Map/Set key kind {kind:?} with width {key_size} is invalid"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let class_id = if is_set { CLASS_SET } else { CLASS_MAP };
    let handle = ctx.alloc(std::mem::size_of::<AssocHeader>(), class_id, pos_id);
    if handle.is_null() {
        return handle;
    }
    // SAFETY: `handle` is a fresh allocation of exactly an
    // `AssocHeader` payload.
    unsafe {
        let h = &mut *handle.cast::<AssocHeader>();
        h.key_size = key_size as u64;
        h.value_size = value_size as u64;
        h.entry_stride = stride as u64;
        h.key_kind = kind as u64;
    }
    handle
}

/// Returns the active element count.
///
/// # Safety
///
/// `handle` is null or a live `AssocHeader` owned by `ctx`.
pub(crate) unsafe fn len(handle: *const u8) -> i32 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: caller contract.
    unsafe { (*handle.cast::<AssocHeader>()).len as i32 }
}

unsafe fn header<'a>(handle: *mut u8) -> Option<&'a mut AssocHeader> {
    if handle.is_null() {
        None
    } else {
        // SAFETY: callers of the module's operations guarantee a live
        // container handle; the returned borrow is kept within one
        // operation and re-established after any Context allocation.
        Some(unsafe { &mut *handle.cast::<AssocHeader>() })
    }
}

fn key_offset() -> usize {
    ENTRY_PREFIX
}

fn value_offset(h: &AssocHeader) -> usize {
    ENTRY_PREFIX + round_word(h.key_size as usize).unwrap_or(0)
}

unsafe fn entry_ptr(h: &AssocHeader, index: usize) -> *mut u8 {
    // SAFETY: callers ensure `index < order_cap` and the multiplication
    // was validated when storage was allocated.
    unsafe { h.entries.add(index * h.entry_stride as usize) }
}

unsafe fn entry_active(h: &AssocHeader, index: usize) -> bool {
    // SAFETY: `entry_ptr` contract; active is the second prefix word.
    unsafe { entry_ptr(h, index).add(8).cast::<u64>().read_unaligned() != 0 }
}

unsafe fn entry_hash(h: &AssocHeader, index: usize) -> u64 {
    // SAFETY: `entry_ptr` contract; hash is the first prefix word.
    unsafe { entry_ptr(h, index).cast::<u64>().read_unaligned() }
}

unsafe fn entry_key(h: &AssocHeader, index: usize) -> *const u8 {
    // SAFETY: `entry_ptr` contract and fixed entry layout.
    unsafe { entry_ptr(h, index).add(key_offset()) }
}

unsafe fn entry_value(h: &AssocHeader, index: usize) -> *const u8 {
    // SAFETY: `entry_ptr` contract and fixed entry layout.
    unsafe { entry_ptr(h, index).add(value_offset(h)) }
}

unsafe fn read_bits(ptr: *const u8, size: usize) -> u64 {
    // SAFETY: caller supplies a readable key slot of `size` bytes.
    unsafe {
        match size {
            1 => u64::from(ptr.read_unaligned()),
            2 => u64::from(ptr.cast::<u16>().read_unaligned()),
            4 => u64::from(ptr.cast::<u32>().read_unaligned()),
            8 => ptr.cast::<u64>().read_unaligned(),
            _ => 0,
        }
    }
}

fn mix64(mut value: u64) -> u64 {
    // MurmurHash3's 64-bit finalizer: deterministic, seed-free, and
    // avalanches narrow integer and pointer bit patterns well.
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const CANONICAL_F32_NAN_BITS: u32 = 0x7fc0_0000;
const CANONICAL_F64_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

fn f32_key_bits(value: f32) -> u32 {
    if value == 0.0 {
        0
    } else if value.is_nan() {
        CANONICAL_F32_NAN_BITS
    } else {
        value.to_bits()
    }
}

fn f64_key_bits(value: f64) -> u64 {
    if value == 0.0 {
        0
    } else if value.is_nan() {
        CANONICAL_F64_NAN_BITS
    } else {
        value.to_bits()
    }
}

unsafe fn hash_key(ctx: *mut Context, kind: KeyKind, key: *const u8, size: usize) -> u64 {
    match kind {
        KeyKind::Bits | KeyKind::Ref => {
            // SAFETY: caller guarantees a readable key slot.
            mix64(unsafe { read_bits(key, size) })
        }
        KeyKind::F32 => {
            // SAFETY: the validated shape is four readable bytes.
            let value = unsafe { key.cast::<f32>().read_unaligned() };
            mix64(u64::from(f32_key_bits(value)))
        }
        KeyKind::F64 => {
            // SAFETY: the validated shape is eight readable bytes.
            let value = unsafe { key.cast::<f64>().read_unaligned() };
            mix64(f64_key_bits(value))
        }
        KeyKind::Str => {
            // SAFETY: the key slot contains a string handle.
            let string = unsafe { key.cast::<*const u8>().read_unaligned() };
            if string.is_null() {
                return mix64(0);
            }
            // SAFETY: caller contract: string keys are live handles of
            // this Context.
            fnv1a(unsafe { (*ctx).str_bytes(string) })
        }
    }
}

unsafe fn keys_equal(
    ctx: *mut Context,
    kind: KeyKind,
    left: *const u8,
    right: *const u8,
    size: usize,
) -> bool {
    match kind {
        KeyKind::Bits | KeyKind::Ref => {
            // SAFETY: caller guarantees two readable key slots.
            unsafe { read_bits(left, size) == read_bits(right, size) }
        }
        // SAFETY: validated shapes are readable for the float width.
        KeyKind::F32 => unsafe {
            let a = left.cast::<f32>().read_unaligned();
            let b = right.cast::<f32>().read_unaligned();
            a == b || (a.is_nan() && b.is_nan())
        },
        // SAFETY: as above.
        KeyKind::F64 => unsafe {
            let a = left.cast::<f64>().read_unaligned();
            let b = right.cast::<f64>().read_unaligned();
            a == b || (a.is_nan() && b.is_nan())
        },
        KeyKind::Str => {
            // SAFETY: both key slots contain handles.
            let a = unsafe { left.cast::<*const u8>().read_unaligned() };
            let b = unsafe { right.cast::<*const u8>().read_unaligned() };
            if a.is_null() || b.is_null() {
                return a == b;
            }
            // SAFETY: both are live strings of this Context.
            unsafe { (*ctx).str_bytes(a) == (*ctx).str_bytes(b) }
        }
    }
}

/// Copies a new key into ordered storage, canonicalizing the
/// SameValueZero-equivalent float representations. This makes traversal
/// observe `+0` after insertion of `-0`, and gives every NaN payload one
/// stable stored representation.
///
/// # Safety
///
/// `src` and `dst` cover `size` bytes and do not overlap; `(kind, size)`
/// was validated when the container was created.
unsafe fn copy_stored_key(kind: KeyKind, src: *const u8, dst: *mut u8, size: usize) {
    match kind {
        KeyKind::F32 => {
            // SAFETY: the validated F32 shape covers four bytes.
            let value = unsafe { src.cast::<f32>().read_unaligned() };
            unsafe { dst.cast::<u32>().write_unaligned(f32_key_bits(value)) };
        }
        KeyKind::F64 => {
            // SAFETY: the validated F64 shape covers eight bytes.
            let value = unsafe { src.cast::<f64>().read_unaligned() };
            unsafe { dst.cast::<u64>().write_unaligned(f64_key_bits(value)) };
        }
        _ => {
            // SAFETY: caller contract.
            unsafe { std::ptr::copy_nonoverlapping(src, dst, size) };
        }
    }
}

struct Lookup {
    bucket: usize,
    entry: Option<usize>,
}

unsafe fn lookup(
    ctx: *mut Context,
    h: &AssocHeader,
    key: *const u8,
    hash: u64,
) -> Lookup {
    let cap = h.bucket_cap as usize;
    if cap == 0 || h.buckets.is_null() {
        return Lookup {
            bucket: 0,
            entry: None,
        };
    }
    let mut bucket = hash as usize & (cap - 1);
    let mut first_tombstone = None;
    for _ in 0..cap {
        // SAFETY: `bucket < cap`; bucket storage holds `u64` slots.
        let slot = unsafe { h.buckets.add(bucket * 8).cast::<u64>().read_unaligned() };
        if slot == EMPTY {
            return Lookup {
                bucket: first_tombstone.unwrap_or(bucket),
                entry: None,
            };
        }
        if slot == TOMBSTONE {
            first_tombstone.get_or_insert(bucket);
        } else {
            let entry = (slot - 1) as usize;
            // The hash check avoids string-content work on unrelated
            // entries.
            // SAFETY: active buckets contain valid entry indices.
            if unsafe { entry_hash(h, entry) } == hash
                && unsafe {
                    keys_equal(
                        ctx,
                        KeyKind::from_u32(h.key_kind as u32).unwrap_or(KeyKind::Bits),
                        entry_key(h, entry),
                        key,
                        h.key_size as usize,
                    )
                }
            {
                return Lookup {
                    bucket,
                    entry: Some(entry),
                };
            }
        }
        bucket = (bucket + 1) & (cap - 1);
    }
    Lookup {
        bucket: first_tombstone.unwrap_or(0),
        entry: None,
    }
}

unsafe fn grow_entries(ctx: &mut Context, handle: *mut u8, new_cap: usize, pos_id: u32) -> bool {
    // SAFETY: caller contract.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    let Some(size) = allocation_size(ctx, new_cap, h.entry_stride as usize, pos_id) else {
        return false;
    };
    let data = ctx.alloc(size, CLASS_MAP_DATA, pos_id);
    if data.is_null() {
        return false;
    }
    let old = h.entries;
    let old_bytes = h.order_len as usize * h.entry_stride as usize;
    if !old.is_null() && old_bytes != 0 {
        // SAFETY: old storage has `order_cap * stride` bytes; new
        // storage is larger; initialized prefix is `order_len * stride`.
        unsafe { std::ptr::copy_nonoverlapping(old, data, old_bytes) };
        ctx.delete(old as usize, pos_id);
    }
    // Re-borrow after Context mutation.
    // SAFETY: container remains live.
    let h = unsafe { &mut *handle.cast::<AssocHeader>() };
    h.entries = data;
    h.order_cap = new_cap as u64;
    true
}

unsafe fn rehash(ctx: &mut Context, handle: *mut u8, new_cap: usize, pos_id: u32) -> bool {
    let Some(bytes) = allocation_size(ctx, new_cap, 8, pos_id) else {
        return false;
    };
    let buckets = ctx.alloc(bytes, CLASS_MAP_INDEX, pos_id);
    if buckets.is_null() {
        return false;
    }
    // SAFETY: container remains live; new bucket allocation is zeroed.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    for entry in 0..h.order_len as usize {
        // SAFETY: entry is inside the ordered prefix.
        if !unsafe { entry_active(h, entry) } {
            continue;
        }
        // SAFETY: active entry prefix is readable.
        let hash = unsafe { entry_hash(h, entry) };
        let mut bucket = hash as usize & (new_cap - 1);
        loop {
            // SAFETY: bucket is masked into the allocation.
            let slot = unsafe { buckets.add(bucket * 8).cast::<u64>() };
            // SAFETY: slot is readable.
            if unsafe { slot.read_unaligned() } == EMPTY {
                // SAFETY: slot is writable.
                unsafe { slot.write_unaligned(entry as u64 + 1) };
                break;
            }
            bucket = (bucket + 1) & (new_cap - 1);
        }
    }
    let old = h.buckets;
    if !old.is_null() {
        ctx.delete(old as usize, pos_id);
    }
    // SAFETY: re-borrow after Context mutation.
    let h = unsafe { &mut *handle.cast::<AssocHeader>() };
    h.buckets = buckets;
    h.bucket_cap = new_cap as u64;
    h.tombstones = 0;
    true
}

/// Stable-packs active entries and rebuilds the existing bucket index.
///
/// No allocation is needed: destinations precede their sources, and
/// the bucket block is cleared before active entries are re-indexed.
///
/// # Safety
///
/// `handle` is a live container with valid entry and bucket storage,
/// and no `forEach` is active.
unsafe fn compact_entries(handle: *mut u8) -> bool {
    // SAFETY: caller contract.
    let h = unsafe { &mut *handle.cast::<AssocHeader>() };
    if h.iteration_depth != 0 || h.order_len == h.len {
        return false;
    }
    if h.bucket_cap == 0 || h.buckets.is_null() {
        return false;
    }

    let old_len = h.order_len as usize;
    let stride = h.entry_stride as usize;
    let bucket_cap = h.bucket_cap as usize;
    // SAFETY: the index allocation has `bucket_cap * 8` bytes.
    unsafe { std::ptr::write_bytes(h.buckets, 0, bucket_cap * 8) };

    let mut write = 0usize;
    for read in 0..old_len {
        // SAFETY: read is inside the ordered prefix.
        if !unsafe { entry_active(h, read) } {
            continue;
        }
        if write != read {
            // SAFETY: distinct entry slots do not overlap. Stable packing
            // always moves toward the front, so unread sources remain
            // intact.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    entry_ptr(h, read),
                    entry_ptr(h, write),
                    stride,
                )
            };
        }
        // SAFETY: the packed entry prefix is readable.
        let hash = unsafe { entry_hash(h, write) };
        let mut bucket = hash as usize & (bucket_cap - 1);
        loop {
            // SAFETY: bucket is masked inside the zeroed index block.
            let slot = unsafe { h.buckets.add(bucket * 8).cast::<u64>() };
            // SAFETY: slot is readable.
            if unsafe { slot.read_unaligned() } == EMPTY {
                // SAFETY: slot is writable.
                unsafe { slot.write_unaligned(write as u64 + 1) };
                break;
            }
            bucket = (bucket + 1) & (bucket_cap - 1);
        }
        write += 1;
    }

    if write < old_len {
        // SAFETY: the packed prefix ends at `write`; zeroing the old
        // suffix removes duplicate managed handles left by moves.
        unsafe {
            std::ptr::write_bytes(
                entry_ptr(h, write),
                0,
                (old_len - write) * stride,
            )
        };
    }
    h.order_len = write as u64;
    h.tombstones = 0;
    true
}

unsafe fn ensure_capacity(ctx: &mut Context, handle: *mut u8, pos_id: u32) -> bool {
    // Entry-vector growth is append-driven, including after deletes:
    // re-insertion must append rather than reuse a removed position.
    // At the growth boundary, stable-pack deleted slots when no
    // iteration is active; moving entries during forEach would skip or
    // repeat callbacks after the loop advances its ordered index.
    // SAFETY: caller contract.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    if h.order_len == h.order_cap && h.iteration_depth == 0 && h.len < h.order_len {
        // SAFETY: the container is live and no iteration is active.
        let _ = unsafe { compact_entries(handle) };
    }
    // Re-borrow after possible compaction.
    // SAFETY: container remains live.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    if h.order_len == h.order_cap {
        let new_cap = if h.order_cap == 0 {
            INITIAL_ORDER_CAP
        } else {
            match (h.order_cap as usize).checked_mul(2) {
                Some(v) => v,
                None => {
                    ctx.trap(
                        TrapKind::AllocationFailure,
                        "Map/Set ordered capacity overflow",
                        pos_id,
                    );
                    return false;
                }
            }
        };
        // SAFETY: container is live.
        if !unsafe { grow_entries(ctx, handle, new_cap, pos_id) } {
            return false;
        }
    }
    // Re-borrow after possible growth.
    // At 75% used-or-tombstoned, rebuild. Grow only when active entries
    // themselves need room; tombstone pressure rehashes at the same cap.
    // SAFETY: container remains live.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    let used = h.len.saturating_add(h.tombstones).saturating_add(1);
    if h.bucket_cap == 0 || used.saturating_mul(4) >= h.bucket_cap.saturating_mul(3) {
        let new_cap = if h.bucket_cap == 0 {
            INITIAL_BUCKET_CAP
        } else if h.len.saturating_add(1).saturating_mul(4)
            >= h.bucket_cap.saturating_mul(3)
        {
            match (h.bucket_cap as usize).checked_mul(2) {
                Some(v) => v,
                None => {
                    ctx.trap(
                        TrapKind::AllocationFailure,
                        "Map/Set bucket capacity overflow",
                        pos_id,
                    );
                    return false;
                }
            }
        } else {
            h.bucket_cap as usize
        };
        // SAFETY: container is live.
        if !unsafe { rehash(ctx, handle, new_cap, pos_id) } {
            return false;
        }
    }
    true
}

/// Inserts or overwrites one entry. Returns the receiver.
///
/// # Safety
///
/// `handle` is a live container; `key` and (for a map) `value` are
/// readable for the monomorphized widths stored in its header.
pub(crate) unsafe fn insert(
    ctx: *mut Context,
    handle: *mut u8,
    key: *const u8,
    value: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if handle.is_null() || key.is_null() || unsafe { (*ctx).trapped() } {
        return handle;
    }
    // SAFETY: caller contract.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    let kind = KeyKind::from_u32(h.key_kind as u32).unwrap_or(KeyKind::Bits);
    // SAFETY: key is readable for key_size.
    let hash = unsafe { hash_key(ctx, kind, key, h.key_size as usize) };
    // SAFETY: header/index are valid.
    let found = unsafe { lookup(ctx, h, key, hash) };
    if let Some(entry) = found.entry {
        if h.value_size != 0 && !value.is_null() {
            // SAFETY: destination and source cover `value_size` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    value,
                    entry_value(h, entry) as *mut u8,
                    h.value_size as usize,
                );
            }
        }
        return handle;
    }
    // SAFETY: container is live; only this path allocates.
    if !unsafe { ensure_capacity(&mut *ctx, handle, pos_id) } {
        return handle;
    }
    // Recompute lookup after a possible rehash.
    // SAFETY: container remains live.
    let h = unsafe { &mut *handle.cast::<AssocHeader>() };
    // SAFETY: valid rebuilt index.
    let vacant = unsafe { lookup(ctx, h, key, hash) };
    let entry = h.order_len as usize;
    // SAFETY: capacity was ensured.
    let ep = unsafe { entry_ptr(h, entry) };
    // SAFETY: entry prefix and key/value slots are writable.
    unsafe {
        ep.cast::<u64>().write_unaligned(hash);
        ep.add(8).cast::<u64>().write_unaligned(1);
        copy_stored_key(
            kind,
            key,
            ep.add(key_offset()),
            h.key_size as usize,
        );
        if h.value_size != 0 && !value.is_null() {
            std::ptr::copy_nonoverlapping(
                value,
                ep.add(value_offset(h)),
                h.value_size as usize,
            );
        }
        let slot = h.buckets.add(vacant.bucket * 8).cast::<u64>();
        if slot.read_unaligned() == TOMBSTONE {
            h.tombstones = h.tombstones.saturating_sub(1);
        }
        slot.write_unaligned(entry as u64 + 1);
    }
    h.order_len += 1;
    h.len += 1;
    handle
}

unsafe fn find_entry(ctx: *mut Context, handle: *mut u8, key: *const u8) -> Option<usize> {
    let h = unsafe { header(handle)? };
    if key.is_null() || h.bucket_cap == 0 {
        return None;
    }
    let kind = KeyKind::from_u32(h.key_kind as u32)?;
    // SAFETY: caller supplies a readable key.
    let hash = unsafe { hash_key(ctx, kind, key, h.key_size as usize) };
    // SAFETY: valid container storage.
    unsafe { lookup(ctx, h, key, hash).entry }
}

/// Copies the found map value to `out`; returns whether it existed.
///
/// # Safety
///
/// `handle` is a live map; `key` is readable for its key width and
/// `out` is writable for its value width.
pub(crate) unsafe fn get(
    ctx: *mut Context,
    handle: *mut u8,
    key: *const u8,
    out: *mut u8,
) -> bool {
    // SAFETY: caller contract.
    let Some(entry) = (unsafe { find_entry(ctx, handle, key) }) else {
        return false;
    };
    // SAFETY: live map header and active entry.
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    if h.value_size != 0 && !out.is_null() {
        // SAFETY: source/destination cover value_size bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(entry_value(h, entry), out, h.value_size as usize);
        }
    }
    true
}

/// Copies the found value or `fallback` to `out`.
///
/// # Safety
///
/// As [`get`], and `fallback` is readable for the value width.
/// `fallback_size` supplies that width when `handle` is null.
pub(crate) unsafe fn get_or(
    ctx: *mut Context,
    handle: *mut u8,
    key: *const u8,
    fallback: *const u8,
    out: *mut u8,
    fallback_size: usize,
) {
    // SAFETY: forwarded contract.
    if unsafe { get(ctx, handle, key, out) } {
        return;
    }
    if fallback.is_null() || out.is_null() {
        return;
    }
    let size = if handle.is_null() {
        fallback_size
    } else {
        // SAFETY: caller contract.
        unsafe { (*handle.cast::<AssocHeader>()).value_size as usize }
    };
    if size != 0 {
        // SAFETY: fallback/out cover `size` bytes.
        unsafe { std::ptr::copy_nonoverlapping(fallback, out, size) };
    }
}

/// Returns whether a key is present.
///
/// # Safety
///
/// `handle` is a live container and `key` is readable for its key width.
pub(crate) unsafe fn has(ctx: *mut Context, handle: *mut u8, key: *const u8) -> bool {
    // SAFETY: caller contract.
    unsafe { find_entry(ctx, handle, key) }.is_some()
}

/// Deletes one key without allocation, zeroing the ordered entry so its
/// managed contents no longer retain referents.
///
/// # Safety
///
/// As [`has`].
pub(crate) unsafe fn delete(ctx: *mut Context, handle: *mut u8, key: *const u8) -> bool {
    let Some(h) = (unsafe { header(handle) }) else {
        return false;
    };
    if key.is_null() || h.bucket_cap == 0 {
        return false;
    }
    let Some(kind) = KeyKind::from_u32(h.key_kind as u32) else {
        return false;
    };
    // SAFETY: readable key and valid index.
    let hash = unsafe { hash_key(ctx, kind, key, h.key_size as usize) };
    // SAFETY: valid index.
    let found = unsafe { lookup(ctx, h, key, hash) };
    let Some(entry) = found.entry else {
        return false;
    };
    // SAFETY: bucket and entry lie inside their allocations.
    unsafe {
        h.buckets
            .add(found.bucket * 8)
            .cast::<u64>()
            .write_unaligned(TOMBSTONE);
        std::ptr::write_bytes(
            entry_ptr(h, entry),
            0,
            h.entry_stride as usize,
        );
    }
    h.len = h.len.saturating_sub(1);
    h.tombstones = h.tombstones.saturating_add(1);
    true
}

/// Eagerly retires a container's entry and bucket allocations and resets
/// it to the empty state. This is also called by `Context::delete` before
/// deleting a Map/Set header.
///
/// # Safety
///
/// `handle` is a live `AssocHeader` owned by `ctx`.
pub(crate) unsafe fn clear(ctx: &mut Context, handle: *mut u8) {
    let Some(h) = (unsafe { header(handle) }) else {
        return;
    };
    let entries = h.entries;
    let buckets = h.buckets;
    h.len = 0;
    h.order_len = 0;
    h.order_cap = 0;
    h.bucket_cap = 0;
    h.tombstones = 0;
    h.entries = std::ptr::null_mut();
    h.buckets = std::ptr::null_mut();
    if !entries.is_null() {
        ctx.delete(entries as usize, 0);
    }
    if !buckets.is_null() {
        ctx.delete(buckets as usize, 0);
    }
}

/// Fixed ABI of a generated Map callback bridge. The bridge loads
/// monomorphized value/key bytes and invokes the actual script function
/// under that tier's language calling convention.
type MapBridge = unsafe extern "C" fn(
    *mut Context,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
);

/// Fixed ABI of a generated Set callback bridge.
type SetBridge =
    unsafe extern "C" fn(*mut Context, *const u8, *const u8, *const u8);

/// Iterates a map in insertion order, checking the trap flag after every
/// callback. Mutation by a callback is observed like JS: removed entries
/// are skipped and entries appended before completion are visited.
///
/// # Safety
///
/// `handle` is a live map; `code` is a script callback pointer, `env` its
/// environment, and `bridge` has the [`MapBridge`] signature.
pub(crate) unsafe fn map_for_each(
    ctx: *mut Context,
    handle: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    if handle.is_null() || code.is_null() || bridge.is_null() || unsafe { (*ctx).trapped() } {
        return;
    }
    // SAFETY: caller guarantees the generated bridge signature.
    let call: MapBridge = unsafe { std::mem::transmute(bridge) };
    // SAFETY: the live header is writable for the synchronous iteration.
    unsafe {
        let h = &mut *handle.cast::<AssocHeader>();
        h.iteration_depth = h.iteration_depth.saturating_add(1);
    }
    let mut index = 0usize;
    loop {
        // Re-read after every callback: it may clear/grow the container.
        // SAFETY: container handle remains live for the duration of its
        // synchronous non-escaping callback.
        let h = unsafe { &*handle.cast::<AssocHeader>() };
        if index >= h.order_len as usize {
            break;
        }
        // SAFETY: index is inside ordered prefix.
        if unsafe { entry_active(h, index) } {
            // SAFETY: active entry pointers remain valid until the
            // bridge has copied them into the callback ABI; mutation
            // happens only inside that callback.
            unsafe {
                call(
                    ctx,
                    code,
                    env,
                    entry_value(h, index),
                    entry_key(h, index),
                )
            };
            // SAFETY: live Context.
            if unsafe { (*ctx).trapped() || !(*ctx).is_live(handle as usize) } {
                break;
            }
        }
        index += 1;
    }
    // A callback may delete the receiver. Development storage remains
    // poisoned and ship storage may be recycled, so touch the depth only
    // while this exact payload is still live.
    // SAFETY: live Context and, when true, a live container payload.
    if unsafe { (*ctx).is_live(handle as usize) } {
        // SAFETY: liveness check above.
        let h = unsafe { &mut *handle.cast::<AssocHeader>() };
        h.iteration_depth = h.iteration_depth.saturating_sub(1);
    }
}

/// Iterates a set in insertion order with the same mutation and trap
/// discipline as [`map_for_each`].
///
/// # Safety
///
/// As [`map_for_each`], with a [`SetBridge`].
pub(crate) unsafe fn set_for_each(
    ctx: *mut Context,
    handle: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    if handle.is_null() || code.is_null() || bridge.is_null() || unsafe { (*ctx).trapped() } {
        return;
    }
    // SAFETY: caller guarantees the generated bridge signature.
    let call: SetBridge = unsafe { std::mem::transmute(bridge) };
    // SAFETY: the live header is writable for the synchronous iteration.
    unsafe {
        let h = &mut *handle.cast::<AssocHeader>();
        h.iteration_depth = h.iteration_depth.saturating_add(1);
    }
    let mut index = 0usize;
    loop {
        // SAFETY: live container; re-read after mutation.
        let h = unsafe { &*handle.cast::<AssocHeader>() };
        if index >= h.order_len as usize {
            break;
        }
        // SAFETY: index is inside ordered prefix.
        if unsafe { entry_active(h, index) } {
            // SAFETY: bridge contract.
            unsafe { call(ctx, code, env, entry_key(h, index)) };
            // SAFETY: live Context.
            if unsafe { (*ctx).trapped() || !(*ctx).is_live(handle as usize) } {
                break;
            }
        }
        index += 1;
    }
    // SAFETY: as in map_for_each.
    if unsafe { (*ctx).is_live(handle as usize) } {
        // SAFETY: liveness check above.
        let h = unsafe { &mut *handle.cast::<AssocHeader>() };
        h.iteration_depth = h.iteration_depth.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn map_i32_bridge(
        ctx: *mut Context,
        code: *const u8,
        env: *const u8,
        value: *const u8,
        key: *const u8,
    ) {
        // SAFETY: test supplies this exact signature.
        let f: unsafe extern "C" fn(*mut Context, *const u8, i32, i32) =
            unsafe { std::mem::transmute(code) };
        // SAFETY: i32 test entries.
        unsafe {
            f(
                ctx,
                env,
                value.cast::<i32>().read_unaligned(),
                key.cast::<i32>().read_unaligned(),
            )
        };
    }

    unsafe extern "C" fn collect_pair(
        _ctx: *mut Context,
        env: *const u8,
        value: i32,
        key: i32,
    ) {
        // SAFETY: env points at the test's Vec for this synchronous call.
        unsafe { &mut *(env as *mut Vec<(i32, i32)>) }.push((key, value));
    }

    struct InsertDuringIteration {
        map: *mut u8,
        seen: Vec<i32>,
        inserted: bool,
    }

    unsafe extern "C" fn insert_after_deleted_prefix(
        ctx: *mut Context,
        env: *const u8,
        _value: i32,
        key: i32,
    ) {
        // SAFETY: env points at this synchronous test state.
        let state = unsafe { &mut *(env as *mut InsertDuringIteration) };
        state.seen.push(key);
        if key == 2 && !state.inserted {
            state.inserted = true;
            let added_key = 5i32;
            let added_value = 50i32;
            // SAFETY: the state holds the live i32/i32 map being
            // traversed, and both values are readable.
            unsafe {
                insert(
                    ctx,
                    state.map,
                    (&added_key as *const i32).cast(),
                    (&added_value as *const i32).cast(),
                    0,
                );
            }
        }
    }

    #[test]
    fn map_order_overwrite_delete_and_reinsert() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
        for (k, v) in [(2i32, 20i32), (1, 10), (3, 30)] {
            // SAFETY: h is an i32/i32 map and pointers are readable.
            unsafe {
                insert(
                    &mut *ctx,
                    h,
                    (&k as *const i32).cast(),
                    (&v as *const i32).cast(),
                    0,
                );
            }
        }
        let overwrite = 22i32;
        let two = 2i32;
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&two as *const i32).cast(),
                (&overwrite as *const i32).cast(),
                0,
            );
        }
        let one = 1i32;
        // SAFETY: matching map shape.
        assert!(unsafe { delete(&mut *ctx, h, (&one as *const i32).cast()) });
        let reinsert = 11i32;
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&one as *const i32).cast(),
                (&reinsert as *const i32).cast(),
                0,
            );
        }
        let four = 4i32;
        let forty = 40i32;
        // This insertion reaches a full ordered vector containing one
        // deleted slot, so it stable-compacts before appending.
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&four as *const i32).cast(),
                (&forty as *const i32).cast(),
                0,
            );
        }
        let mut seen = Vec::new();
        // SAFETY: bridge and callback have the test's fixed signatures.
        unsafe {
            map_for_each(
                &mut *ctx,
                h,
                collect_pair as *const u8,
                (&mut seen as *mut Vec<(i32, i32)>).cast(),
                map_i32_bridge as *const u8,
            );
        }
        assert_eq!(seen, vec![(2, 22), (3, 30), (1, 11), (4, 40)]);
    }

    #[test]
    fn churn_compacts_ordered_storage_at_the_growth_boundary() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
        for key in 0i32..200_000 {
            let value = key;
            // SAFETY: h is an i32/i32 map and pointers are readable.
            unsafe {
                insert(
                    &mut *ctx,
                    h,
                    (&key as *const i32).cast(),
                    (&value as *const i32).cast(),
                    0,
                );
            }
            // SAFETY: matching map shape.
            assert!(unsafe { delete(&mut *ctx, h, (&key as *const i32).cast()) });
        }
        // SAFETY: h remains a live map header.
        let header = unsafe { &*h.cast::<AssocHeader>() };
        assert_eq!(header.len, 0);
        assert_eq!(header.order_len, 4);
        assert_eq!(header.order_cap, INITIAL_ORDER_CAP as u64);
        assert_eq!(header.bucket_cap, INITIAL_BUCKET_CAP as u64);
        assert_eq!(
            header.order_cap * header.entry_stride,
            128,
            "empty i32/i32 map must retain only its four-entry block"
        );
    }

    #[test]
    fn compaction_is_suppressed_during_for_each_mutation() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
        for key in 1i32..=4 {
            let value = key * 10;
            // SAFETY: h is an i32/i32 map and pointers are readable.
            unsafe {
                insert(
                    &mut *ctx,
                    h,
                    (&key as *const i32).cast(),
                    (&value as *const i32).cast(),
                    0,
                );
            }
        }
        let deleted = 1i32;
        // Leave a deleted slot before the first callback. Compacting
        // when key 2 inserts would shift key 3 behind the loop cursor.
        // SAFETY: matching map shape.
        assert!(unsafe { delete(&mut *ctx, h, (&deleted as *const i32).cast()) });
        let mut state = InsertDuringIteration {
            map: h,
            seen: Vec::new(),
            inserted: false,
        };
        // SAFETY: bridge, callback, and environment use the test's fixed
        // signatures for this synchronous iteration.
        unsafe {
            map_for_each(
                &mut *ctx,
                h,
                insert_after_deleted_prefix as *const u8,
                (&mut state as *mut InsertDuringIteration).cast(),
                map_i32_bridge as *const u8,
            );
        }
        assert_eq!(state.seen, vec![2, 3, 4, 5]);
        // SAFETY: h remains live.
        let header = unsafe { &*h.cast::<AssocHeader>() };
        assert_eq!(header.order_len, 5);
        assert_eq!(header.order_cap, 8);
        assert_eq!(header.iteration_depth, 0);
    }

    #[test]
    fn float_keys_use_same_value_zero_and_normalize_storage() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 8, 4, KeyKind::F64, false, 0);
        let neg_zero = -0.0f64;
        let value = 7i32;
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&neg_zero as *const f64).cast(),
                (&value as *const i32).cast(),
                0,
            );
        }
        let pos_zero = 0.0f64;
        // SAFETY: matching map shape.
        assert!(unsafe { has(&mut *ctx, h, (&pos_zero as *const f64).cast()) });
        // SAFETY: the first ordered entry is active f64 key storage.
        assert_eq!(
            unsafe { entry_key(&*h.cast::<AssocHeader>(), 0).cast::<u64>().read_unaligned() },
            0,
            "-0 must be stored as +0 for traversal"
        );

        let nan_a = f64::from_bits(0x7ff0_0000_0000_0001);
        let nan_b = f64::from_bits(0xfff8_0000_0000_0042);
        assert!(nan_a.is_nan() && nan_b.is_nan());
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&nan_a as *const f64).cast(),
                (&value as *const i32).cast(),
                0,
            );
        }
        // SAFETY: a distinct NaN payload is the same key.
        assert!(unsafe { has(&mut *ctx, h, (&nan_b as *const f64).cast()) });
        let replacement = 9i32;
        // SAFETY: matching map shape; this overwrites the NaN entry.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&nan_b as *const f64).cast(),
                (&replacement as *const i32).cast(),
                0,
            );
        }
        let mut out = 0i32;
        // SAFETY: matching map shape and writable output.
        assert!(unsafe {
            get(
                &mut *ctx,
                h,
                (&nan_a as *const f64).cast(),
                (&mut out as *mut i32).cast(),
            )
        });
        assert_eq!(out, replacement);
        // One zero and one NaN are stored; the NaN is canonicalized.
        // SAFETY: live header.
        assert_eq!(unsafe { len(h) }, 2);
        assert_eq!(
            unsafe { entry_key(&*h.cast::<AssocHeader>(), 1).cast::<u64>().read_unaligned() },
            CANONICAL_F64_NAN_BITS
        );

        let f32_map = new(&mut ctx, 4, 4, KeyKind::F32, false, 0);
        let f32_nan_a = f32::from_bits(0x7f80_0001);
        let f32_nan_b = f32::from_bits(0xffc0_0042);
        // SAFETY: matching f32/i32 map shape.
        unsafe {
            insert(
                &mut *ctx,
                f32_map,
                (&f32_nan_a as *const f32).cast(),
                (&value as *const i32).cast(),
                0,
            );
        }
        // SAFETY: a distinct f32 NaN payload is the same key.
        assert!(unsafe { has(&mut *ctx, f32_map, (&f32_nan_b as *const f32).cast()) });
        assert_eq!(unsafe { len(f32_map) }, 1);
        assert_eq!(
            unsafe {
                entry_key(&*f32_map.cast::<AssocHeader>(), 0)
                    .cast::<u32>()
                    .read_unaligned()
            },
            CANONICAL_F32_NAN_BITS
        );
    }

    #[test]
    fn string_keys_use_content() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 8, 4, KeyKind::Str, false, 0);
        let a = ctx.alloc_str(b"same", 0);
        let b = ctx.alloc_str(b"same", 0);
        assert_ne!(a, b);
        let value = 9i32;
        // SAFETY: matching map shape and live string handles.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&a as *const *mut u8).cast(),
                (&value as *const i32).cast(),
                0,
            );
        }
        // SAFETY: b has equal content.
        assert!(unsafe { has(&mut *ctx, h, (&b as *const *mut u8).cast()) });
    }

    #[test]
    fn get_or_writes_fallback_for_a_null_handle() {
        let mut ctx = Context::new();
        let key = 1i32;
        let fallback = 77i32;
        let mut out = 0i32;
        // SAFETY: fallback_size describes the i32 fallback/output slots;
        // a null handle deliberately exercises the total fallback path.
        unsafe {
            get_or(
                &mut *ctx,
                std::ptr::null_mut(),
                (&key as *const i32).cast(),
                (&fallback as *const i32).cast(),
                (&mut out as *mut i32).cast(),
                std::mem::size_of::<i32>(),
            );
        }
        assert_eq!(out, fallback);
    }

    #[test]
    fn clear_retires_backing_storage() {
        let mut ctx = Context::new();
        let h = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
        let key = 1i32;
        let value = 2i32;
        // SAFETY: matching map shape.
        unsafe {
            insert(
                &mut *ctx,
                h,
                (&key as *const i32).cast(),
                (&value as *const i32).cast(),
                0,
            );
        }
        assert_eq!(ctx.live_count(), 3);
        // SAFETY: live map.
        unsafe { clear(&mut ctx, h) };
        assert_eq!(ctx.live_count(), 1);
        // SAFETY: live empty map.
        assert_eq!(unsafe { len(h) }, 0);
    }

    fn assert_collector_scans_container_storage(mut ctx: Box<Context>) {
        let map = new(&mut ctx, 8, 8, KeyKind::Str, false, 0);
        let key = ctx.alloc_str(b"key", 0);
        let value = ctx.alloc(8, 77, 0);
        // SAFETY: map stores a string handle and one reference handle.
        unsafe {
            insert(
                &mut *ctx,
                map,
                (&key as *const *mut u8).cast(),
                (&value as *const *mut u8).cast(),
                0,
            );
        }
        assert_eq!(ctx.live_count(), 5);

        let mut root = map as usize;
        ctx.root_add((&mut root as *mut usize) as usize, 1);
        ctx.collect();
        assert!(ctx.is_live(map as usize));
        assert!(ctx.is_live(key as usize));
        assert!(ctx.is_live(value as usize));
        assert_eq!(ctx.live_count(), 5);

        root = 0;
        // Read the assignment so this test also documents that it is the
        // registered root word, not a stale compiler temporary, that
        // controls reachability.
        assert_eq!(root, 0);
        ctx.collect();
        assert_eq!(ctx.live_count(), 0);
    }

    #[test]
    fn collector_scans_map_storage_in_dev_and_releasing_modes() {
        assert_collector_scans_container_storage(Context::new());
        assert_collector_scans_container_storage(Context::new_releasing());
    }

    #[test]
    fn deleting_container_retires_owned_storage_in_both_modes() {
        for mut ctx in [Context::new(), Context::new_releasing()] {
            let map = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
            let key = 1i32;
            let value = 2i32;
            // SAFETY: matching map shape.
            unsafe {
                insert(
                    &mut *ctx,
                    map,
                    (&key as *const i32).cast(),
                    (&value as *const i32).cast(),
                    0,
                );
            }
            assert_eq!(ctx.live_count(), 3);
            ctx.delete(map as usize, 1);
            assert_eq!(ctx.live_count(), 0);
        }
    }
}
