//! Measures per-frame reserved-byte growth under both dev-tier freed-handle
//! diagnostic settings.
//!
//! This is an accounting probe, not a timed cross-language benchmark. Each
//! point runs a fresh Context through the dev JIT, then reads the Context's
//! §18.2d live and reserved byte figures before that Context is dropped.

use std::process::ExitCode;

use subscript_codegen::run_jit_with_memory_accounting;
use subscript_compiler::SourceFile;

const LINEARITY_FRAME_COUNTS: [u64; 4] = [0, 100, 1_000, 10_000];
const SWEEP_FRAME_COUNTS: [u64; 2] = [100, 10_000];
const ALLOCATIONS_PER_FRAME: u64 = 1;
const REFERENCE_ALLOCATIONS_PER_FRAME: f64 = 1_000.0;
const REFERENCE_FRAMES_PER_SECOND: f64 = 60.0;
const REFERENCE_BUDGET_BYTES: f64 = 8_000_000_000.0;

#[derive(Clone, Copy)]
struct Setting {
    enabled: bool,
    label: &'static str,
}

const SETTINGS: [Setting; 2] = [
    Setting {
        enabled: false,
        label: "off",
    },
    Setting {
        enabled: true,
        label: "on",
    },
];

#[derive(Clone, Copy)]
struct Shape {
    id: &'static str,
    label: &'static str,
    field_type: &'static str,
    field_count: usize,
}

const SHAPES: [Shape; 4] = [
    Shape {
        id: "i32",
        label: "1 x i32",
        field_type: "i32",
        field_count: 1,
    },
    Shape {
        id: "vec3",
        label: "vector: 3 x f32",
        field_type: "f32",
        field_count: 3,
    },
    Shape {
        id: "particle",
        label: "particle: 8 x f32",
        field_type: "f32",
        field_count: 8,
    },
    Shape {
        id: "large",
        label: "large: 32 x f32",
        field_type: "f32",
        field_count: 32,
    },
];

#[derive(Clone, Copy)]
enum Variant {
    Free,
    Collect,
}

impl Variant {
    fn id(self) -> &'static str {
        match self {
            Variant::Free => "A",
            Variant::Collect => "B",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Variant::Free => "Context.free in the allocation frame",
            Variant::Collect => "drop reference, then Context.collect per frame",
        }
    }

    fn frame_body(self) -> &'static str {
        match self {
            Variant::Free => {
                "const value: ProbeObject = new ProbeObject(frame);\n\
                 Context.free(value);"
            }
            Variant::Collect => {
                "let value: ProbeObject | null = new ProbeObject(frame);\n\
                 value = null;\n\
                 Context.collect();"
            }
        }
    }
}

struct Sample {
    frames: u64,
    live_bytes: u64,
    reserved_bytes: u64,
}

struct VariantResult {
    variant: Variant,
    samples: Vec<Sample>,
}

struct ShapeResult {
    shape: Shape,
    payload_bytes: u64,
    free: VariantResult,
    collect: VariantResult,
}

struct SettingResult {
    setting: Setting,
    shapes: Vec<ShapeResult>,
}

struct Growth {
    per_frame: f64,
    per_allocation: f64,
    exact_per_allocation: Option<u64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dev-retention-probe: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    println!("dev-tier freed-handle diagnostics retention probe");
    println!("one reference-class allocation per frame");
    println!("reference budget: 1000 allocations/frame, 60 fps, 8 GB (8000000000 bytes)");

    let payloads = SHAPES
        .iter()
        .copied()
        .map(|shape| measure_payload(shape).map(|payload| (shape.id, payload)))
        .collect::<Result<Vec<_>, String>>()?;

    let mut settings = Vec::with_capacity(SETTINGS.len());
    for setting in SETTINGS {
        let mut shapes = Vec::with_capacity(SHAPES.len());
        for (index, shape) in SHAPES.iter().copied().enumerate() {
            let frame_counts = if index == 0 {
                &LINEARITY_FRAME_COUNTS[..]
            } else {
                &SWEEP_FRAME_COUNTS[..]
            };
            let payload_bytes = payloads[index].1;
            shapes.push(measure_shape(setting, shape, payload_bytes, frame_counts)?);
        }
        settings.push(SettingResult { setting, shapes });
    }

