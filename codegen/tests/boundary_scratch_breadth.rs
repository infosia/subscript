//! OBS-3 §44.8: combined breadth-and-depth coverage for recursive boundary
//! scratch.

use std::collections::HashSet;
use std::fmt::Write as _;

use subscript_codegen::{emit_c, run_jit_with_native_libraries, NativeLibrary};
use subscript_compiler::{check_program, SourceFile};

const MAX_POSITIONS: usize = 32;

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn target_size(position: usize) -> usize {
    // String view + pair + embedded nested state (string + pair + pointer) +
    // the position's aggregate.
    align_up(72 + position * 4, 8)
}

fn leaf_size(position: usize) -> usize {
    // String view + collapsed count/pointer pair + position-sized aggregate.
    align_up(32 + position * 4, 8)
}

fn outer_layout(position_count: usize) -> (Vec<usize>, usize) {
    let mut offset = 20; // outer label string view + u32 position count
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
unsafe extern "C" fn observe(descriptor: *const u8) -> u32 {
    if descriptor.is_null() || descriptor.addr() % 8 != 0 {
        return 1;
    }
    // SAFETY: every generated outer type has a u32 position count at +16.
    let position_count = unsafe { descriptor.add(16).cast::<u32>().read_unaligned() as usize };
    if !(1..=MAX_POSITIONS).contains(&position_count) {
        return 1;
    }
    let (pointer_offsets, outer_size) = outer_layout(position_count);
    let mut owners = Vec::with_capacity(1 + position_count * 2);
    let mut arrays = Vec::with_capacity(position_count * 3);
    owners.push((descriptor.addr(), outer_size));

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
        owners.push(target_range);

        // The target pair is at +16/+24. Its embedded nested state begins at
        // +32, putting the nested pair at +48/+56 and its reach-through leaf
        // pointer at +64.
        for (count_offset, data_offset) in [(16, 24), (48, 56)] {
            // SAFETY: the target was validated above and each pair offset is
            // fixed by the actual C layout described above.
            let count = unsafe {
                target
                    .add(count_offset)
                    .cast::<usize>()
                    .read_unaligned()
            };
            let data = unsafe {
                target
                    .add(data_offset)
                    .cast::<*const u8>()
                    .read_unaligned()
            };
            if count != 2 || data.is_null() || data.addr() % 4 != 0 {
                return 8;
            }
            arrays.push((data.addr(), count * 4));
        }

        // SAFETY: +64 is the pointer-aligned leaf member inside the embedded
        // nested state, and the leaf type is registered for recursive
        // reach-through lowering.
        let leaf = unsafe { target.add(64).cast::<*const u8>().read_unaligned() };
        if leaf.is_null() || leaf.addr() % 8 != 0 {
            return 4;
        }
        owners.push((leaf.addr(), leaf_size(index + 1)));

        // SAFETY: the leaf pair follows its 16-byte string view at +16/+24.
        let leaf_count = unsafe { leaf.add(16).cast::<usize>().read_unaligned() };
        let leaf_data = unsafe { leaf.add(24).cast::<*const u8>().read_unaligned() };
        if leaf_count != 2 || leaf_data.is_null() || leaf_data.addr() % 4 != 0 {
            return 8;
        }
        arrays.push((leaf_data.addr(), leaf_count * 4));
    }

    let regions = owners.iter().chain(&arrays).copied().collect::<Vec<_>>();
    for left in 0..regions.len() {
        for right in left + 1..regions.len() {
            if overlaps(regions[left], regions[right]) {
                return 16;
            }
        }
    }
    0
}

