//! The allocation fact of each intrinsic operation (`compiler.md` §342
//! rule 10).
//!
//! An operation can allocate if a path of its runtime entry calls a Context
//! allocation, grows an array, a Map, or a Set, makes a string, boxes a
//! value, makes an Error, or runs script code. Each match is total, so a new
//! operation cannot exist without its fact. The interpreter verifies each
//! "cannot allocate" value at run time (`compiler.md` §342 rule 10a).

use super::{
    AmbientFn, ArrFn, ContextBytesFn, DateFn, JsonFn, MapFn, MathFn, NumFn, RegexFn, SetFn, StrFn,
    TextFn, WorkerFn,
};

impl AmbientFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // A collection and a release free storage; they do not allocate.
            Self::Print | Self::Unreachable | Self::Collect | Self::UnsafeDelete => false,
        }
    }
}

impl ContextBytesFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            Self::BytesOf => true,
            // The result of `fromBytes` is a value class or a fixed array.
            Self::BytesInto | Self::FromBytes => false,
        }
    }
}

impl MathFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            Self::Abs
            | Self::Acos
            | Self::Acosh
            | Self::Asin
            | Self::Asinh
            | Self::Atan
            | Self::Atanh
            | Self::Cbrt
            | Self::Ceil
            | Self::Cos
            | Self::Cosh
            | Self::Exp
            | Self::Expm1
            | Self::Floor
            | Self::Log
            | Self::Log1p
            | Self::Log10
            | Self::Log2
            | Self::Round
            | Self::Sign
            | Self::Sin
            | Self::Sinh
            | Self::Sqrt
            | Self::Tan
            | Self::Tanh
            | Self::Trunc
            | Self::Atan2
            | Self::Hypot
            | Self::Pow
            | Self::Max
            | Self::Min
            | Self::Random
            | Self::Clz32
            | Self::Imul
            | Self::Fround
            | Self::F32ToBits
            | Self::F32FromBits => false,
        }
    }
}

impl NumFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            Self::IsNaN
            | Self::IsFinite
            | Self::IsInteger
            | Self::IsSafeInteger
            | Self::ParseInt
            | Self::ParseFloat => false,
            Self::ToFixed
            | Self::ToStringF32
            | Self::ToStringF64
            | Self::ToExponential
            | Self::ToPrecision => true,
        }
    }
}

impl JsonFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // The builders, the parsed trees, and the mismatch record are
            // runtime tables outside the Context heap.
            Self::Begin
            | Self::BeginTracked
            | Self::Raw
            | Self::Str
            | Self::I32
            | Self::U32
            | Self::I64
            | Self::U64
            | Self::F32
            | Self::F64
            | Self::Bool
            | Self::Date
            | Self::Null
            | Self::Visit
            | Self::Leave
            | Self::ParseBegin
            | Self::ParseEnd
            | Self::ParseRoot
            | Self::ParseIsKind
            | Self::ParseNumberFits
            | Self::ParseNumber
            | Self::ParseInteger
            | Self::ParseBool
            | Self::ParseArrayLen
            | Self::ParseArrayGet
            | Self::ParseObjectGet
            | Self::ParseRecord
            | Self::ParseField
            | Self::ParseIndex
            | Self::ParseRelabel => false,
            Self::Finish | Self::ParseString | Self::ParseFailure | Self::ParseMismatch => true,
        }
    }
}

impl DateFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            Self::New
            | Self::Utc
            | Self::Now
            | Self::GetUtcFullYear
            | Self::GetUtcMonth
            | Self::GetUtcDate
            | Self::GetUtcDay
            | Self::GetUtcHours
            | Self::GetUtcMinutes
            | Self::GetUtcSeconds
            | Self::GetUtcMilliseconds => false,
            Self::ToIso | Self::ToUtcString => true,
        }
    }
}

impl StrFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            Self::IndexOf
            | Self::LastIndexOf
            | Self::Includes
            | Self::StartsWith
            | Self::EndsWith
            | Self::CharCodeAt
            | Self::CodePointAt
            | Self::GraphemeLength => false,
            Self::Slice
            | Self::Split
            | Self::Trim
            | Self::TrimStart
            | Self::TrimEnd
            | Self::Repeat
            | Self::PadStart
            | Self::PadEnd
            | Self::ToUpperCase
            | Self::ToLowerCase
            | Self::Replace
            | Self::ReplaceAll
            | Self::Substring
            | Self::Substr
            | Self::CharAt
            | Self::Concat
            | Self::At
            | Self::Normalize
            | Self::SliceGraphemes => true,
        }
    }
}

