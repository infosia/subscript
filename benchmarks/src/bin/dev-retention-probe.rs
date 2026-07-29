//! Measures per-frame reserved-byte growth with dev-tier freed-handle
//! diagnostics off, at threshold 0, at a representative size threshold, and
//! with a finite retention budget.
//!
//! This is an accounting probe, not a timed cross-language benchmark. Each
//! point uses a fresh exact-size development Context, then reads its §18.2d
//! live and reserved byte figures before that Context is dropped.

use std::process::ExitCode;

use subscript_runtime::context::HEADER_SIZE;
use subscript_runtime::Context;

const LINEARITY_FRAME_COUNTS: [u64; 4] = [0, 100, 1_000, 10_000];
const SWEEP_FRAME_COUNTS: [u64; 2] = [100, 10_000];
const THRESHOLD_BYTES: usize = 32;
const RETENTION_BUDGET_BYTES: usize = 1_440;
const ALLOCATIONS_PER_FRAME: u64 = 1;
const REFERENCE_ALLOCATIONS_PER_FRAME: f64 = 1_000.0;
const REFERENCE_FRAMES_PER_SECOND: f64 = 60.0;
const REFERENCE_BUDGET_BYTES: f64 = 8_000_000_000.0;

#[derive(Clone, Copy)]
struct Setting {
    enabled: bool,
    min_payload_bytes: usize,
    max_retained_bytes: usize,
    label: &'static str,
}

const SETTINGS: [Setting; 4] = [
    Setting {
        enabled: false,
        min_payload_bytes: usize::MAX,
        max_retained_bytes: 0,
        label: "off",
    },
    Setting {
        enabled: true,
        min_payload_bytes: 0,
        max_retained_bytes: usize::MAX,
        label: "threshold 0",
    },
    Setting {
        enabled: true,
        min_payload_bytes: THRESHOLD_BYTES,
        max_retained_bytes: usize::MAX,
        label: "threshold 32",
    },
    Setting {
        enabled: true,
        min_payload_bytes: 0,
        max_retained_bytes: RETENTION_BUDGET_BYTES,
        label: "budget 1440",
    },
];

#[derive(Clone, Copy)]
struct Shape {
    id: &'static str,
    label: &'static str,
    payload_bytes: u64,
}

