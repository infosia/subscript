//! Generates one native archive and one module for the ABI and result sweeps.
use std::fmt::Write;
use std::path::Path;

pub fn generate(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut header = String::from("#include <stdint.h>\n#include <stdbool.h>\n#include \"interop.h\"\n#include \"host-completion.h\"\n");
    let mut c = String::from("#include \"class-sweep.h\"\nextern void *subCompletionDeviceContext(SubDevice);\nextern int32_t subscript_rt_complete_value(void *, subscript_rt_completion, const void *, size_t);\n");
    let mut mirror = String::from("// @subscript-c-header include=\"class-sweep.h\"\n");
    let mut script = String::from("export function main(): void {\n");
    let mut symbols = String::from("extern \"C\" {\n");
    let mut addresses = String::new();
    let mut cases = 0;
    // SIMD kinds cover zero, last available, and exhausted general-register counts.
    let kinds = [
        ("Integer", "int32_t", "i32", "31", "value == 31", false),
        ("Float", "float", "f32", "31", "value == 31", true),
        ("Double", "double", "f64", "31", "value == 31", true),
        (
            "Endpoint",
            "subscript_rt_completion",
            "subscript_rt_completion",
            "new subscript_rt_completion(31, 37)",
            "value.context_id == 31 && value.operation_id == 37",
            false,
        ),
        (
            "Small",
            "SweepSmall",
            "SweepSmall",
            "new SweepSmall(31, 37, 41)",
            "value.x == 31 && value.y == 37 && value.z == 41",
            false,
        ),
        (
            "Large",
            "SweepLarge",
            "SweepLarge",
            "new SweepLarge(31, 37, 41)",
            "value.x == 31 && value.y == 37 && value.z == 41",
            false,
        ),
        (
            "F32x2",
            "SweepF32x2",
            "SweepF32x2",
            "new SweepF32x2(31, 37)",
            "value.x == 31 && value.y == 37",
            true,
        ),
        (
            "F32x4",
            "SweepF32x4",
            "SweepF32x4",
            "new SweepF32x4(31, 37, 41, 43)",
            "value.x == 31 && value.y == 37 && value.z == 41 && value.w == 43",
            true,
        ),
        (
            "F64x2",
            "SweepF64x2",
            "SweepF64x2",
            "new SweepF64x2(31, 37)",
            "value.x == 31 && value.y == 37",
            true,
        ),
        (
            "F64x4",
            "SweepF64x4",
            "SweepF64x4",
            "new SweepF64x4(31, 37, 41, 43)",
            "value.x == 31 && value.y == 37 && value.z == 41 && value.w == 43",
            true,
        ),
        (
            "F32x1",
            "SweepF32x1",
            "SweepF32x1",
            "new SweepF32x1(31)",
            "value.x == 31",
            true,
        ),
        (
            "F32x3",
            "SweepF32x3",
            "SweepF32x3",
            "new SweepF32x3(31, 37, 41)",
            "value.x == 31 && value.y == 37 && value.z == 41",
            true,
        ),
        (
            "F64x1",
            "SweepF64x1",
            "SweepF64x1",
            "new SweepF64x1(31)",
            "value.x == 31",
            true,
        ),
        (
            "F64x3",
            "SweepF64x3",
            "SweepF64x3",
            "new SweepF64x3(31, 37, 41)",
            "value.x == 31 && value.y == 37 && value.z == 41",
            true,
        ),
    ];
    for (name, fields) in [
        ("SweepF32x1", vec![("x", "float", "f32")]),
        (
            "SweepF32x3",
            vec![
                ("x", "float", "f32"),
                ("y", "float", "f32"),
                ("z", "float", "f32"),
            ],
        ),
        ("SweepF64x1", vec![("x", "double", "f64")]),
        (
            "SweepF64x3",
            vec![
                ("x", "double", "f64"),
                ("y", "double", "f64"),
                ("z", "double", "f64"),
            ],
        ),
        (
            "SweepSmall",
            vec![
                ("x", "int32_t", "i32"),
                ("y", "int32_t", "i32"),
                ("z", "int32_t", "i32"),
            ],
        ),
        (
            "SweepLarge",
            vec![
                ("x", "uint64_t", "u64"),
                ("y", "uint64_t", "u64"),
                ("z", "uint64_t", "u64"),
            ],
        ),
        (
            "SweepF32x2",
            vec![("x", "float", "f32"), ("y", "float", "f32")],
        ),
        (
            "SweepF32x4",
            vec![
                ("x", "float", "f32"),
                ("y", "float", "f32"),
                ("z", "float", "f32"),
                ("w", "float", "f32"),
            ],
        ),
        (
            "SweepF64x2",
            vec![("x", "double", "f64"), ("y", "double", "f64")],
        ),
        (
            "SweepF64x4",
            vec![
                ("x", "double", "f64"),
                ("y", "double", "f64"),
                ("z", "double", "f64"),
                ("w", "double", "f64"),
            ],
        ),
    ] {
        writeln!(
            header,
            "typedef struct {{ {} }} {name};",
            fields
                .iter()
                .map(|(f, ct, _)| format!("{ct} {f};"))
                .collect::<Vec<_>>()
                .join(" ")
        )?;
        writeln!(
            mirror,
            "declare class {name} {{ {} constructor({}); }}",
            fields
                .iter()
                .map(|(f, _, ts)| format!("{f}: {ts};"))
                .collect::<Vec<_>>()
                .join(" "),
            fields
                .iter()
                .map(|(f, _, ts)| format!("{f}: {ts}"))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    mirror.push_str("declare class subscript_rt_completion { context_id: u64; operation_id: u64; constructor(context_id: u64, operation_id: u64); }\n");
    for (kind, ct, ts, arg, test, simd) in kinds {
        let general_counts: Vec<_> = if simd {
            vec![0, 7, 8]
        } else {
            (0..=8).collect()
        };
        for g in general_counts {
            for f in 0..=if simd { 8 } else { 0 } {
                let name = format!("sweep{kind}G{g}F{f}");
                let mut cp = Vec::new();
                let mut tp = Vec::new();
                let mut args = Vec::new();
                let mut checks = Vec::new();
                for i in 0..g {
                    cp.push(format!("int32_t g{i}"));
                    tp.push(format!("g{i}: i32"));
                    args.push((11 + i).to_string());
                    checks.push(format!("g{i} == {}", 11 + i));
                }
                for i in 0..f {
                    let (ct, ts) = if kind.starts_with("F32") || kind == "Float" {
                        ("float", "f32")
                    } else {
                        ("double", "f64")
                    };
                    cp.push(format!("{ct} f{i}"));
                    tp.push(format!("f{i}: {ts}"));
                    args.push((21 + i).to_string());
                    checks.push(format!("f{i} == {}", 21 + i));
                }
                cp.extend([
                    format!("{ct} value"),
                    "float ftail".into(),
                    "double dtail".into(),
                    "int32_t tail".into(),
                ]);
                tp.extend([
                    format!("value: {ts}"),
                    "ftail: f32".into(),
                    "dtail: f64".into(),
                    "tail: i32".into(),
                ]);
                args.extend([arg.to_string(), "47".into(), "53".into(), "97".into()]);
                checks.extend([
                    test.into(),
                    "ftail == 47".into(),
                    "dtail == 53".into(),
                    "tail == 97".into(),
                ]);
                writeln!(header, "int32_t {name}({});", cp.join(", "))?;
                writeln!(
                    c,
                    "int32_t {name}({}) {{ return {}; }}",
                    cp.join(", "),
                    checks.join(" && ")
                )?;
                writeln!(mirror, "declare function {name}({}): i32;", tp.join(", "))?;
                writeln!(script, "print(`${{{name}({})}}`);", args.join(", "))?;
                writeln!(symbols, "fn {name}();")?;
                writeln!(addresses, "({name:?}.into(), {name} as *const u8),")?;
                cases += 1;
            }
        }
    }
    script.push_str("}\n");
    let mut result_c = String::from("#include \"class-sweep.h\"\nextern void *subCompletionDeviceContext(SubDevice);\nextern int32_t subscript_rt_complete_value(void *, subscript_rt_completion, const void *, size_t);\n");
    let mut result_mirror = String::from("// @subscript-c-header include=\"class-sweep.h\"\n");
    let mut result_script = String::from("export async function main(): Promise<void> {\n const device = subDeviceCreate(null);\n subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));\n");
    header.push_str("typedef uint32_t SweepAlias;\ntypedef enum { SweepIdle = 0, SweepActive = 1 } SweepMode;\ntypedef struct { uint8_t x; double y; } SweepResult;\n");
    result_mirror.push_str("type SweepAlias = u32;\ndeclare enum SweepMode { SweepIdle = 0, SweepActive = 1 }\ndeclare class SweepResult { x: u8; y: f64; constructor(x: u8, y: f64); }\n");
    let mut producers = String::new();
    for (i, (ct, ts, val, expr)) in [
        ("int8_t", "i8", "-31", "value == -31"),
        ("uint8_t", "u8", "231", "value == 231"),
        ("int16_t", "i16", "-31001", "value == -31001"),
        ("uint16_t", "u16", "61001", "value == 61001"),
        ("int32_t", "i32", "-31000001", "value == -31000001"),
        ("uint32_t", "u32", "3100000001", "value == 3100000001"),
        (
            "int64_t",
            "i64",
            "-1234567890123",
            "value == -1234567890123",
        ),
        (
            "uint64_t",
            "u64",
            "12345678901234",
            "value == 12345678901234",
        ),
        ("bool", "boolean", "true", "value"),
        ("_Float16", "f16", "1.5", "(value as f32) == 1.5"),
        ("float", "f32", "31.5", "value == 31.5"),
        ("double", "f64", "37.5", "value == 37.5"),
        (
            "SweepMode",
            "SweepMode",
            "SweepActive",
            "value == SweepMode.SweepActive",
        ),
        (
            "SweepAlias",
            "SweepAlias",
            "3100000001",
            "value == 3100000001",
        ),
        (
            "SweepResult",
            "SweepResult",
            "{231, 37.5}",
            "value.x == 231 && value.y == 37.5",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("sweepResult{i}");
        writeln!(
            header,
            "void {name}(SubDevice device, subscript_rt_completion endpoint);"
        )?;
        writeln!(result_c,"void {name}(SubDevice device, subscript_rt_completion endpoint) {{ {ct} value = {val}; subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value); }}")?;
        writeln!(result_mirror,"// @subscript-c-completion function=\"{name}\" result=\"{ct}\"\ndeclare function {name}(device: SubDevice): Promise<{ts}>;")?;
        let literal = match ts {
            "SweepMode" => "SweepMode.SweepActive".to_string(),
            "SweepResult" => "new SweepResult(231, 37.5)".to_string(),
            "boolean" => "true".to_string(),
            _ => val.to_string(),
        };
        // Each producer suspends before completion; all three readers consume its C bytes.
        writeln!(producers, "async function resultFunction{i}(): Promise<{ts}> {{ await Context.suspend(); const result: {ts} = {literal}; return result; }}")?;
        writeln!(producers, "class ResultMethod{i} {{ async value(): Promise<{ts}> {{ await Context.suspend(); const result: {ts} = {literal}; return result; }} }}")?;
        writeln!(result_script, "const method{i} = new ResultMethod{i}();\nconst closure{i} = async (): Promise<{ts}> => {{ await Context.suspend(); const result: {ts} = {literal}; return result; }};")?;
        for call in [
            format!("{name}(device)"),
            format!("resultFunction{i}()"),
            format!("method{i}.value()"),
            format!("closure{i}()"),
        ] {
            writeln!(result_script, "{{ const value = await {call}; print(`${{{expr}}}`); }}\n{{ const pending = {call}; const value = await pending; print(`${{{expr}}}`); }}\n{{ const values = await Promise.all([{call}, {call}]); for (const value of values) {{ print(`${{{expr}}}`); }} }}")?;
        }
        writeln!(symbols, "fn {name}();")?;
        writeln!(addresses, "({name:?}.into(), {name} as *const u8),")?;
    }
    result_script.push_str("const booleans = await Promise.all([closure8(), falseBool()]); print(`${booleans[0] && !booleans[1]}`);\nsubDeviceRelease(device);\n}\n");
    producers.push_str(
        "async function falseBool(): Promise<boolean> { await Context.suspend(); return false; }\n",
    );
    result_script.push_str(&producers);
    symbols.push_str("}\n/// Addresses of generated C callees.\npub fn class_symbols() -> Vec<(String, *const u8)> { vec![\n");
    symbols.push_str(&addresses);
    symbols.push_str("] }\n");
    for (name, content) in [
        ("class-sweep.h", header),
        ("class-sweep.c", c),
        ("class-results.c", result_c),
        ("class-mirror.d.ts", mirror),
        ("class-script.ts", script),
        ("result-mirror.d.ts", result_mirror),
        ("result-script.ts", result_script),
        ("class-symbols.rs", symbols),
    ] {
        std::fs::write(out.join(name), content)?;
    }
    std::fs::write(out.join("class-count.txt"), cases.to_string())?;
    Ok(())
}
