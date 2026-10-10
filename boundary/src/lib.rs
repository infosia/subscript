//! C boundary kind facts shared by the binder, layouts, and both backends.
#![warn(missing_docs)]

mod pass;
mod read;

pub use pass::{
    builds_scratch, class_pass, copy_back, copying_structs, cycle_structs, element_pass,
    embedded_pass, member_pass, parameter_pass, scratch_members, struct_pass, target_pass,
    value_parameter_pass, writes_back, CopyBack, FieldShape, PointerPass, StructPass, StructView,
};
pub use read::{
    first_cycle, first_unreadable, member_read, no_read_lowering_message,
    written_back_pair_message, MemberKind, MemberRead, Reach, ReadMember, ReadPosition, ReadRoot,
    ReadView, StructCycle, Unreadable, UnreadableKind, WrittenBack,
};

/// The native register bank of a scalar boundary value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Carrier {
    /// A general register or its stack slot.
    General,
    /// A SIMD register or its stack slot.
    Simd,
    /// An aggregate memory image.
    Memory,
}

/// The extension that a caller supplies for a narrow integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Extension {
    /// No extension.
    None,
    /// Sign extension.
    Signed,
    /// Zero extension.
    Unsigned,
}

/// The homogeneous floating-point aggregate leaf class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Leaf {
    /// An integer or pointer leaf.
    Integer,
    /// An IEEE binary16 leaf.
    Half,
    /// An IEEE binary32 leaf.
    Float,
    /// An IEEE binary64 leaf.
    Double,
}

/// All ABI facts for one fundamental boundary kind on supported 64-bit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Kind {
    /// The canonical C type.
    pub c_type: &'static str,
    /// The language spelling.
    pub language: &'static str,
    /// The storage size in bytes.
    pub size: u32,
    /// The natural storage alignment in bytes.
    pub align: u32,
    /// The native register bank.
    pub carrier: Carrier,
    /// The caller extension for arguments narrower than 32 bits.
    pub extension: Extension,
    /// The aggregate leaf class.
    pub leaf: Leaf,
}

use Carrier::{General, Simd};
use Extension::{None as NoExtension, Signed, Unsigned};
use Leaf::{Double, Float, Half, Integer};

/// The complete fundamental kind table.
pub const KINDS: &[Kind] = &[
    Kind {
        c_type: "int8_t",
        language: "i8",
        size: 1,
        align: 1,
        carrier: General,
        extension: Signed,
        leaf: Integer,
    },
    Kind {
        c_type: "uint8_t",
        language: "u8",
        size: 1,
        align: 1,
        carrier: General,
        extension: Unsigned,
        leaf: Integer,
    },
    Kind {
        c_type: "int16_t",
        language: "i16",
        size: 2,
        align: 2,
        carrier: General,
        extension: Signed,
        leaf: Integer,
    },
    Kind {
        c_type: "uint16_t",
        language: "u16",
        size: 2,
        align: 2,
        carrier: General,
        extension: Unsigned,
        leaf: Integer,
    },
    Kind {
        c_type: "int32_t",
        language: "i32",
        size: 4,
        align: 4,
        carrier: General,
        extension: NoExtension,
        leaf: Integer,
    },
    Kind {
        c_type: "uint32_t",
        language: "u32",
        size: 4,
        align: 4,
        carrier: General,
        extension: NoExtension,
        leaf: Integer,
    },
    Kind {
        c_type: "int64_t",
        language: "i64",
        size: 8,
        align: 8,
        carrier: General,
        extension: NoExtension,
        leaf: Integer,
    },
    Kind {
        c_type: "uint64_t",
        language: "u64",
        size: 8,
        align: 8,
        carrier: General,
        extension: NoExtension,
        leaf: Integer,
    },
    Kind {
        c_type: "bool",
        language: "boolean",
        size: 1,
        align: 1,
        carrier: General,
        extension: Unsigned,
        leaf: Integer,
    },
    Kind {
        c_type: "_Float16",
        language: "f16",
        size: 2,
        align: 2,
        carrier: Simd,
        extension: NoExtension,
        leaf: Half,
    },
    Kind {
        c_type: "float",
        language: "f32",
        size: 4,
        align: 4,
        carrier: Simd,
        extension: NoExtension,
        leaf: Float,
    },
    Kind {
        c_type: "double",
        language: "f64",
        size: 8,
        align: 8,
        carrier: Simd,
        extension: NoExtension,
        leaf: Double,
    },
];

/// Looks up a language scalar kind.
#[must_use]
pub fn language_kind(language: &str) -> Option<&'static Kind> {
    let index = match language {
        "i8" => 0,
        "u8" => 1,
        "i16" => 2,
        "u16" => 3,
        "i32" => 4,
        "u32" => 5,
        "i64" => 6,
        "u64" => 7,
        "boolean" => 8,
        "f16" => 9,
        "f32" => 10,
        "f64" => 11,
        _ => return None,
    };
    KINDS.get(index)
}

/// Looks up a supported C scalar spelling, including width-stable builtin aliases.
#[must_use]
pub fn c_kind(spelling: &str) -> Option<&'static Kind> {
    let canonical = match spelling {
        "signed char" => "int8_t",
        "unsigned char" => "uint8_t",
        "short" | "short int" | "signed short" | "signed short int" => "int16_t",
        "unsigned short" | "unsigned short int" => "uint16_t",
        "int" => "int32_t",
        "unsigned int" => "uint32_t",
        "long long" => "int64_t",
        "unsigned long long" | "size_t" => "uint64_t",
        other => other,
    };
    KINDS.iter().find(|kind| kind.c_type == canonical)
}
