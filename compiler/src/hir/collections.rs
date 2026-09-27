use super::*;

impl ArrFn {
    /// Every accepted `Array` method, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [ArrFn; 26] = [
        ArrFn::IndexOf,
        ArrFn::LastIndexOf,
        ArrFn::Includes,
        ArrFn::Join,
        ArrFn::Slice,
        ArrFn::Fill,
        ArrFn::Reverse,
        ArrFn::Concat,
        ArrFn::ForEach,
        ArrFn::Map,
        ArrFn::Filter,
        ArrFn::Reduce,
        ArrFn::Some,
        ArrFn::Every,
        ArrFn::FindIndex,
        ArrFn::Sort,
        ArrFn::ReduceRight,
        ArrFn::Splice,
        ArrFn::Shift,
        ArrFn::Unshift,
        ArrFn::CopyWithin,
        ArrFn::At,
        ArrFn::Find,
        ArrFn::FindLast,
        ArrFn::FindLastIndex,
        ArrFn::FlatMap,
    ];

    /// The checker's spelling of a defaulted missing `end` argument of
    /// `slice`/`fill`/`copyWithin`: `i32::MAX`, which the runtime's JS clamp reduces
    /// to the length ("to the end"). An explicit `end` of this value
    /// means the same thing, so the sentinel is not observable.
    pub const END_SENTINEL: i64 = i32::MAX as i64;

    /// The lib member name (the checker's lookup and diagnostics).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ArrFn::At => "at",
            ArrFn::Find => "find",
            ArrFn::FindLast => "findLast",
            ArrFn::FindLastIndex => "findLastIndex",
            ArrFn::FlatMap => "flatMap",
            ArrFn::IndexOf => "indexOf",
            ArrFn::LastIndexOf => "lastIndexOf",
            ArrFn::Includes => "includes",
            ArrFn::Join => "join",
            ArrFn::Slice => "slice",
            ArrFn::Fill => "fill",
            ArrFn::Reverse => "reverse",
            ArrFn::Concat => "concat",
            ArrFn::ForEach => "forEach",
            ArrFn::Map => "map",
            ArrFn::Filter => "filter",
            ArrFn::Reduce => "reduce",
            ArrFn::Some => "some",
            ArrFn::Every => "every",
            ArrFn::FindIndex => "findIndex",
            ArrFn::Sort => "sort",
            ArrFn::ReduceRight => "reduceRight",
            ArrFn::Splice => "splice",
            ArrFn::Shift => "shift",
            ArrFn::Unshift => "unshift",
            ArrFn::CopyWithin => "copyWithin",
        }
    }

    /// The opaque runtime symbol both tiers call.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            ArrFn::At => "subscript_rt_arr_at",
            ArrFn::Find => "subscript_rt_arr_find",
            ArrFn::FindLast => "subscript_rt_arr_find_last",
            ArrFn::FindLastIndex => "subscript_rt_arr_find_last_index",
            ArrFn::FlatMap => "subscript_rt_arr_flat_map",
            ArrFn::IndexOf => "subscript_rt_arr_index_of",
            ArrFn::LastIndexOf => "subscript_rt_arr_last_index_of",
            ArrFn::Includes => "subscript_rt_arr_includes",
            ArrFn::Join => "subscript_rt_arr_join",
            ArrFn::Slice => "subscript_rt_arr_slice",
            ArrFn::Fill => "subscript_rt_arr_fill",
            ArrFn::Reverse => "subscript_rt_arr_reverse",
            ArrFn::Concat => "subscript_rt_arr_concat",
            ArrFn::ForEach => "subscript_rt_arr_for_each",
            ArrFn::Map => "subscript_rt_arr_map",
            ArrFn::Filter => "subscript_rt_arr_filter",
            ArrFn::Reduce => "subscript_rt_arr_reduce",
            ArrFn::Some => "subscript_rt_arr_some",
            ArrFn::Every => "subscript_rt_arr_every",
            ArrFn::FindIndex => "subscript_rt_arr_find_index",
            ArrFn::Sort => "subscript_rt_arr_sort",
            ArrFn::ReduceRight => "subscript_rt_arr_reduce_right",
            ArrFn::Splice => "subscript_rt_arr_splice",
            ArrFn::Shift => "subscript_rt_arr_shift",
            ArrFn::Unshift => "subscript_rt_arr_unshift",
            ArrFn::CopyWithin => "subscript_rt_arr_copy_within",
        }
    }

    /// The Q27 `FixedArray<T, N>` callback-family runtime symbol, when
    /// this operation is accepted on an in-place fixed buffer.
    #[must_use]
    pub fn fixed_symbol(self) -> Option<&'static str> {
        Some(match self {
            ArrFn::ForEach => "subscript_rt_fixed_arr_for_each",
            ArrFn::Map => "subscript_rt_fixed_arr_map",
            ArrFn::Filter => "subscript_rt_fixed_arr_filter",
            ArrFn::Reduce => "subscript_rt_fixed_arr_reduce",
            ArrFn::Some => "subscript_rt_fixed_arr_some",
            ArrFn::Every => "subscript_rt_fixed_arr_every",
            ArrFn::FindIndex => "subscript_rt_fixed_arr_find_index",
            ArrFn::ReduceRight => "subscript_rt_fixed_arr_reduce_right",
            _ => return None,
        })
    }

    /// True for the methods whose second HIR argument is a script
    /// callback (a `(code, env)` function value).
    #[must_use]
    pub fn takes_callback(self) -> bool {
        matches!(
            self,
            ArrFn::ForEach
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Reduce
                | ArrFn::ReduceRight
                | ArrFn::Some
                | ArrFn::Every
                | ArrFn::FindIndex
                | ArrFn::Find
                | ArrFn::FindLast
                | ArrFn::FindLastIndex
                | ArrFn::FlatMap
                | ArrFn::Sort
        )
    }

    /// Callback arity for the accepted form that includes the trailing
    /// element index, or `None` when this operation has no index callback
    /// form. `sort` is deliberately excluded: JavaScript calls its
    /// comparator with only the two compared values.
    #[must_use]
    pub fn callback_index_arity(self) -> Option<usize> {
        match self {
            ArrFn::ForEach
            | ArrFn::Map
            | ArrFn::Filter
            | ArrFn::Some
            | ArrFn::Every
            | ArrFn::FindIndex
            | ArrFn::Find
            | ArrFn::FindLast
            | ArrFn::FindLastIndex
            | ArrFn::FlatMap => Some(2),
            ArrFn::Reduce | ArrFn::ReduceRight => Some(3),
            _ => None,
        }
    }

    /// Whether the runtime symbol takes a trailing `pos_id`: the
    /// operations that allocate through the Context (a fresh array or
    /// string), plus `shift`, whose empty-receiver trap is at the call
    /// site.
    /// The callback-taking non-allocating operations surface only
    /// *callback* traps, which carry their own position.
    #[must_use]
    pub fn takes_pos_id(self) -> bool {
        matches!(
            self,
            ArrFn::At
                | ArrFn::FlatMap
                | ArrFn::Join
                | ArrFn::Slice
                | ArrFn::Concat
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Splice
                | ArrFn::Shift
                | ArrFn::Unshift
        )
    }

    /// Whether the generated call must be followed by a trap check:
    /// every operation that can leave the Context trapped (an
    /// allocation failure, or a script callback that trapped).
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.takes_callback() || self.takes_pos_id()
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            ArrFn::At => "at(index: i32): T",
            ArrFn::Find => "find(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): T | null",
            ArrFn::FindLast => "findLast(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): T | null",
            ArrFn::FindLastIndex => "findLastIndex(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): i32",
            ArrFn::FlatMap => "flatMap<U>(callback: ((value: T) => U[]) | ((value: T, index: i32) => U[])): U[]",
            ArrFn::IndexOf => "indexOf(value: T, fromIndex?: i32): i32",
            ArrFn::LastIndexOf => "lastIndexOf(value: T, fromIndex?: i32): i32",
            ArrFn::Includes => "includes(value: T, fromIndex?: i32): boolean",
            ArrFn::Join => "join(separator?: string): string",
            ArrFn::Slice => "slice(start?: i32, end?: i32): T[]",
            ArrFn::Fill => "fill(value: T, start?: i32, end?: i32): T[]",
            ArrFn::Reverse => "reverse(): T[]",
            ArrFn::Concat => "concat(other: T[]): T[]",
            ArrFn::ForEach => {
                "forEach(callback: ((value: T) => void) | ((value: T, index: i32) => void)): void"
            }
            ArrFn::Map => {
                "map<U>(callback: ((value: T) => U) | ((value: T, index: i32) => U)): U[]"
            }
            ArrFn::Filter => {
                "filter(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): T[]"
            }
            ArrFn::Reduce => {
                "reduce<U>(callback: ((acc: U, value: T) => U) | ((acc: U, value: T, index: i32) => U), init: U): U"
            }
            ArrFn::Some => {
                "some(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): boolean"
            }
            ArrFn::Every => {
                "every(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): boolean"
            }
            ArrFn::FindIndex => {
                "findIndex(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): i32"
            }
            ArrFn::Sort => "sort(comparator: (left: T, right: T) => i32): T[]",
            ArrFn::ReduceRight => {
                "reduceRight<U>(callback: ((acc: U, value: T) => U) | ((acc: U, value: T, index: i32) => U), init: U): U"
            }
            ArrFn::Splice => "splice(start: i32, deleteCount?: i32): T[]",
            ArrFn::Shift => "shift(): T",
            ArrFn::Unshift => "unshift(value: T): i32",
            ArrFn::CopyWithin => "copyWithin(target: i32, start: i32, end?: i32): T[]",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            ArrFn::At => "Reads a signed index; an out-of-range index traps.",
            ArrFn::Find => "Returns the first matching nullable-capable element, or null.",
            ArrFn::FindLast => "Returns the last matching nullable-capable element, or null.",
            ArrFn::FindLastIndex => "Returns the last matching index, or -1.",
            ArrFn::FlatMap => "Concatenates callback arrays at depth one.",
            ArrFn::IndexOf => "Returns the first `===`-equal element index, or -1.",
            ArrFn::LastIndexOf => "Returns the last `===`-equal element index, or -1.",
            ArrFn::Includes => "Uses SameValueZero equality.",
            ArrFn::Join => "Formats elements with the language interpolation rules.",
            ArrFn::Slice => "Returns a fresh range using JS clamp and negative-index rules.",
            ArrFn::Fill => "Stores one value across a range and returns the receiver.",
            ArrFn::Reverse => "Reverses in place and returns the receiver.",
            ArrFn::Concat => "Returns a fresh array from exactly one other array.",
            ArrFn::ForEach => "Calls a non-escaping callback with a value and optional index.",
            ArrFn::Map => "Maps through a non-escaping callback and infers `U`.",
            ArrFn::Filter => "Returns elements selected by a non-escaping callback.",
            ArrFn::Reduce => "Folds from a required initial accumulator.",
            ArrFn::Some => "Short-circuits on the first true callback result.",
            ArrFn::Every => "Short-circuits on the first false callback result.",
            ArrFn::FindIndex => "Returns the first matching callback index, or -1.",
            ArrFn::Sort => "Stable-sorts in place with a required comparator.",
            ArrFn::ReduceRight => "Folds right-to-left from a required initial accumulator.",
            ArrFn::Splice => "Deletes a clamped range in place and returns the removed elements.",
            ArrFn::Shift => "Removes the first element; an empty array traps.",
            ArrFn::Unshift => "Prepends one element and returns the new length.",
            ArrFn::CopyWithin => {
                "Copies a clamped range within the receiver and returns the receiver."
            }
        }
    }
}