const SHAPES: [Shape; 4] = [
    Shape {
        id: "i32",
        label: "1 x i32",
        payload_bytes: 4,
    },
    Shape {
        id: "vec3",
        label: "vector: 3 x f32",
        payload_bytes: 12,
    },
    Shape {
        id: "particle",
        label: "particle: 8 x f32",
        payload_bytes: 32,
    },
    Shape {
        id: "large",
        label: "large: 32 x f32",
        payload_bytes: 128,
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

    let mut settings = Vec::with_capacity(SETTINGS.len());
    for setting in SETTINGS {
        let mut shapes = Vec::with_capacity(SHAPES.len());
        for (index, shape) in SHAPES.iter().copied().enumerate() {
            let frame_counts = if index == 0 {
                &LINEARITY_FRAME_COUNTS[..]
            } else {
                &SWEEP_FRAME_COUNTS[..]
            };
            shapes.push(measure_shape(setting, shape, frame_counts)?);
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
    frame_counts: &[u64],
) -> Result<ShapeResult, String> {
    let free = measure_variant(setting, shape, Variant::Free, frame_counts)?;
    let collect = measure_variant(setting, shape, Variant::Collect, frame_counts)?;
    require_constant_live_set(setting, shape, &free)?;
    require_constant_live_set(setting, shape, &collect)?;
    Ok(ShapeResult {
        shape,
        payload_bytes: shape.payload_bytes,
        free,
        collect,
    })
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
            let mut ctx = Context::new();
            if !ctx.set_freed_handle_diagnostics(
                setting.enabled,
                setting.min_payload_bytes,
                setting.max_retained_bytes,
            ) {
                return Err(format!(
                    "diagnostics {} was refused before the first allocation",
                    setting.label
                ));
            }
            for _ in 0..frames {
                let allocation = ctx.alloc(shape.payload_bytes as usize, 1, 0);
                if allocation.is_null() || ctx.trapped() {
                    return Err(format!(
                        "diagnostics {} shape {} variant {} at {frames} frames failed to allocate",
                        setting.label,
                        shape.id,
                        variant.id()
                    ));
                }
                match variant {
                    Variant::Free => ctx.delete(allocation as usize, 0),
                    Variant::Collect => ctx.collect(),
                }
            }
            Ok(Sample {
                frames,
                live_bytes: ctx.live_bytes() as u64,
                reserved_bytes: ctx.reserved_bytes() as u64,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(VariantResult { variant, samples })
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
    let [off, threshold_zero, threshold, budget] = settings else {
        return Err(
            "the probe requires off, threshold-0, thresholded, and budgeted settings".to_string(),
        );
    };
    println!();
    println!("object-size sweep, settings side by side");
    println!(
        "{:<22} {:>9} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12} {:>15} {:>15} {:>12} {:>12}",
        "shape",
        "payload",
        "off A B/alloc",
        "off B B/alloc",
        "min0 A B/alloc",
        "min0 B B/alloc",
        "min32 A B/alloc",
        "min32 B B/alloc",
        "budget A B/alloc",
        "budget B B/alloc",
        "min0 A hours",
        "min0 B hours"
    );
    for (((off_shape, threshold_zero_shape), threshold_shape), budget_shape) in off
        .shapes
        .iter()
        .zip(&threshold_zero.shapes)
        .zip(&threshold.shapes)
        .zip(&budget.shapes)
    {
        if off_shape.shape.id != threshold_zero_shape.shape.id
            || off_shape.shape.id != threshold_shape.shape.id
            || off_shape.shape.id != budget_shape.shape.id
            || off_shape.payload_bytes != threshold_zero_shape.payload_bytes
            || off_shape.payload_bytes != threshold_shape.payload_bytes
            || off_shape.payload_bytes != budget_shape.payload_bytes
        {
            return Err("setting sweeps do not contain the same shapes".to_string());
        }
        let off_a = sweep_growth(&off_shape.free)?;
        let off_b = sweep_growth(&off_shape.collect)?;
        let threshold_zero_a = sweep_growth(&threshold_zero_shape.free)?;
        let threshold_zero_b = sweep_growth(&threshold_zero_shape.collect)?;
        let threshold_a = sweep_growth(&threshold_shape.free)?;
        let threshold_b = sweep_growth(&threshold_shape.collect)?;
        let budget_a = sweep_growth(&budget_shape.free)?;
        let budget_b = sweep_growth(&budget_shape.collect)?;
        for (setting, variant, growth) in [
            (off.setting, "A", &off_a),
            (off.setting, "B", &off_b),
            (threshold_zero.setting, "A", &threshold_zero_a),
            (threshold_zero.setting, "B", &threshold_zero_b),
            (threshold.setting, "A", &threshold_a),
            (threshold.setting, "B", &threshold_b),
            (budget.setting, "A", &budget_a),
            (budget.setting, "B", &budget_b),
        ] {
            let expected = expected_growth(setting, off_shape.payload_bytes);
            if growth.exact_per_allocation != Some(expected) {
                return Err(format!(
                    "diagnostics {} shape {} variant {variant} measured {:.3} bytes/allocation; \
                     expected {expected}",
                    setting.label,
                    off_shape.shape.id,
                    growth.per_allocation
                ));
            }
        }
        println!(
            "{:<22} {:>9} {:>12.3} {:>12.3} {:>12.3} {:>12.3} {:>12.3} {:>12.3} {:>15.3} {:>15.3} {:>12.6} {:>12.6}",
            off_shape.shape.label,
            off_shape.payload_bytes,
            off_a.per_allocation,
            off_b.per_allocation,
            threshold_zero_a.per_allocation,
            threshold_zero_b.per_allocation,
            threshold_a.per_allocation,
            threshold_b.per_allocation,
            budget_a.per_allocation,
            budget_b.per_allocation,
            reference_hours(threshold_zero_a.per_allocation)?,
            reference_hours(threshold_zero_b.per_allocation)?
        );
    }
    println!("mode-off reference-budget duration: unbounded when growth/allocation is zero");
    Ok(())
}

fn print_rule(result: &SettingResult) -> Result<(), String> {
    for shape in &result.shapes {
        for variant in [&shape.free, &shape.collect] {
            if result.setting.enabled
                && result.setting.max_retained_bytes != usize::MAX
                && variant.samples.iter().any(|sample| {
                    sample.reserved_bytes > result.setting.max_retained_bytes as u64
                })
            {
                return Err(format!(
                    "diagnostics {} shape {} variant {} exceeded its retention budget",
                    result.setting.label,
                    shape.shape.id,
                    variant.variant.id()
                ));
            }
            let growth = sweep_growth(variant)?;
            let expected = expected_growth(result.setting, shape.payload_bytes);
            if growth.exact_per_allocation != Some(expected) {
                return Err(format!(
                    "diagnostics {} shape {} variant {} violates its retention rule",
                    result.setting.label,
                    shape.shape.id,
                    variant.variant.id()
                ));
            }
        }
    }
    println!();
    if !result.setting.enabled {
        println!(
            "rule, diagnostics off: reserved growth/allocation = 0 bytes exactly across all \
             measured shapes and both variants"
        );
    } else if result.setting.max_retained_bytes != usize::MAX {
        println!(
            "rule, diagnostics {}: reserved bytes plateau at or below {} bytes and subsequent \
             growth/allocation = 0 across all measured shapes and both variants",
            result.setting.label, result.setting.max_retained_bytes
        );
    } else if result.setting.min_payload_bytes == 0 {
        println!(
            "rule, diagnostics threshold 0: reserved growth/allocation = payload bytes + {} bytes \
             exactly across all measured shapes and both variants",
            HEADER_SIZE
        );
    } else {
        println!(
            "rule, diagnostics threshold {}: reserved growth/allocation = 0 below the threshold; \
             payload bytes + {} bytes at or above it, exactly across both variants",
            result.setting.min_payload_bytes, HEADER_SIZE
        );
    }
    Ok(())
}

fn expected_growth(setting: Setting, payload_bytes: u64) -> u64 {
    if setting.enabled
        && setting.max_retained_bytes == usize::MAX
        && payload_bytes >= setting.min_payload_bytes as u64
    {
        payload_bytes + HEADER_SIZE as u64
    } else {
        0
    }
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