    for result in &settings {
        println!();
        println!(
            "linearity evidence: freed-handle diagnostics {} for {} (payload {} bytes)",
            result.setting.label, result.shapes[0].shape.label, result.shapes[0].payload_bytes
        );
        print_variant_table(&result.shapes[0].free)?;
        print_variant_table(&result.shapes[0].collect)?;
        print_sweep_raw(result)?;
    }
    print_sweep_summary(&settings)?;
    for result in &settings {
        print_rule(result)?;
    }
    Ok(())
}

fn measure_shape(
    setting: Setting,
    shape: Shape,
    payload_bytes: u64,
    frame_counts: &[u64],
) -> Result<ShapeResult, String> {
    let free = measure_variant(setting, shape, Variant::Free, frame_counts)?;
    let collect = measure_variant(setting, shape, Variant::Collect, frame_counts)?;
    require_constant_live_set(setting, shape, &free)?;
    require_constant_live_set(setting, shape, &collect)?;
    Ok(ShapeResult {
        shape,
        payload_bytes,
        free,
        collect,
    })
}

fn measure_payload(shape: Shape) -> Result<u64, String> {
    let source = format!(
        "{}\n\
         let kept: ProbeObject | null = null;\n\
         export function main(): void {{\n\
           kept = new ProbeObject(1);\n\
         }}\n",
        class_source(shape)
    );
    let (stdout, accounting) = run_jit_with_memory_accounting(
        &[SourceFile::new(
            format!("dev-retention-payload-{}.ts", shape.id),
            source,
        )],
        false,
    )
    .map_err(|error| format!("payload probe {} did not run: {error}", shape.id))?;
    if !stdout.is_empty() {
        return Err(format!(
            "payload probe {} produced unexpected stdout {:?}",
            shape.id,
            String::from_utf8_lossy(&stdout)
        ));
    }
    if accounting.live_bytes == 0 {
        return Err(format!(
            "payload probe {} reported zero live bytes for one kept object",
            shape.id
        ));
    }
    Ok(accounting.live_bytes)
}

