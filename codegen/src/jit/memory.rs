//! One dev-JIT module is one reservation (`specs/blocks/compiler.md`
//! §110).
//!
//! `JITBuilder::with_isa` refuses position-independent code, so a call
//! from a module's code to the module's own data carries a 32-bit
//! displacement. Separate mappings put no bound on that displacement.
//! An `ArenaMemoryProvider` reserves one contiguous region per module
//! and carves the code, read-write, and read-only segments from it, so
//! the module's own reservation bounds the displacement.

use std::io;

use cranelift_jit::{ArenaMemoryProvider, BranchProtection, JITBuilder, JITMemoryProvider};
use cranelift_module::ModuleResult;

use crate::lower::internal;

/// The per-module cost that does not scale with the source, in bytes
/// (§110 rule 3, `floor`).
///
/// Measured over the 989 dev-JIT modules the debug workspace test run
/// builds. 800 of them hold a source of 10,000 bytes or less, which is
/// too small for the slope to reach. The worst span of those 800 is
/// this number, from a module of 7,267 source bytes.
const RESERVATION_FLOOR_BYTES: u64 = 104_466;

/// Reservation bytes for each byte of module source (§110 rule 3,
/// `slope`).
///
/// Measured over the same 989 modules. The other 189 hold a source of
/// more than 10,000 bytes, and the worst `(span - floor)` for each
/// source byte over those is 2.999, from a module of 493,178 source
/// bytes with a span of 1,583,180. This constant is the next whole
/// byte.
const RESERVATION_SLOPE: u64 = 3;

/// The margin the reservation holds over the derived size, as a
/// numerator over a denominator (§110 rule 3: 1.5). It is the margin
/// §109.2a holds over the same kind of measured bound.
const MARGIN_NUMERATOR: u64 = 3;

/// The denominator of the §110 rule 3 margin.
const MARGIN_DENOMINATOR: u64 = 2;

/// Returns the bytes one module reserves for `source_bytes` of source
/// (§110 rule 3).
pub(crate) fn reservation_bytes(source_bytes: usize) -> usize {
    let derived = RESERVATION_FLOOR_BYTES
        .saturating_add(RESERVATION_SLOPE.saturating_mul(source_bytes as u64))
        .saturating_mul(MARGIN_NUMERATOR)
        / MARGIN_DENOMINATOR;
    usize::try_from(derived).unwrap_or(usize::MAX)
}

/// One module's one reservation, with the two numbers §110 rule 4
/// reports when the reservation runs out.
struct OneReservation {
    arena: ArenaMemoryProvider,
    source_bytes: usize,
    reserved_bytes: usize,
}

impl OneReservation {
    /// Names the module's source bytes, the reservation the derivation
    /// gave, and the bytes the module asked for (§110 rule 4).
    fn report(&self, request: usize, result: io::Result<*mut u8>) -> io::Result<*mut u8> {
        result.map_err(|error| {
            io::Error::other(format!(
                "the reservation of one dev-JIT module ran out: module source \
                 {} bytes, reservation {} bytes, request {request} bytes; the \
                 constant of `specs/blocks/compiler.md` §110 rule 3 is too \
                 small: {error}",
                self.source_bytes, self.reserved_bytes
            ))
        })
    }
}

impl JITMemoryProvider for OneReservation {
    fn allocate_readexec(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        let result = self.arena.allocate_readexec(size, align);
        self.report(size, result)
    }

    fn allocate_readwrite(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        let result = self.arena.allocate_readwrite(size, align);
        self.report(size, result)
    }

    fn allocate_readonly(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        let result = self.arena.allocate_readonly(size, align);
        self.report(size, result)
    }

    unsafe fn free_memory(&mut self) {
        // SAFETY: the caller states that no pointer into this module
        // is live, which is the same condition the arena requires.
        unsafe { self.arena.free_memory() };
    }

    fn finalize(&mut self, branch_protection: BranchProtection) -> ModuleResult<()> {
        self.arena.finalize(branch_protection)
    }
}

/// Installs one reservation on `builder` for a module of
/// `source_bytes` of source (§110 rule 1).
///
/// # Errors
///
/// Returns an internal error when the host refuses the reservation.
pub(crate) fn install_reservation(
    builder: &mut JITBuilder,
    source_bytes: usize,
) -> Result<(), String> {
    let reserved_bytes = reservation_bytes(source_bytes);
    let arena = ArenaMemoryProvider::new_with_size(reserved_bytes).map_err(|error| {
        internal(format!(
            "the host refused a dev-JIT reservation of {reserved_bytes} bytes: {error}"
        ))
    })?;
    builder.memory_provider(Box::new(OneReservation {
        arena,
        source_bytes,
        reserved_bytes,
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use cranelift_module::{FuncOrDataId, Module};
    use subscript_compiler::{Profile, SourceFile};

    use super::reservation_bytes;
    use crate::jit::compile::compile_jit;

    /// §110 rule 2: the displacement between two items of one finished
    /// module is inside that module's reservation.
    ///
    /// The two addresses come from the finished module, and the bound
    /// comes from the derivation of §110 rule 3.
    ///
    /// The literal holds one mebibyte, so the module's read-only data
    /// is too large for the mapping that holds the code. A module
    /// built with no reservation therefore puts the two items in two
    /// mappings, and the compile thread's stack lies between them.
    #[test]
    fn two_items_of_one_module_are_inside_that_modules_reservation() {
        let literal = "a".repeat(1 << 20);
        let source = format!("export function main(): void {{\n  print(\"{literal}\");\n}}\n");
        let source = source.as_str();
        let files = [SourceFile::new("main.ts", source)];
        let (module, lowered, _) =
            compile_jit(&files, &[], Profile::Default).expect("the dev JIT compiles the module");
        let main = lowered.main_id().expect("the module exports `main`");
        let code = module.get_finalized_function(main) as usize;
        let data_id = match module.get_name("subscript_str0") {
            Some(FuncOrDataId::Data(id)) => id,
            other => panic!("the module defines no string literal data: {other:?}"),
        };
        let (data, _) = module.get_finalized_data(data_id);
        let data = data as usize;
        // SAFETY: nothing ran, and no address of this module leaves
        // this test as a pointer.
        unsafe { module.free_memory() };

        let displacement = code.abs_diff(data);
        let reservation = reservation_bytes(source.len());
        assert!(
            displacement < reservation,
            "the code-to-data displacement is {displacement} bytes, \
             and the module reserves {reservation} bytes"
        );
    }

    /// The derivation is the floor plus the slope, times the margin.
    #[test]
    fn the_reservation_is_the_floor_plus_the_slope_times_the_margin() {
        assert_eq!(reservation_bytes(0), 156_699);
        assert_eq!(reservation_bytes(131_072), 746_523);
    }
}
