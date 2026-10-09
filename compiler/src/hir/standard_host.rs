//! Standard operations that the host completes through a Context provider.
use crate::Type;

/// A checked whole-file operation (§185).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum StandardHostOperation {
    /// Reads UTF-8 text.
    ReadText = 0,
    /// Reads bytes.
    ReadBytes = 1,
    /// Writes UTF-8 text.
    WriteText = 2,
    /// Writes bytes.
    WriteBytes = 3,
}
impl StandardHostOperation {
    /// The normalized script parameters; the encoding is resolved by the checker.
    #[must_use]
    pub fn parameters(self) -> Vec<Type> {
        match self {
            Self::ReadText | Self::ReadBytes => vec![Type::Str],
            Self::WriteText => vec![Type::Str, Type::Str],
            Self::WriteBytes => vec![Type::Str, Type::Array(Box::new(Type::U8))],
        }
    }
    /// The source result type.
    #[must_use]
    pub fn result(self) -> Type {
        match self {
            Self::ReadText => Type::Str,
            Self::ReadBytes => Type::Array(Box::new(Type::U8)),
            Self::WriteText | Self::WriteBytes => Type::Void,
        }
    }
}