impl AssocKeyKind {
    /// Stable runtime ABI code.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            AssocKeyKind::Bits => 0,
            AssocKeyKind::F32 => 1,
            AssocKeyKind::F64 => 2,
            AssocKeyKind::Str => 3,
            AssocKeyKind::Ref => 4,
        }
    }

    /// Returns the Q24 key kind of `ty`, or `None` when it is outside
    /// the whitelist. `is_value_class` supplies the program's nominal
    /// class-kind lookup.
    #[must_use]
    pub fn of(ty: &Type, is_value_class: &dyn Fn(ClassId) -> bool) -> Option<AssocKeyKind> {
        Some(match ty {
            Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::Bool
            | Type::Enum(_)
            | Type::Date => AssocKeyKind::Bits,
            Type::F32 => AssocKeyKind::F32,
            Type::F64 => AssocKeyKind::F64,
            Type::Str => AssocKeyKind::Str,
            Type::Class(id) if !is_value_class(*id) => AssocKeyKind::Ref,
            _ => return None,
        })
    }
}

impl MapFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [MapFn; 10] = [
        MapFn::New,
        MapFn::Size,
        MapFn::Get,
        MapFn::GetOr,
        MapFn::Set,
        MapFn::Has,
        MapFn::Delete,
        MapFn::Clear,
        MapFn::ForEach,
        MapFn::GroupBy,
    ];

    /// Surface spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            MapFn::New => "Map",
            MapFn::Size => "size",
            MapFn::Get => "get",
            MapFn::GetOr => "getOr",
            MapFn::Set => "set",
            MapFn::Has => "has",
            MapFn::Delete => "delete",
            MapFn::Clear => "clear",
            MapFn::ForEach => "forEach",
            MapFn::GroupBy => "groupBy",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            MapFn::New => "subscript_rt_map_new",
            MapFn::Size => "subscript_rt_assoc_size",
            MapFn::Get => "subscript_rt_map_get",
            MapFn::GetOr => "subscript_rt_map_get_or",
            MapFn::Set => "subscript_rt_map_set",
            MapFn::Has => "subscript_rt_assoc_has",
            MapFn::Delete => "subscript_rt_assoc_delete",
            MapFn::Clear => "subscript_rt_assoc_clear",
            MapFn::ForEach => "subscript_rt_map_for_each",
            MapFn::GroupBy => "subscript_rt_map_group_by",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(self, MapFn::New | MapFn::Set | MapFn::GroupBy)
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || matches!(self, MapFn::ForEach | MapFn::GroupBy)
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            MapFn::New => "new Map<K, V>(): Map<K, V>",
            MapFn::Size => "size: i32",
            MapFn::Get => "get(key: K): V | null",
            MapFn::GetOr => "getOr(key: K, fallback: V): V",
            MapFn::Set => "set(key: K, value: V): Map<K, V>",
            MapFn::Has => "has(key: K): boolean",
            MapFn::Delete => "delete(key: K): boolean",
            MapFn::Clear => "clear(): void",
            MapFn::ForEach => "forEach(callback: (value: V, key: K) => void): void",
            MapFn::GroupBy => "groupBy<K, T>(items: T[], callback: (value: T) => K): Map<K, T[]>",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            MapFn::New => "Constructs an empty insertion-ordered map.",
            MapFn::Size => "Returns the entry count as `i32`.",
            MapFn::Get => "Returns a nullable reference value; scalar values must use `getOr`.",
            MapFn::GetOr => "Returns the stored value or the explicit fallback.",
            MapFn::Set => "Stores a value and returns the receiver.",
            MapFn::Has => "Tests key presence with the key kind's equality.",
            MapFn::Delete => "Deletes a key and reports whether it was present.",
            MapFn::Clear => "Removes every entry.",
            MapFn::ForEach => "Traverses in insertion order with a fixed two-parameter callback.",
            MapFn::GroupBy => "Groups array values under whitelisted keys in first-seen key order.",
        }
    }
}

