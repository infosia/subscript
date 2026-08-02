//! OBS-3 §44.7: direct breadth-axis coverage for recursive boundary scratch.

use std::collections::HashSet;
use std::fmt::Write as _;

use subscript_codegen::{emit_c, run_jit_with_native_libraries, NativeLibrary};
use subscript_compiler::{check_program, SourceFile};

const MAX_POSITIONS: usize = 6;

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn target_size(position: usize) -> usize {
    // String view + collapsed count/pointer pair + the position's aggregate.
    align_up(16 + 16 + position * 4, 8)
}

fn outer_layout(position_count: usize) -> (Vec<usize>, usize) {
    let mut offset = 16; // outer label string view
    let mut pointers = Vec::with_capacity(position_count);
    for position in 1..=position_count {
        offset = align_up(offset, 8);
        pointers.push(offset);
        offset += 8;
        offset = align_up(offset, 4);
        offset += position * 4;
    }
    (pointers, align_up(offset, 8))
}

fn overlaps(left: (usize, usize), right: (usize, usize)) -> bool {
    let (Some(left_end), Some(right_end)) =
        (left.0.checked_add(left.1), right.0.checked_add(right.1))
    else {
        return true;
    };
    left.1 != 0 && right.1 != 0 && left.0 < right_end && right.0 < left_end
}

/// Checks the concrete pointers received by a foreign call. A non-zero bit
/// identifies null/misaligned storage or overlap between independently
/// lowered positions. Reads are unaligned so a lowering defect is reported
/// as a value instead of making the test host perform a misaligned load.
unsafe fn observe(position_count: usize, descriptor: *const u8) -> u32 {
    if descriptor.is_null() || descriptor.addr() % 8 != 0 {
        return 1;
    }
    let (pointer_offsets, outer_size) = outer_layout(position_count);
    let outer = (descriptor.addr(), outer_size);
    let mut targets = Vec::with_capacity(position_count);
    let mut arrays = Vec::with_capacity(position_count);

    for (index, pointer_offset) in pointer_offsets.into_iter().enumerate() {
        // SAFETY: each offset is the pointer-aligned C-layout position derived
        // above; read_unaligned also turns an alignment defect into evidence.
        let target = unsafe {
            descriptor
                .add(pointer_offset)
                .cast::<*const u8>()
                .read_unaligned()
        };
        if target.is_null() || target.addr() % 8 != 0 {
            return 2;
        }
        let target_range = (target.addr(), target_size(index + 1));
        if overlaps(outer, target_range) {
            return 4;
        }
        if targets
            .iter()
            .copied()
            .any(|prior| overlaps(prior, target_range))
        {
            return 8;
        }

        // The target's array pair follows its 16-byte string view.
        // SAFETY: the target was validated above and the pair's fields are
        // at the fixed actual-C-layout offsets 16 and 24.
        let count = unsafe { target.add(16).cast::<usize>().read_unaligned() };
        let data = unsafe { target.add(24).cast::<*const u8>().read_unaligned() };
        if count != 2 || data.is_null() || data.addr() % 4 != 0 {
            return 16;
        }
        let array_range = (data.addr(), count * 4);
        if overlaps(outer, array_range)
            || targets
                .iter()
                .copied()
                .any(|prior| overlaps(prior, array_range))
            || arrays
                .iter()
                .copied()
                .any(|prior| overlaps(prior, array_range))
        {
            return 32;
        }
        targets.push(target_range);
        arrays.push(array_range);
    }
    0
}

macro_rules! observer {
    ($name:ident, $count:literal) => {
        unsafe extern "C" fn $name(descriptor: *const u8) -> u32 {
            // SAFETY: the generated mirror declares the same one-pointer ABI;
            // `observe` validates the actual C-layout storage before use.
            unsafe { observe($count, descriptor) }
        }
    };
}

observer!(observe_1, 1);
observer!(observe_2, 2);
observer!(observe_3, 3);
observer!(observe_4, 4);
observer!(observe_5, 5);
observer!(observe_6, 6);

fn library() -> NativeLibrary {
    let symbols = vec![
        ("scratchObserve1".to_string(), observe_1 as *const u8),
        ("scratchObserve2".to_string(), observe_2 as *const u8),
        ("scratchObserve3".to_string(), observe_3 as *const u8),
        ("scratchObserve4".to_string(), observe_4 as *const u8),
        ("scratchObserve5".to_string(), observe_5 as *const u8),
        ("scratchObserve6".to_string(), observe_6 as *const u8),
    ];
    // SAFETY: every static-lifetime address above has the one-pointer C ABI
    // declared in `mirror`, and each returns the declared u32 status value.
    unsafe { NativeLibrary::new(Vec::new(), Vec::new(), symbols) }
}

