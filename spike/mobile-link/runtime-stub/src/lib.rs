//! Runtime stub for the P0.5 mobile link spike: the single external
//! function the emitted spike object imports.

#![warn(missing_docs)]

use std::io::Write;

/// Prints `value` followed by a newline to stdout.
///
/// C ABI: `void subscript_rt_print_i64(int64_t value);`
///
/// Never panics: a stdout write failure is deliberately ignored — the spike
/// criterion is compile+link, not run-level verification.
// SAFETY: `unsafe(no_mangle)` only asserts that the exported symbol name
// `subscript_rt_print_i64` collides with no other symbol in the final
// link; the spike links exactly one copy of this stub and the emitted
// object only imports the symbol.
#[unsafe(no_mangle)]
pub extern "C" fn subscript_rt_print_i64(value: i64) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
}