impl SetFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [SetFn; 14] = [
        SetFn::New,
        SetFn::Size,
        SetFn::Add,
        SetFn::Has,
        SetFn::Delete,
        SetFn::Clear,
        SetFn::ForEach,
        SetFn::Union,
        SetFn::Intersection,
        SetFn::Difference,
        SetFn::SymmetricDifference,
        SetFn::IsSubsetOf,
        SetFn::IsSupersetOf,
        SetFn::IsDisjointFrom,
    ];

    /// Surface spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SetFn::New => "Set",
            SetFn::Size => "size",
            SetFn::Add => "add",
            SetFn::Has => "has",
            SetFn::Delete => "delete",
            SetFn::Clear => "clear",
            SetFn::ForEach => "forEach",
            SetFn::Union => "union",
            SetFn::Intersection => "intersection",
            SetFn::Difference => "difference",
            SetFn::SymmetricDifference => "symmetricDifference",
            SetFn::IsSubsetOf => "isSubsetOf",
            SetFn::IsSupersetOf => "isSupersetOf",
            SetFn::IsDisjointFrom => "isDisjointFrom",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            SetFn::New => "subscript_rt_set_new",
            SetFn::Size => "subscript_rt_assoc_size",
            SetFn::Add => "subscript_rt_set_add",
            SetFn::Has => "subscript_rt_assoc_has",
            SetFn::Delete => "subscript_rt_assoc_delete",
            SetFn::Clear => "subscript_rt_assoc_clear",
            SetFn::ForEach => "subscript_rt_set_for_each",
            SetFn::Union => "subscript_rt_set_union",
            SetFn::Intersection => "subscript_rt_set_intersection",
            SetFn::Difference => "subscript_rt_set_difference",
            SetFn::SymmetricDifference => "subscript_rt_set_symmetric_difference",
            SetFn::IsSubsetOf => "subscript_rt_set_is_subset_of",
            SetFn::IsSupersetOf => "subscript_rt_set_is_superset_of",
            SetFn::IsDisjointFrom => "subscript_rt_set_is_disjoint_from",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(
            self,
            SetFn::New
                | SetFn::Add
                | SetFn::Union
                | SetFn::Intersection
                | SetFn::Difference
                | SetFn::SymmetricDifference
        )
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || self == SetFn::ForEach
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            SetFn::New => "new Set<K>(): Set<K>",
            SetFn::Size => "size: i32",
            SetFn::Add => "add(key: K): Set<K>",
            SetFn::Has => "has(key: K): boolean",
            SetFn::Delete => "delete(key: K): boolean",
            SetFn::Clear => "clear(): void",
            SetFn::ForEach => "forEach(callback: (key: K) => void): void",
            SetFn::Union => "union(other: Set<K>): Set<K>",
            SetFn::Intersection => "intersection(other: Set<K>): Set<K>",
            SetFn::Difference => "difference(other: Set<K>): Set<K>",
            SetFn::SymmetricDifference => "symmetricDifference(other: Set<K>): Set<K>",
            SetFn::IsSubsetOf => "isSubsetOf(other: Set<K>): boolean",
            SetFn::IsSupersetOf => "isSupersetOf(other: Set<K>): boolean",
            SetFn::IsDisjointFrom => "isDisjointFrom(other: Set<K>): boolean",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            SetFn::New => "Constructs an empty insertion-ordered set.",
            SetFn::Size => "Returns the entry count as `i32`.",
            SetFn::Add => "Adds a key and returns the receiver.",
            SetFn::Has => "Tests key presence with the key kind's equality.",
            SetFn::Delete => "Deletes a key and reports whether it was present.",
            SetFn::Clear => "Removes every entry.",
            SetFn::ForEach => "Traverses in insertion order with a fixed one-parameter callback.",
            SetFn::Union => "Returns a fresh union in ES2024 result order.",
            SetFn::Intersection => "Returns a fresh intersection in ES2024 result order.",
            SetFn::Difference => "Returns a fresh receiver-minus-argument set.",
            SetFn::SymmetricDifference => {
                "Returns a fresh symmetric difference in receiver-then-argument order."
            }
            SetFn::IsSubsetOf => "Tests whether every receiver key is in the argument.",
            SetFn::IsSupersetOf => "Tests whether every argument key is in the receiver.",
            SetFn::IsDisjointFrom => "Tests whether the sets have no common key.",
        }
    }
}