fn mirror() -> String {
    let mut source = String::from("// @subscript-c-header include=\"scratch-breadth.h\"\n");
    for position in 1..=MAX_POSITIONS {
        writeln!(source, "declare class ScratchAggregate{position} {{").unwrap();
        for field in 1..=position {
            writeln!(source, "  field{field}: u32;").unwrap();
        }
        write!(source, "  constructor(").unwrap();
        for field in 1..=position {
            if field != 1 {
                source.push_str(", ");
            }
            write!(source, "field{field}: u32").unwrap();
        }
        source.push_str(");\n}\n");

        writeln!(source, "declare class ScratchTarget{position} {{").unwrap();
        source.push_str("  label: string;\n  values: u32[];\n");
        writeln!(source, "  aggregate: ScratchAggregate{position};").unwrap();
        writeln!(
            source,
            "  constructor(label: string, values: u32[], aggregate: ScratchAggregate{position});"
        )
        .unwrap();
        source.push_str("}\n");
    }

    for count in 1..=MAX_POSITIONS {
        writeln!(source, "declare class ScratchOuter{count} {{").unwrap();
        source.push_str("  label: string;\n");
        for position in 1..=count {
            writeln!(
                source,
                "  position{position}: ScratchTarget{position} | null;"
            )
            .unwrap();
            writeln!(source, "  sibling{position}: ScratchAggregate{position};").unwrap();
        }
        source.push_str("  constructor(label: string");
        for position in 1..=count {
            write!(
                source,
                ", position{position}: ScratchTarget{position} | null, sibling{position}: ScratchAggregate{position}"
            )
            .unwrap();
        }
        source.push_str(");\n}\n");
        writeln!(
            source,
            "declare function scratchObserve{count}(descriptor: ScratchOuter{count} | null): u32;"
        )
        .unwrap();
    }
    source
}

fn aggregate_new(position: usize, populated: bool, seed: usize) -> String {
    let fields = (1..=position)
        .map(|field| {
            if populated {
                (seed * 100 + field).to_string()
            } else {
                "0".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("new ScratchAggregate{position}({fields})")
}

fn program(populated_siblings: bool) -> String {
    let mut source = String::from("export function main(): void {\n");
    for count in 1..=MAX_POSITIONS {
        for position in 1..=count {
            writeln!(
                source,
                "  const values{count}_{position}: u32[] = [{}, {}];",
                count * 1000 + position * 10 + 1,
                count * 1000 + position * 10 + 2,
            )
            .unwrap();
            writeln!(
                source,
                "  const target{count}_{position}: ScratchTarget{position} = new ScratchTarget{position}(\"target-{count}-{position}\", values{count}_{position}, {});",
                aggregate_new(position, true, count * 10 + position),
            )
            .unwrap();
        }
        write!(
            source,
            "  const outer{count}: ScratchOuter{count} = new ScratchOuter{count}(\"outer-{count}\""
        )
        .unwrap();
        for position in 1..=count {
            write!(
                source,
                ", target{count}_{position}, {}",
                aggregate_new(position, populated_siblings, count * 10 + position),
            )
            .unwrap();
        }
        source.push_str(");\n");
        writeln!(
            source,
            "  print(`${{scratchObserve{count}(outer{count})}}`);"
        )
        .unwrap();
    }
    source.push_str("}\n");
    source
}

fn checked(populated_siblings: bool) -> subscript_compiler::hir::Module {
    check_program(&[
        SourceFile::ambient("scratch-breadth.generated.d.ts", mirror()),
        SourceFile::new("scratch-breadth.ts", program(populated_siblings)),
    ])
    .unwrap_or_else(|diagnostics| {
        panic!(
            "scratch breadth fixture rejected:\n{}",
            diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn address_plan(c: &str) -> Vec<String> {
    c.lines()
        .map(str::trim)
        .filter(|line| {
            line.contains("subscript_rt_boundary_scratch_alloc")
                && line.contains("sizeof(ScratchTarget")
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn lowered_positions_are_disjoint_and_sibling_content_independent_from_one_through_n() {
    let empty_hir = checked(false);
    let populated_hir = checked(true);
    let empty_c = emit_c(&empty_hir)
        .expect("empty-sibling breadth fixture emits C")
        .source;
    let populated_c = emit_c(&populated_hir)
        .expect("populated-sibling breadth fixture emits C")
        .source;

    let empty_plan = address_plan(&empty_c);
    let populated_plan = address_plan(&populated_c);
    let expected_positions = (1..=MAX_POSITIONS).sum::<usize>();
    assert_eq!(empty_plan.len(), expected_positions, "{empty_c}");
    assert_eq!(
        populated_plan, empty_plan,
        "sibling payload changed scratch addresses"
    );

    let owners = empty_plan
        .iter()
        .map(|line| {
            line.split('=')
                .next()
                .expect("scratch owner assignment")
                .trim()
        })
        .collect::<HashSet<_>>();
    assert_eq!(
        owners.len(),
        expected_positions,
        "two lowered positions share one scratch owner: {empty_plan:?}"
    );
    for position in 1..=MAX_POSITIONS {
        let expected_uses = MAX_POSITIONS - position + 1;
        let needle = format!("sizeof(ScratchTarget{position})");
        assert_eq!(
            empty_plan
                .iter()
                .filter(|line| line.contains(&needle))
                .count(),
            expected_uses,
            "position {position} was sized from another target: {empty_plan:?}"
        );
    }
    assert!(
        empty_plan.iter().all(|line| !line.contains("sibling")),
        "a sibling field participates in a scratch address: {empty_plan:?}"
    );

    let expected_stdout = "0\n".repeat(MAX_POSITIONS);
    for (label, hir, source) in [
        ("empty siblings", empty_hir, program(false)),
        ("populated siblings", populated_hir, program(true)),
    ] {
        // Keep C emission in the same test as the executed CLIF lowering: the
        // former pins content-independent address construction, while this
        // observer checks the concrete root, target, and pair-data regions.
        drop(hir);
        let files = [
            SourceFile::ambient("scratch-breadth.generated.d.ts", mirror()),
            SourceFile::new("scratch-breadth.ts", source),
        ];
        let stdout = run_jit_with_native_libraries(&files, &[library()])
            .unwrap_or_else(|error| panic!("{label}: breadth JIT run failed: {error}"));
        assert_eq!(String::from_utf8_lossy(&stdout), expected_stdout, "{label}");
    }
}