fn library() -> NativeLibrary {
    let symbols = (1..=MAX_POSITIONS)
        .map(|count| (format!("scratchObserve{count}"), observe as *const u8))
        .collect();
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

        writeln!(source, "declare class ScratchLeaf{position} {{").unwrap();
        source.push_str("  label: string;\n  values: u32[];\n");
        writeln!(source, "  aggregate: ScratchAggregate{position};").unwrap();
        writeln!(
            source,
            "  constructor(label: string, values: u32[], aggregate: ScratchAggregate{position});"
        )
        .unwrap();
        source.push_str("}\n");

        writeln!(source, "declare class ScratchNested{position} {{").unwrap();
        source.push_str("  label: string;\n  values: u32[];\n");
        writeln!(source, "  leaf: ScratchLeaf{position} | null;").unwrap();
        writeln!(
            source,
            "  constructor(label: string, values: u32[], leaf: ScratchLeaf{position} | null);"
        )
        .unwrap();
        source.push_str("}\n");

        writeln!(source, "declare class ScratchTarget{position} {{").unwrap();
        source.push_str("  label: string;\n  values: u32[];\n");
        writeln!(source, "  nested: ScratchNested{position};").unwrap();
        writeln!(source, "  aggregate: ScratchAggregate{position};").unwrap();
        writeln!(
            source,
            "  constructor(label: string, values: u32[], nested: ScratchNested{position}, aggregate: ScratchAggregate{position});"
        )
        .unwrap();
        source.push_str("}\n");
    }

    for count in 1..=MAX_POSITIONS {
        writeln!(source, "declare class ScratchOuter{count} {{").unwrap();
        source.push_str("  label: string;\n  positionCount: u32;\n");
        for position in 1..=count {
            writeln!(
                source,
                "  position{position}: ScratchTarget{position} | null;"
            )
            .unwrap();
            writeln!(source, "  sibling{position}: ScratchAggregate{position};").unwrap();
        }
        source.push_str("  constructor(label: string, positionCount: u32");
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
                "  const targetValues{count}_{position}: u32[] = [{}, {}];",
                count * 1000 + position * 10 + 1,
                count * 1000 + position * 10 + 2,
            )
            .unwrap();
            writeln!(
                source,
                "  const nestedValues{count}_{position}: u32[] = [{}, {}];",
                count * 2000 + position * 10 + 1,
                count * 2000 + position * 10 + 2,
            )
            .unwrap();
            writeln!(
                source,
                "  const leafValues{count}_{position}: u32[] = [{}, {}];",
                count * 3000 + position * 10 + 1,
                count * 3000 + position * 10 + 2,
            )
            .unwrap();
            writeln!(
                source,
                "  const leaf{count}_{position}: ScratchLeaf{position} = new ScratchLeaf{position}(\"leaf-{count}-{position}\", leafValues{count}_{position}, {});",
                aggregate_new(position, true, count * 30 + position),
            )
            .unwrap();
            writeln!(
                source,
                "  const nested{count}_{position}: ScratchNested{position} = new ScratchNested{position}(\"nested-{count}-{position}\", nestedValues{count}_{position}, leaf{count}_{position});",
            )
            .unwrap();
            writeln!(
                source,
                "  const target{count}_{position}: ScratchTarget{position} = new ScratchTarget{position}(\"target-{count}-{position}\", targetValues{count}_{position}, nested{count}_{position}, {});",
                aggregate_new(position, true, count * 10 + position),
            )
            .unwrap();
        }
        write!(
            source,
            "  const outer{count}: ScratchOuter{count} = new ScratchOuter{count}(\"outer-{count}\", {count}"
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

fn address_plan(c: &str, owner_type: &str) -> Vec<String> {
    c.lines()
        .map(str::trim)
        .filter(|line| {
            line.contains("subscript_rt_boundary_scratch_alloc")
                && line.contains(&format!("sizeof({owner_type}"))
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

    let empty_targets = address_plan(&empty_c, "ScratchTarget");
    let populated_targets = address_plan(&populated_c, "ScratchTarget");
    let empty_leaves = address_plan(&empty_c, "ScratchLeaf");
    let populated_leaves = address_plan(&populated_c, "ScratchLeaf");
    let expected_positions = (1..=MAX_POSITIONS).sum::<usize>();
    assert_eq!(empty_targets.len(), expected_positions, "{empty_c}");
    assert_eq!(empty_leaves.len(), expected_positions, "{empty_c}");
    assert_eq!(
        populated_targets, empty_targets,
        "sibling payload changed target scratch addresses"
    );
    assert_eq!(
        populated_leaves, empty_leaves,
        "sibling payload changed nested leaf scratch addresses"
    );

    let owners = empty_targets
        .iter()
        .chain(&empty_leaves)
        .map(|line| {
            line.split('=')
                .next()
                .expect("scratch owner assignment")
                .trim()
        })
        .collect::<HashSet<_>>();
    assert_eq!(
        owners.len(),
        expected_positions * 2,
        "two lowered positions share one scratch owner"
    );
    for position in 1..=MAX_POSITIONS {
        let expected_uses = MAX_POSITIONS - position + 1;
        for (kind, plan) in [("Target", &empty_targets), ("Leaf", &empty_leaves)] {
            let needle = format!("sizeof(Scratch{kind}{position})");
            assert_eq!(
                plan.iter().filter(|line| line.contains(&needle)).count(),
                expected_uses,
                "position {position} was sized from another {kind}"
            );
        }
    }
    assert!(
        empty_targets
            .iter()
            .chain(&empty_leaves)
            .all(|line| !line.contains("sibling")),
        "a sibling field participates in a scratch address"
    );

    let expected_stdout = "0\n".repeat(MAX_POSITIONS);
    for (label, hir, source) in [
        ("empty siblings", empty_hir, program(false)),
        ("populated siblings", populated_hir, program(true)),
    ] {
        // Keep C emission in the same test as the executed CLIF lowering: the
        // former pins content-independent address construction, while this
        // observer checks the concrete root, target, nested leaf, and
        // pair-data regions.
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