impl ArrElemKind {
    /// The stable `u32` code passed to the runtime.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            ArrElemKind::Int => 0,
            ArrElemKind::F32 => 1,
            ArrElemKind::F64 => 2,
            ArrElemKind::Str => 3,
            ArrElemKind::F16 => 4,
            ArrElemKind::SignedInt => 5,
        }
    }

    /// The kind of element type `ty`, or `None` when the type cannot
    /// cross the runtime↔script element boundary (value classes,
    /// function values, `FixedArray`, `void`). `is_value_class`
    /// distinguishes value classes (excluded) from reference classes
    /// (identity — included).
    #[must_use]
    pub fn of(ty: &Type, is_value_class: &dyn Fn(ClassId) -> bool) -> Option<ArrElemKind> {
        Some(match ty {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Object
            | Type::Array(_)
            | Type::Map(..)
            | Type::Set(_) => ArrElemKind::Int,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Enum(_) | Type::Date => {
                ArrElemKind::SignedInt
            }
            Type::Class(id) if !is_value_class(*id) => ArrElemKind::Int,
            Type::Nullable(inner) if !matches!(**inner, Type::Func(_)) => ArrElemKind::Int,
            Type::F32 => ArrElemKind::F32,
            Type::F64 => ArrElemKind::F64,
            Type::F16 => ArrElemKind::F16,
            Type::Str => ArrElemKind::Str,
            _ => return None,
        })
    }
}

