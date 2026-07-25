#![warn(missing_docs)]
//! subscript runtime (plan phase P2, `specs/blocks/compiler.md` §7).
//!
//! The single runtime crate shared by both execution tiers: Context
//! memory (manual delete, explicit collect), strings, arrays, traps,
//! coroutine frame storage, and Q14 numeric formatting. Everything
//! callable from generated code lives in [`ffi`] as `extern "C"`
//! functions with stable signatures.
//!
//! # Trap mechanism
//!
//! Traps do not use signals, SEH, or unwinding. A runtime function
//! that detects a fault records a [`TrapRecord`] and sets the trap
//! flag at byte offset 0 of the [`Context`]; generated code loads the
//! flag after every call and after each emitted check and branches to
//! a per-function early-return path, so the whole script call stack
//! returns normally to the driver, which then reads the record. The
//! host process is never killed and no foreign frames are unwound.

pub mod arrops;
pub mod context;
pub mod date;
pub mod ffi;
pub mod fmt;
mod half;
pub mod math;
pub mod strops;
pub mod trap;

pub use context::Context;
pub use trap::{TrapKind, TrapRecord};