fn measure_variant(
    setting: Setting,
    shape: Shape,
    variant: Variant,
    frame_counts: &[u64],
) -> Result<VariantResult, String> {
    let samples = frame_counts
        .iter()
        .map(|&frames| {
            let source = frame_source(shape, variant, frames)?;
            let file = format!(
                "dev-retention-{}-{}-{}-{frames}.ts",
                setting.label,
                shape.id,
                variant.id().to_ascii_lowercase()
            );
            let (stdout, accounting) =
                run_jit_with_memory_accounting(&[SourceFile::new(file, source)], setting.enabled)
                    .map_err(|error| {
                    format!(
                    "diagnostics {} shape {} variant {} at {frames} frames did not run: {error}",
                    setting.label,
                    shape.id,
                    variant.id()
                )
                })?;
            if !stdout.is_empty() {
                return Err(format!(
                    "diagnostics {} shape {} variant {} at {frames} frames produced unexpected \
                     stdout {:?}",
                    setting.label,
                    shape.id,
                    variant.id(),
                    String::from_utf8_lossy(&stdout)
                ));
            }
            Ok(Sample {
                frames,
                live_bytes: accounting.live_bytes,
                reserved_bytes: accounting.reserved_bytes,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(VariantResult { variant, samples })
}

fn class_source(shape: Shape) -> String {
    let mut source = String::from("class ProbeObject {\n");
    for index in 0..shape.field_count {
        source.push_str(&format!("  field{index}: {};\n", shape.field_type));
    }
    source.push_str("  constructor(seed: i32) {\n");
    for index in 0..shape.field_count {
        let value = if shape.field_type == "i32" {
            "seed"
        } else {
            "seed as f32"
        };
        source.push_str(&format!("    this.field{index} = {value};\n"));
    }
    source.push_str("  }\n}\n");
    source
}

fn frame_source(shape: Shape, variant: Variant, frames: u64) -> Result<String, String> {
    let frames = i32::try_from(frames)
        .map_err(|_| format!("frame count {frames} does not fit script i32"))?;
    Ok(format!(
        "{}\n\
         export function main(): void {{\n\
           for (let frame: i32 = 0; frame < {frames}; frame += 1) {{\n\
             {}\n\
           }}\n\
         }}\n",
        class_source(shape),
        variant.frame_body()
    ))
}

fn require_constant_live_set(
    setting: Setting,
    shape: Shape,
    result: &VariantResult,
) -> Result<(), String> {
    let Some(first) = result.samples.first() else {
        return Err(format!(
            "diagnostics {} shape {} variant {} produced no samples",
            setting.label,
            shape.id,
            result.variant.id()
        ));
    };
    if result
        .samples
        .iter()
        .any(|sample| sample.live_bytes != first.live_bytes)
    {
        let values = result
            .samples
            .iter()
            .map(|sample| format!("{}:{}", sample.frames, sample.live_bytes))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "diagnostics {} shape {} variant {} live_bytes changed across frame counts \
             ({values}); the run is void",
            setting.label,
            shape.id,
            result.variant.id()
        ));
    }
    Ok(())
}

fn print_variant_table(result: &VariantResult) -> Result<(), String> {
    println!();
    println!(
        "variant {}: {}",
        result.variant.id(),
        result.variant.label()
    );
    println!(
        "{:>8} {:>12} {:>15} {:>18} {:>23}",
        "frames", "live_bytes", "reserved_bytes", "growth/frame", "growth/allocation"
    );
    for (index, sample) in result.samples.iter().enumerate() {
        if index == 0 {
            println!(
                "{:>8} {:>12} {:>15} {:>18} {:>23}",
                sample.frames, sample.live_bytes, sample.reserved_bytes, "-", "-"
            );
            continue;
        }
        let growth = growth_between(&result.samples[index - 1], sample)?;
        println!(
            "{:>8} {:>12} {:>15} {:>18.3} {:>23.3}",
            sample.frames,
            sample.live_bytes,
            sample.reserved_bytes,
            growth.per_frame,
            growth.per_allocation
        );
    }
    Ok(())
}

fn print_sweep_raw(result: &SettingResult) -> Result<(), String> {
    println!();
    println!(
        "object-size sweep raw endpoints, diagnostics {} (100..10000 frames)",
        result.setting.label
    );
    println!(
        "{:<22} {:>12} {:>19} {:>12} {:>19}",
        "shape", "A live", "A reserved", "B live", "B reserved"
    );
    for shape in &result.shapes {
        let (a_first, a_last) = sweep_endpoints(&shape.free)?;
        let (b_first, b_last) = sweep_endpoints(&shape.collect)?;
        println!(
            "{:<22} {:>12} {:>19} {:>12} {:>19}",
            shape.shape.label,
            pair(a_first.live_bytes, a_last.live_bytes),
            pair(a_first.reserved_bytes, a_last.reserved_bytes),
            pair(b_first.live_bytes, b_last.live_bytes),
            pair(b_first.reserved_bytes, b_last.reserved_bytes)
        );
    }
    Ok(())
}

fn print_sweep_summary(settings: &[SettingResult]) -> Result<(), String> {
    let [off, on] = settings else {
        return Err("the probe requires exactly the off and on settings".to_string());
    };
    println!();
    println!("object-size sweep, settings side by side");
    println!(
        "{:<22} {:>9} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "shape",
        "payload",
        "off A B/alloc",
        "off B B/alloc",
        "on A B/alloc",
        "on B B/alloc",
        "on A hours",
        "on B hours"
    );
    for (off_shape, on_shape) in off.shapes.iter().zip(&on.shapes) {
        if off_shape.shape.id != on_shape.shape.id
            || off_shape.payload_bytes != on_shape.payload_bytes
        {
            return Err("setting sweeps do not contain the same shapes".to_string());
        }
        let off_a = sweep_growth(&off_shape.free)?;
        let off_b = sweep_growth(&off_shape.collect)?;
        let on_a = sweep_growth(&on_shape.free)?;
        let on_b = sweep_growth(&on_shape.collect)?;
        println!(
            "{:<22} {:>9} {:>12.3} {:>12.3} {:>12.3} {:>12.3} {:>12.6} {:>12.6}",
            off_shape.shape.label,
            off_shape.payload_bytes,
            off_a.per_allocation,
            off_b.per_allocation,
            on_a.per_allocation,
            on_b.per_allocation,
            reference_hours(on_a.per_allocation)?,
            reference_hours(on_b.per_allocation)?
        );
    }
    println!("mode-off reference-budget duration: unbounded when growth/allocation is zero");
    Ok(())
}

fn print_rule(result: &SettingResult) -> Result<(), String> {
    let mut measured = Vec::with_capacity(result.shapes.len() * 2);
    for shape in &result.shapes {
        for variant in [&shape.free, &shape.collect] {
            let growth = sweep_growth(variant)?;
            let overhead = growth
                .exact_per_allocation
                .map(|bytes| i128::from(bytes) - i128::from(shape.payload_bytes));
            measured.push((shape.shape.label, variant.variant.id(), growth, overhead));
        }
    }

    println!();
    if measured
        .iter()
        .all(|entry| entry.2.exact_per_allocation == Some(0))
    {
        println!(
            "rule, diagnostics {}: reserved growth/allocation = 0 bytes exactly across all \
             measured shapes and both variants",
            result.setting.label
        );
        return Ok(());
    }

    let fixed = measured
        .first()
        .and_then(|entry| entry.3)
        .filter(|first| measured.iter().all(|entry| entry.3 == Some(*first)));
    match fixed {
        Some(overhead) => println!(
            "rule, diagnostics {}: reserved growth/allocation = payload bytes + {overhead} bytes \
             exactly across all measured shapes and both variants",
            result.setting.label
        ),
        None => {
            println!(
                "rule, diagnostics {}: no exact fixed overhead held across the measured sweep",
                result.setting.label
            );
            for (shape, variant, growth, overhead) in measured {
                match overhead {
                    Some(value) => println!(
                        "deviation: shape={shape}, variant={variant}, \
                         growth/allocation={:.3}, payload delta={value}",
                        growth.per_allocation
                    ),
                    None => println!(
                        "deviation: shape={shape}, variant={variant}, \
                         growth/allocation={:.3}, non-integral slope",
                        growth.per_allocation
                    ),
                }
            }
        }
    }
    Ok(())
}

fn sweep_endpoints(result: &VariantResult) -> Result<(&Sample, &Sample), String> {
    let first = result
        .samples
        .iter()
        .find(|sample| sample.frames == SWEEP_FRAME_COUNTS[0])
        .ok_or_else(|| {
            format!(
                "variant {} is missing the {}-frame sample",
                result.variant.id(),
                SWEEP_FRAME_COUNTS[0]
            )
        })?;
    let last = result
        .samples
        .iter()
        .find(|sample| sample.frames == SWEEP_FRAME_COUNTS[1])
        .ok_or_else(|| {
            format!(
                "variant {} is missing the {}-frame sample",
                result.variant.id(),
                SWEEP_FRAME_COUNTS[1]
            )
        })?;
    Ok((first, last))
}

fn sweep_growth(result: &VariantResult) -> Result<Growth, String> {
    let (first, last) = sweep_endpoints(result)?;
    growth_between(first, last)
}

fn growth_between(before: &Sample, after: &Sample) -> Result<Growth, String> {
    let frame_delta = after
        .frames
        .checked_sub(before.frames)
        .filter(|delta| *delta != 0)
        .ok_or_else(|| {
            format!(
                "frame counts are not strictly increasing: {} then {}",
                before.frames, after.frames
            )
        })?;
    let reserved_delta = after
        .reserved_bytes
        .checked_sub(before.reserved_bytes)
        .ok_or_else(|| {
            format!(
                "reserved bytes decreased from {} at {} frames to {} at {} frames",
                before.reserved_bytes, before.frames, after.reserved_bytes, after.frames
            )
        })?;
    let allocation_delta = frame_delta
        .checked_mul(ALLOCATIONS_PER_FRAME)
        .ok_or_else(|| "allocation delta overflowed u64".to_string())?;
    Ok(Growth {
        per_frame: reserved_delta as f64 / frame_delta as f64,
        per_allocation: reserved_delta as f64 / allocation_delta as f64,
        exact_per_allocation: (reserved_delta % allocation_delta == 0)
            .then_some(reserved_delta / allocation_delta),
    })
}

fn reference_hours(bytes_per_allocation: f64) -> Result<f64, String> {
    if bytes_per_allocation <= 0.0 {
        return Err(format!(
            "non-positive retained bytes per allocation: {bytes_per_allocation}"
        ));
    }
    Ok(REFERENCE_BUDGET_BYTES
        / (bytes_per_allocation * REFERENCE_ALLOCATIONS_PER_FRAME * REFERENCE_FRAMES_PER_SECOND)
        / 3_600.0)
}

fn pair(first: u64, last: u64) -> String {
    format!("{first}->{last}")
}