impl ArrFmtKind {
    /// The stable `u32` code passed to the runtime.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            ArrFmtKind::I32 => 0,
            ArrFmtKind::U32 => 1,
            ArrFmtKind::I64 => 2,
            ArrFmtKind::U64 => 3,
            ArrFmtKind::F32 => 4,
            ArrFmtKind::F64 => 5,
            ArrFmtKind::Bool => 6,
            ArrFmtKind::Str => 7,
            ArrFmtKind::I8 => 8,
            ArrFmtKind::U8 => 9,
            ArrFmtKind::I16 => 10,
            ArrFmtKind::U16 => 11,
            ArrFmtKind::F16 => 12,
        }
    }

    /// The formatting kind of element type `ty`, or `None` when `ty`
    /// is not interpolatable under Q14.
    #[must_use]
    pub fn of(ty: &Type) -> Option<ArrFmtKind> {
        Some(match ty {
            Type::I8 => ArrFmtKind::I8,
            Type::U8 => ArrFmtKind::U8,
            Type::I16 => ArrFmtKind::I16,
            Type::U16 => ArrFmtKind::U16,
            Type::I32 | Type::Enum(_) => ArrFmtKind::I32,
            Type::U32 => ArrFmtKind::U32,
            Type::I64 => ArrFmtKind::I64,
            Type::U64 => ArrFmtKind::U64,
            Type::F32 => ArrFmtKind::F32,
            Type::F64 => ArrFmtKind::F64,
            Type::F16 => ArrFmtKind::F16,
            Type::Bool => ArrFmtKind::Bool,
            Type::Str => ArrFmtKind::Str,
            _ => return None,
        })
    }
}
