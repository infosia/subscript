//! Generates one native archive and one module for the ABI and result sweeps.
use std::fmt::Write;
use std::path::Path;

pub fn generate(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut header = String::from("#include <stdint.h>\n#include <stdbool.h>\n#include <string.h>\n#include \"interop.h\"\n#include \"host-completion.h\"\n");
    let mut c = String::from("#include \"class-sweep.h\"\nextern void *subCompletionDeviceContext(SubDevice);\nextern int32_t subscript_rt_complete_value(void *, subscript_rt_completion, const void *, size_t);\n");
    let mut mirror = String::from("// @subscript-c-header include=\"class-sweep.h\"\n");
    let mut script = String::from("export function main(): void {\n");
    let mut symbols = String::from("extern \"C\" {\n");
    let mut addresses = String::new();
    let mut cases = 0;
    // SIMD kinds cover zero, last available, and exhausted general-register counts.
    let mut kinds = vec![
        ("I8", "int8_t", "i8", "-31", "value == -31", false),
        ("I16", "int16_t", "i16", "-31001", "value == -31001", false),
        (
            "Half",
            "_Float16",
            "f16",
            "1.5",
            "sweepHalfBits(value) == 0x3e00",
            true,
        ),
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
    let half_names = ["SweepF16x1", "SweepF16x2", "SweepF16x3", "SweepF16x4"];
    let half_args = [
        "new SweepF16x1(31)",
        "new SweepF16x2(31, 37)",
        "new SweepF16x3(31, 37, 41)",
        "new SweepF16x4(31, 37, 41, 43)",
    ];
    let half_tests = ["sweepHalfBits(value.x) == 0x4fc0", "sweepHalfBits(value.x) == 0x4fc0 && sweepHalfBits(value.y) == 0x50a0", "sweepHalfBits(value.x) == 0x4fc0 && sweepHalfBits(value.y) == 0x50a0 && sweepHalfBits(value.z) == 0x5120", "sweepHalfBits(value.x) == 0x4fc0 && sweepHalfBits(value.y) == 0x50a0 && sweepHalfBits(value.z) == 0x5120 && sweepHalfBits(value.w) == 0x5160"];
    for i in 0..4 {
        kinds.push((
            ["F16x1", "F16x2", "F16x3", "F16x4"][i],
            half_names[i],
            half_names[i],
            half_args[i],
            half_tests[i],
            true,
        ));
    }
    c.push_str("static uint16_t sweepHalfBits(_Float16 value) { uint16_t bits; memcpy(&bits, &value, sizeof bits); return bits; }\n");
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
    ]
    .into_iter()
    .chain(half_names.iter().enumerate().map(|(i, name)| {
        (
            *name,
            ["x", "y", "z", "w"][..=i]
                .iter()
                .map(|field| (*field, "_Float16", "f16"))
                .collect(),
        )
    })) {
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
                let test = if ct == "float" || ct == "double" {
                    format!("memcmp(&value, &({ct}){{31}}, sizeof value) == 0")
                } else if kind.starts_with("F32") || kind.starts_with("F64") {
                    let n: usize = kind[4..].parse()?;
                    let fields = ["x", "y", "z", "w"];
                    let values = [31, 37, 41, 43];
                    let leaf = if kind.starts_with("F32") {
                        "float"
                    } else {
                        "double"
                    };
                    (0..n)
                        .map(|i| {
                            format!(
                                "memcmp(&value.{}, &({leaf}){{{}}}, sizeof value.{}) == 0",
                                fields[i], values[i], fields[i]
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" && ")
                } else {
                    test.into()
                };
                checks.extend([
                    test,
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
    for (ct, ts, values) in [
        ("int8_t", "i8", vec![-31.0]),
        ("uint8_t", "u8", vec![231.0]),
        ("int16_t", "i16", vec![-31001.0]),
        ("uint16_t", "u16", vec![61001.0]),
        ("bool", "boolean", vec![0.0]),
        ("_Float16", "f16", vec![1.5]),
        ("float", "f32", vec![31.0]),
        ("double", "f64", vec![31.0]),
    ] {
        let name = format!("sweepReturn{ts}");
        let check = format!("sweepCheck{ts}");
        writeln!(header, "{ct} {name}(void); int32_t {check}({ct} value);")?;
        writeln!(c, "{ct} {name}(void) {{ return {}; }} int32_t {check}({ct} value) {{ const {ct} expected = {}; return memcmp(&value, &expected, sizeof value) == 0; }}", values[0], values[0])?;
        writeln!(
            mirror,
            "declare function {name}(): {ts}; declare function {check}(value: {ts}): i32;"
        )?;
        writeln!(script, "print(`${{{check}({name}())}}`);")?;
        for symbol in [&name, &check] {
            writeln!(symbols, "fn {symbol}();")?;
            writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
        }
        cases += 1;
    }
    for ts in ["F16", "F32", "F64"] {
        for n in 1..=4 {
            let ct = format!("Sweep{ts}x{n}");
            let name = format!("sweepReturn{ts}x{n}");
            let check = format!("sweepCheck{ts}x{n}");
            let values = [31, 37, 41, 43][..n]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(header, "{ct} {name}(void); int32_t {check}({ct} value);")?;
            writeln!(c, "{ct} {name}(void) {{ return ({ct}){{{values}}}; }} int32_t {check}({ct} value) {{ const {ct} expected = {{{values}}}; return memcmp(&value, &expected, sizeof value) == 0; }}")?;
            writeln!(
                mirror,
                "declare function {name}(): {ct}; declare function {check}(value: {ct}): i32;"
            )?;
            writeln!(script, "print(`${{{check}({name}())}}`);")?;
            for symbol in [&name, &check] {
                writeln!(symbols, "fn {symbol}();")?;
                writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
            }
            cases += 1;
        }
    }
    header.push_str("typedef struct { uint8_t head; _Float16 value; uint8_t tail; } SweepHalfField;\ntypedef struct { SubStringView label; _Float16 value; } SweepStringHalf;\nint32_t sweepHalfField(SweepHalfField value); SweepHalfField sweepHalfFieldReturn(void); void sweepHalfFieldWrite(SweepHalfField *value); void sweepStringHalfWrite(SweepStringHalf *value);\n");
    mirror.push_str("declare class SweepHalfField { head: u8; value: f16; tail: u8; constructor(head: u8, value: f16, tail: u8); }\ndeclare class SweepStringHalf { label: string; value: f16; constructor(label: string, value: f16); }\ndeclare function sweepHalfField(value: SweepHalfField): i32; declare function sweepHalfFieldReturn(): SweepHalfField; declare function sweepHalfFieldWrite(value: SweepHalfField | null): void; declare function sweepStringHalfWrite(value: SweepStringHalf | null): void;\n");
    c.push_str("int32_t sweepHalfField(SweepHalfField value) { return value.head == 7 && sweepHalfBits(value.value) == 0x3e00 && value.tail == 9; }\nSweepHalfField sweepHalfFieldReturn(void) { SweepHalfField value; memset(&value, 0xa5, sizeof value); value.head = 7; value.value = 1.5; value.tail = 9; return value; }\nvoid sweepHalfFieldWrite(SweepHalfField *value) { *value = sweepHalfFieldReturn(); }\nvoid sweepStringHalfWrite(SweepStringHalf *value) { value->label = (SubStringView){\"updated\", 7}; value->value = 1.5; }\n");
    script.push_str("print(`${sweepHalfField(new SweepHalfField(7, 1.5, 9))}`);\nprint(`${sweepHalfField(sweepHalfFieldReturn())}`);\nconst field = new SweepHalfField(0, 0, 0); sweepHalfFieldWrite(field); print(`${sweepHalfField(field)}`);\nconst scratch = new SweepStringHalf(\"before\", 0); sweepStringHalfWrite(scratch); print(`${scratch.label == \"updated\" ? sweepCheckf16(scratch.value) : 0}`);\n");
    for symbol in [
        "sweepHalfField",
        "sweepHalfFieldReturn",
        "sweepHalfFieldWrite",
        "sweepStringHalfWrite",
    ] {
        writeln!(symbols, "fn {symbol}();")?;
        writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
    }
    header.push_str("typedef struct { uint32_t marker; SweepStringHalf inner; } SweepNestedHalf;\nvoid sweepNestedHalfWrite(SweepNestedHalf *value);\n");
    c.push_str("void sweepNestedHalfWrite(SweepNestedHalf *value) { sweepStringHalfWrite(&value->inner); }\n");
    mirror.push_str("declare class SweepNestedHalf { marker: u32; inner: SweepStringHalf; constructor(marker: u32, inner: SweepStringHalf); }\ndeclare function sweepNestedHalfWrite(value: SweepNestedHalf | null): void;\n");
    script.push_str("const nested = new SweepNestedHalf(0, new SweepStringHalf(\"before\", 0)); sweepNestedHalfWrite(nested); print(`${nested.inner.label == \"updated\" ? sweepCheckf16(nested.inner.value) : 0}`);\n");
    writeln!(symbols, "fn sweepNestedHalfWrite();")?;
    writeln!(
        addresses,
        "(\"sweepNestedHalfWrite\".into(), sweepNestedHalfWrite as *const u8),"
    )?;
    cases += 1;
    cases += 4;
    for n in 1..=4 {
        let ct = format!("SweepF16x{n}");
        let name = format!("sweepWriteF16x{n}");
        writeln!(header, "void {name}({ct} *value);")?;
        writeln!(
            c,
            "void {name}({ct} *value) {{ *value = sweepReturnF16x{n}(); }}"
        )?;
        writeln!(mirror, "declare function {name}(value: {ct} | null): void;")?;
        writeln!(script, "const half{n} = new {ct}({}); {name}(half{n}); print(`${{sweepCheckF16x{n}(half{n})}}`);", vec!["0"; n].join(", "))?;
        writeln!(symbols, "fn {name}();")?;
        writeln!(addresses, "({name:?}.into(), {name} as *const u8),")?;
        cases += 1;
    }
    script.push_str("}\n");
    header.push_str("typedef struct { bool a; bool b; } SweepBoolPair;\ntypedef struct { bool a; int32_t b; bool c; } SweepBoolPadded;\ntypedef struct { SweepBoolPair inner; bool tail; } SweepBoolNested;\nSweepBoolPadded sweepBoolReturn(void); void sweepBoolWrite(SweepBoolPadded *value);\n");
    c.push_str("SweepBoolPadded sweepBoolReturn(void) { SweepBoolPadded value; memset(&value, 0xa5, sizeof value); value.a = false; value.b = 37; value.c = false; return value; }\nvoid sweepBoolWrite(SweepBoolPadded *value) { *value = sweepBoolReturn(); }\n");
    mirror.push_str("declare class SweepBoolPair { a: boolean; b: boolean; constructor(a: boolean, b: boolean); }\ndeclare class SweepBoolPadded { a: boolean; b: i32; c: boolean; constructor(a: boolean, b: i32, c: boolean); }\ndeclare class SweepBoolNested { inner: SweepBoolPair; tail: boolean; constructor(inner: SweepBoolPair, tail: boolean); }\ndeclare function sweepBoolReturn(): SweepBoolPadded; declare function sweepBoolWrite(value: SweepBoolPadded | null): void;\n");
    header.push_str("int32_t sweepBoolPairBytes(SweepBoolPair value);\n");
    c.push_str("int32_t sweepBoolPairBytes(SweepBoolPair value) { const unsigned char expected[2] = {0, 1}; return sizeof value == 2 && memcmp(&value, expected, 2) == 0; }\n");
    mirror.push_str("declare function sweepBoolPairBytes(value: SweepBoolPair): i32;\n");
    for symbol in ["sweepBoolReturn", "sweepBoolWrite", "sweepBoolPairBytes"] {
        writeln!(symbols, "fn {symbol}();")?;
        writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
    }
    let mut result_c = String::from("#include \"class-sweep.h\"\nextern void *subCompletionDeviceContext(SubDevice);\nextern int32_t subscript_rt_complete_value(void *, subscript_rt_completion, const void *, size_t);\n");
    let mut result_mirror = String::from("// @subscript-c-header include=\"class-sweep.h\"\n");
    let mut result_script = String::from("export async function main(): Promise<void> {\n const device = subDeviceCreate(null);\n subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));\n");
    header.push_str("typedef uint32_t SweepAlias;\ntypedef enum { SweepIdle = 0, SweepActive = 1 } SweepMode;\ntypedef struct { uint8_t x; double y; } SweepResult;\n");
    result_mirror.push_str("type SweepAlias = u32;\ndeclare enum SweepMode { SweepIdle = 0, SweepActive = 1 }\ndeclare class SweepResult { x: u8; y: f64; constructor(x: u8, y: f64); }\n");
    result_mirror.push_str("declare class SweepBoolPair { a: boolean; b: boolean; constructor(a: boolean, b: boolean); }\ndeclare class SweepBoolPadded { a: boolean; b: i32; c: boolean; constructor(a: boolean, b: i32, c: boolean); }\ndeclare class SweepBoolNested { inner: SweepBoolPair; tail: boolean; constructor(inner: SweepBoolPair, tail: boolean); }\n");
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
        ("_Float16", "f16", "1.5", "sweepCheckf16(value) == 1"),
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
        (
            "SweepBoolPair",
            "SweepBoolPair",
            "{false, true}",
            "!value.a && value.b",
        ),
        (
            "SweepBoolPadded",
            "SweepBoolPadded",
            "{false, 37, false}",
            "!value.a && value.b == 37 && !value.c",
        ),
        (
            "SweepBoolNested",
            "SweepBoolNested",
            "{{false, true}, false}",
            "!value.inner.a && value.inner.b && !value.tail",
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
        if ct == "SweepBoolPadded" {
            writeln!(result_c, "void {name}(SubDevice device, subscript_rt_completion endpoint) {{ {ct} value; memset(&value, 0xa5, sizeof value); value.a = false; value.b = 37; value.c = false; subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value); }}")?;
        } else {
            writeln!(result_c,"void {name}(SubDevice device, subscript_rt_completion endpoint) {{ {ct} value = {val}; subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value); }}")?;
        }
        writeln!(result_mirror,"// @subscript-c-completion function=\"{name}\" result=\"{ct}\"\ndeclare function {name}(device: SubDevice): Promise<{ts}>;")?;
        let literal = match ts {
            "SweepMode" => "SweepMode.SweepActive".to_string(),
            "SweepResult" => "new SweepResult(231, 37.5)".to_string(),
            "SweepBoolPair" => "new SweepBoolPair(false, true)".to_string(),
            "SweepBoolPadded" => "new SweepBoolPadded(false, 37, false)".to_string(),
            "SweepBoolNested" => {
                "new SweepBoolNested(new SweepBoolPair(false, true), false)".to_string()
            }
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
    result_mirror.push_str("declare function sweepCheckf16(value: f16): i32;\n");
    for n in 1..=4 {
        let ct = format!("SweepF16x{n}");
        let name = format!("sweepCompleteF16x{n}");
        let fields = ["x", "y", "z", "w"][..n]
            .iter()
            .map(|field| format!("{field}: f16;"))
            .collect::<Vec<_>>()
            .join(" ");
        let params = ["x", "y", "z", "w"][..n]
            .iter()
            .map(|field| format!("{field}: f16"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(result_mirror, "declare class {ct} {{ {fields} constructor({params}); }}\ndeclare function sweepCheckF16x{n}(value: {ct}): i32;")?;
        writeln!(
            header,
            "void {name}(SubDevice device, subscript_rt_completion endpoint);"
        )?;
        writeln!(result_c, "void {name}(SubDevice device, subscript_rt_completion endpoint) {{ {ct} value = sweepReturnF16x{n}(); subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value); }}")?;
        writeln!(result_mirror, "// @subscript-c-completion function=\"{name}\" result=\"{ct}\"\ndeclare function {name}(device: SubDevice): Promise<{ct}>;")?;
        writeln!(result_script, "{{ const value = await {name}(device); print(`${{sweepCheckF16x{n}(value) == 1}}`); }}\n{{ const pending = {name}(device); const value = await pending; print(`${{sweepCheckF16x{n}(value) == 1}}`); }}\n{{ const values = await Promise.all([{name}(device), {name}(device)]); for (const value of values) {{ print(`${{sweepCheckF16x{n}(value) == 1}}`); }} }}")?;
        writeln!(symbols, "fn {name}();")?;
        writeln!(addresses, "({name:?}.into(), {name} as *const u8),")?;
    }
    result_script.push_str("const booleans = await Promise.all([closure8(), falseBool()]); print(`${booleans[0] && !booleans[1]}`);\nsubDeviceRelease(device);\n}\n");
    producers.push_str(
        "async function falseBool(): Promise<boolean> { await Context.suspend(); return false; }\n",
    );
    result_script.push_str(&producers);
    let (gate_mirror, gate_script, gate_count) =
        gate_cases(&mut header, &mut c, &mut symbols, &mut addresses, &script)?;
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
        ("gate-mirror.d.ts", gate_mirror),
        ("gate-script.ts", gate_script),
    ] {
        std::fs::write(out.join(name), content)?;
    }
    std::fs::write(out.join("class-count.txt"), cases.to_string())?;
    std::fs::write(out.join("gate-count.txt"), gate_count.to_string())?;
    Ok(())
}

// Keep allocator positions. Remove unrelated scalar returns and duplicate HFA returns.
fn gate_cases(
    header: &mut String,
    c: &mut String,
    symbols: &mut String,
    addresses: &mut String,
    wide: &str,
) -> Result<(String, String, usize), Box<dyn std::error::Error>> {
    let mut mirror = String::new();
    let mut body = String::new();
    let mut count = 0;
    for line in wide.lines().skip(1).filter(|line| *line != "}") {
        if line.contains("sweepReturn")
            && ![
                "sweepReturni8()",
                "sweepReturnf16()",
                "sweepReturnF16x2()",
                "sweepReturnF32x2()",
            ]
            .iter()
            .any(|name| line.contains(name))
        {
            continue;
        }
        if line.contains("const half")
            || line.contains("sweepHalfField")
            || line.contains("sweepStringHalfWrite")
            || line.contains("sweepNestedHalfWrite")
        {
            continue;
        }
        body.push_str(line);
        body.push('\n');
        count += usize::from(line.contains("print("));
    }
    writeln!(header, "uint32_t gateWide(void);")?;
    writeln!(c, "uint32_t gateWide(void) {{ return 70000; }}")?;
    writeln!(mirror, "declare function gateWide(): u32;")?;
    writeln!(symbols, "fn gateWide();")?;
    writeln!(addresses, "(\"gateWide\".into(), gateWide as *const u8),")?;
    for (name, ct, ts, expression, expected) in [
        ("U8", "uint8_t", "u8", "wide as u8", 112),
        ("U16", "uint16_t", "u16", "wide as u16", 4464),
        ("Bool", "bool", "boolean", "wide != 0", 1),
    ] {
        for g in [0, 7, 8] {
            let cp = (0..g)
                .map(|i| format!("int32_t g{i}, "))
                .collect::<String>();
            let tp = (0..g).map(|i| format!("g{i}: i32, ")).collect::<String>();
            let args = (0..g).map(|i| format!("{}, ", 11 + i)).collect::<String>();
            let fun = format!("gate{name}NarrowG{g}");
            writeln!(header, "uint32_t {fun}({cp}{ct} value);")?;
            let used = (0..g).map(|i| format!("(void)g{i}; ")).collect::<String>();
            writeln!(
                c,
                "uint32_t {fun}({cp}{ct} value) {{ {used}return value; }}"
            )?;
            writeln!(mirror, "declare function {fun}({tp}value: {ts}): u32;")?;
            writeln!(body, "{{ const wide = gateWide(); print(`${{{fun}({args}{expression}) == {expected} ? 1 : 0}}`); }}")?;
            count += 1;
            writeln!(symbols, "fn {fun}();")?;
            writeln!(addresses, "({fun:?}.into(), {fun} as *const u8),")?;
        }
    }
    for (name, ct, ts, literal, check) in [
        ("N", "int8_t", "i8", "-31", "value.value == -31"),
        (
            "F",
            "_Float16",
            "f16",
            "1.5",
            "sweepHalfBits(value.value) == 0x3e00",
        ),
        (
            "H",
            "SweepF16x2",
            "SweepF16x2",
            "new SweepF16x2(31, 37)",
            "sweepCheckF16x2(value.value) == 1",
        ),
        ("B", "bool", "boolean", "false", "!value.value"),
        (
            "R",
            "SweepF32x2",
            "SweepF32x2",
            "new SweepF32x2(31, 37)",
            "sweepCheckF32x2(value.value) == 1",
        ),
    ] {
        let field = format!("Gate{name}Field");
        writeln!(
            header,
            "typedef struct {{ uint8_t head; {ct} value; uint8_t tail; }} {field};"
        )?;
        writeln!(mirror, "declare class {field} {{ head: u8; value: {ts}; tail: u8; constructor(head: u8, value: {ts}, tail: u8); }}")?;
        let c_literal = match name {
            "H" => "(SweepF16x2){31, 37}",
            "R" => "(SweepF32x2){31, 37}",
            _ => literal,
        };
        for g in [0, 7, 8] {
            let cp = (0..g).map(|i| format!("int32_t g{i}")).collect::<Vec<_>>();
            let tp = (0..g).map(|i| format!("g{i}: i32")).collect::<Vec<_>>();
            let args = (0..g).map(|i| (11 + i).to_string()).collect::<Vec<_>>();
            let prefix = if g == 0 {
                String::new()
            } else {
                format!("{}, ", cp.join(", "))
            };
            let tprefix = if g == 0 {
                String::new()
            } else {
                format!("{}, ", tp.join(", "))
            };
            let aprefix = if g == 0 {
                String::new()
            } else {
                format!("{}, ", args.join(", "))
            };
            let checks = (0..g)
                .map(|i| format!("g{i} == {} && ", 11 + i))
                .collect::<String>();
            let arg = format!("gate{name}FieldG{g}");
            let write = format!("gate{name}WriteG{g}");
            writeln!(
                header,
                "int32_t {arg}({prefix}{field} value); void {write}({prefix}{field} *value);"
            )?;
            writeln!(c, "int32_t {arg}({prefix}{field} value) {{ return {checks}value.head == 7 && {check} && value.tail == 9; }} void {write}({prefix}{field} *value) {{ memset(value, 0xa5, sizeof *value); value->head = 7; value->value = {c_literal}; value->tail = ({checks}1) ? 9 : 0; }}")?;
            writeln!(mirror, "declare function {arg}({tprefix}value: {field}): i32; declare function {write}({tprefix}value: {field} | null): void;")?;
            writeln!(body, "print(`${{{arg}({aprefix}new {field}(7, {literal}, 9))}}`); {{ const field = new {field}(0, {literal}, 0); {write}({aprefix}field); print(`${{{arg}({aprefix}field)}}`); }}")?;
            count += 2;
            for symbol in [&arg, &write] {
                writeln!(symbols, "fn {symbol}();")?;
                writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
            }
        }
    }
    // Small and indirect results test field loads at both aggregate return forms.
    for (name, ct, ts, literal) in [
        ("N", "int8_t", "i8", "-31"),
        ("F", "_Float16", "f16", "1.5"),
        ("H", "SweepF16x2", "SweepF16x2", "(SweepF16x2){31, 37}"),
        ("B", "bool", "boolean", "false"),
        ("R", "SweepF32x2", "SweepF32x2", "(SweepF32x2){31, 37}"),
    ] {
        let field = format!("Gate{name}Field");
        let wide = format!("Gate{name}Wide");
        let small = format!("gate{name}Return");
        let large = format!("gate{name}IndirectReturn");
        writeln!(header, "typedef struct {{ uint64_t first; uint64_t last; uint64_t stack; {ct} value; }} {wide}; {field} {small}(void); {wide} {large}(void);")?;
        writeln!(c, "{field} {small}(void) {{ {field} value; memset(&value, 0xa5, sizeof value); value.head = 7; value.value = {literal}; value.tail = 9; return value; }} {wide} {large}(void) {{ {wide} value; memset(&value, 0xa5, sizeof value); value.first = 11; value.last = 17; value.stack = 19; value.value = {literal}; return value; }}")?;
        writeln!(mirror, "declare class {wide} {{ first: u64; last: u64; stack: u64; value: {ts}; constructor(first: u64, last: u64, stack: u64, value: {ts}); }} declare function {small}(): {field}; declare function {large}(): {wide};")?;
        writeln!(body, "print(`${{gate{name}FieldG0({small}())}}`); {{ const value = {large}(); print(`${{value.first == 11 && value.last == 17 && value.stack == 19 ? gate{name}FieldG0(new {field}(7, value.value, 9)) : 0}}`); }}")?;
        count += 2;
        for symbol in [&small, &large] {
            writeln!(symbols, "fn {symbol}();")?;
            writeln!(addresses, "({symbol:?}.into(), {symbol} as *const u8),")?;
        }
    }
    body.push_str("const padded = sweepBoolReturn(); print(`${!padded.a && padded.b == 37 && !padded.c ? 1 : 0}`); const written = new SweepBoolPadded(true, 0, true); sweepBoolWrite(written); print(`${!written.a && written.b == 37 && !written.c ? 1 : 0}`);\n");
    body.push_str("const bools = new GateBools(false, true); print(`${subBoundaryScriptSize(Context.bytesOf<GateBools>(bools)) == 2 ? 1 : 0}`);\n");
    count += 3;
    body.push_str("const device = subDeviceCreate(null); subRequestStart(device, 0, 1, new SubRequestInfo((message, a, b) => {}, null, null));\n");
    for (name, ct, ts, value, expr) in [
        ("N", "int8_t", "i8", "-31", "value == -31"),
        ("F", "_Float16", "f16", "1.5", "sweepCheckf16(value) == 1"),
        (
            "H",
            "SweepF16x2",
            "SweepF16x2",
            "{31, 37}",
            "sweepCheckF16x2(value) == 1",
        ),
        (
            "B",
            "SweepBoolPadded",
            "SweepBoolPadded",
            "{false, 37, false}",
            "!value.a && value.b == 37 && !value.c",
        ),
        (
            "R",
            "SweepF32x2",
            "SweepF32x2",
            "{31, 37}",
            "sweepCheckF32x2(value) == 1",
        ),
    ] {
        // The device occupies x0. Prefixes put the endpoint at x1, x7, and on the stack.
        for g in [0, 6, 7] {
            let cp = (0..g)
                .map(|i| format!(", int32_t g{i}"))
                .collect::<String>();
            let tp = (0..g).map(|i| format!(", g{i}: i32")).collect::<String>();
            let args = (0..g).map(|i| format!(", {}", 11 + i)).collect::<String>();
            let checks = (0..g)
                .map(|i| format!("g{i} == {} && ", 11 + i))
                .collect::<String>();
            let fun = format!("gateComplete{name}G{g}");
            writeln!(
                header,
                "void {fun}(SubDevice device{cp}, subscript_rt_completion endpoint);"
            )?;
            let init = if name == "B" {
                "memset(&value, 0xa5, sizeof value); value.a = false; value.b = 37; value.c = false;".to_string()
            } else {
                format!("value = ({ct}){value};")
            };
            writeln!(c,"void {fun}(SubDevice device{cp}, subscript_rt_completion endpoint) {{ {ct} value; {init} if ({checks}1) subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &value, sizeof value); }}")?;
            writeln!(mirror,"// @subscript-c-completion function=\"{fun}\" result=\"{ct}\"\ndeclare function {fun}(device: SubDevice{tp}): Promise<{ts}>;")?;
            // Three positions use the three distinct completion readers.
            match g {
                0=>writeln!(body,"{{ const value = await {fun}(device{args}); print(`${{{expr} ? 1 : 0}}`); }}")?,
                6=>writeln!(body,"{{ const pending = {fun}(device{args}); const value = await pending; print(`${{{expr} ? 1 : 0}}`); }}")?,
                _=>writeln!(body,"{{ const values = await Promise.all([{fun}(device{args})]); const value = values[0]; print(`${{{expr} ? 1 : 0}}`); }}")?,
            }
            count += 1;
            writeln!(symbols, "fn {fun}();")?;
            writeln!(addresses, "({fun:?}.into(), {fun} as *const u8),")?;
        }
    }
    // Complete with raw bytes, without a C bool read or store.
    writeln!(header, "void gateRawBool(SubDevice device, uint8_t raw, subscript_rt_completion endpoint); void gateRawPair(SubDevice device, uint8_t a, uint8_t b, subscript_rt_completion endpoint); int32_t gatePairBytes(SweepBoolPair *value, uint8_t a, uint8_t b);")?;
    writeln!(c, "void gateRawBool(SubDevice device, uint8_t raw, subscript_rt_completion endpoint) {{ subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, &raw, 1); }} void gateRawPair(SubDevice device, uint8_t a, uint8_t b, subscript_rt_completion endpoint) {{ unsigned char raw[2] = {{a, b}}; subscript_rt_complete_value(subCompletionDeviceContext(device), endpoint, raw, sizeof raw); }}")?;
    writeln!(mirror, "// @subscript-c-completion function=\"gateRawBool\" result=\"bool\"\ndeclare function gateRawBool(device: SubDevice, raw: u8): Promise<boolean>;\n// @subscript-c-completion function=\"gateRawPair\" result=\"SweepBoolPair\"\ndeclare function gateRawPair(device: SubDevice, a: u8, b: u8): Promise<SweepBoolPair>;")?;
    writeln!(c, "int32_t gatePairBytes(SweepBoolPair *value, uint8_t a, uint8_t b) {{ const unsigned char *raw = (const unsigned char*)value; return raw[0] == a && raw[1] == b; }}")?;
    writeln!(
        mirror,
        "declare function gatePairBytes(value: SweepBoolPair | null, a: u8, b: u8): i32;"
    )?;
    for (call, check) in [
        (
            "gateRawBool(device, 0)",
            "value == false && !value && (value ? 10 : 20) == 20",
        ),
        (
            "gateRawBool(device, 1)",
            "value == true && !(!value) && (value ? 10 : 20) == 10",
        ),
        (
            "gateRawBool(device, 2)",
            "value == true && !(!value) && (value ? 10 : 20) == 10",
        ),
        (
            "gateRawBool(device, 255)",
            "value == true && !(!value) && (value ? 10 : 20) == 10",
        ),
        (
            "gateRawPair(device, 0, 1)",
            "!value.a && value.b == true && gatePairBytes(value, 0, 1) == 1",
        ),
        (
            "gateRawPair(device, 2, 255)",
            "value.a == true && value.b == true && gatePairBytes(value, 1, 1) == 1",
        ),
    ] {
        writeln!(body, "{{ const value = await {call}; print(`${{{check} ? 1 : 0}}`); }}\n{{ const pending = {call}; const value = await pending; print(`${{{check} ? 1 : 0}}`); }}\n{{ const values = await Promise.all([{call}]); const value = values[0]; print(`${{{check} ? 1 : 0}}`); }}")?;
        count += 3;
    }
    writeln!(
        header,
        "typedef struct {{ float m[4]; bool on; }} Mat; int32_t gateMat(Mat *value);"
    )?;
    writeln!(c, "int32_t gateMat(Mat *value) {{ value->m[3] = 37; value->on = true; return value->m[0] == 0; }}")?;
    writeln!(mirror, "declare class Mat {{ m: FixedArray<f32, 4>; on: boolean; constructor(m: FixedArray<f32, 4>, on: boolean); }} declare function gateMat(value: Mat | null): i32;")?;
    body.push_str("const mat = new Mat([0, 0, 0, 0], false); print(`${gateMat(mat) == 1 && mat.m[3] == 37 && mat.on ? 1 : 0}`);\n");
    count += 1;
    for name in ["gateRawBool", "gateRawPair", "gatePairBytes", "gateMat"] {
        writeln!(symbols, "fn {name}();")?;
        writeln!(addresses, "({name:?}.into(), {name} as *const u8),")?;
    }
    body.push_str("subDeviceRelease(device);\n");
    Ok((mirror, format!("@ValueType\nclass GateBools {{ a: boolean; b: boolean; constructor(a: boolean, b: boolean) {{ this.a = a; this.b = b; }} }}\nexport async function main(): Promise<void> {{\n{body}}}\n"), count))
}