impl RegexFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // `source` and `flags` return the strings that `new` stored. The
            // last match is a runtime table outside the Context heap.
            Self::Test
            | Self::Source
            | Self::Flags
            | Self::Search
            | Self::MatchStart
            | Self::MatchEnd
            | Self::Global
            | Self::IgnoreCase
            | Self::Multiline
            | Self::DotAll
            | Self::Unicode
            | Self::HasIndices
            | Self::Sticky => false,
            Self::New | Self::Replace | Self::ReplaceAll | Self::Split | Self::ToString => true,
        }
    }
}

impl ArrFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // In-place operations and reads; `shift` truncates in place.
            Self::IndexOf
            | Self::LastIndexOf
            | Self::Includes
            | Self::Fill
            | Self::Reverse
            | Self::Shift
            | Self::CopyWithin
            | Self::At => false,
            // A fresh result, a growth, or a callback that runs script code.
            Self::Join
            | Self::Slice
            | Self::Concat
            | Self::ForEach
            | Self::Map
            | Self::Filter
            | Self::Reduce
            | Self::Some
            | Self::Every
            | Self::FindIndex
            | Self::Sort
            | Self::ReduceRight
            | Self::Splice
            | Self::Unshift
            | Self::Find
            | Self::FindLast
            | Self::FindLastIndex
            | Self::FlatMap => true,
        }
    }
}

impl MapFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // Growth and rehash occur only in an insertion.
            Self::Size | Self::Get | Self::GetOr | Self::Has | Self::Delete | Self::Clear => false,
            Self::New | Self::Set | Self::ForEach | Self::GroupBy => true,
        }
    }
}

impl SetFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // Growth and rehash occur only in an insertion.
            Self::Size
            | Self::Has
            | Self::Delete
            | Self::Clear
            | Self::IsSubsetOf
            | Self::IsSupersetOf
            | Self::IsDisjointFrom => false,
            Self::New
            | Self::Add
            | Self::ForEach
            | Self::Union
            | Self::Intersection
            | Self::Difference
            | Self::SymmetricDifference => true,
        }
    }
}

impl WorkerFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // A message crosses by a copy into a fresh allocation, and a
            // spawn and a join hold runtime state for the worker. A
            // receive allocates its task frame and can materialize a
            // message (compiler.md §349 rule 13).
            Self::Spawn(_)
            | Self::Post
            | Self::Poll
            | Self::Close
            | Self::Join
            | Self::InboxWait
            | Self::InboxPoll
            | Self::OutboxPost
            | Self::Receive => true,
        }
    }
}

impl TextFn {
    /// Returns true if the operation can allocate in the Context.
    #[must_use]
    pub fn can_allocate(self) -> bool {
        match self {
            // Each operation returns a fresh string.
            Self::ErrorToString
            | Self::EncodeUri
            | Self::EncodeComponent
            | Self::DecodeUri
            | Self::DecodeComponent
            | Self::UriFailure
            | Self::ComponentFailure => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_reads_cannot_allocate_and_fresh_results_can() {
        assert!(!MathFn::Sqrt.can_allocate());
        assert!(!StrFn::IndexOf.can_allocate());
        assert!(StrFn::Slice.can_allocate());
        assert!(!ArrFn::At.can_allocate());
        assert!(ArrFn::ForEach.can_allocate());
        assert!(!MapFn::Get.can_allocate());
        assert!(MapFn::Set.can_allocate());
        assert!(SetFn::Add.can_allocate());
        assert!(!JsonFn::I32.can_allocate());
        assert!(JsonFn::Finish.can_allocate());
        assert!(TextFn::EncodeUri.can_allocate());
        assert!(WorkerFn::Spawn(0).can_allocate());
        assert!(WorkerFn::Receive.can_allocate());
        assert!(ContextBytesFn::BytesOf.can_allocate());
        assert!(!ContextBytesFn::FromBytes.can_allocate());
    }
}
