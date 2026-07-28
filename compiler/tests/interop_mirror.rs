//! P5.2a: ingestion of the generated ambient C-header mirror into the
//! checker. Verifies the using-program is accepted and that boundary-rule
//! violations are rejected with the right S-code.

use std::fs;
use std::path::PathBuf;

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

fn interop_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop")
}

fn mirror() -> SourceFile {
    let text = fs::read_to_string(interop_dir().join("interop.generated.d.ts"))
        .expect("read generated mirror");
    SourceFile::ambient("interop.generated.d.ts", text)
}

/// Checks the mirror plus a program snippet.
fn check_with_mirror(program: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[mirror(), SourceFile::new("prog.ts", program)])
}

/// Checks the mirror plus an extra ambient mirror snippet plus a program.
fn check_with_two_mirrors(extra: &str, program: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[
        mirror(),
        SourceFile::ambient("extra.d.ts", extra),
        SourceFile::new("prog.ts", program),
    ])
}

#[test]
fn using_program_type_checks_against_the_generated_mirror() {
    let program = fs::read_to_string(interop_dir().join("use-interop.ts")).expect("read use");
    let module = check_program(&[mirror(), SourceFile::new("use-interop.ts", program)])
        .unwrap_or_else(|diags| {
            let rendered: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
            panic!("using-program rejected:\n{}", rendered.join("\n"));
        });

    // Every C function in the header became a foreign symbol with a mapped
    // signature, in declaration order: the chain-payload reader, the six
    // device entries, then the nine typed-slice facades (their `SubSlice*`
    // descriptors absorbed into `T[]`, so each takes a primitive array and
    // returns i32).
    let names: Vec<&str> = module.foreign_fns.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "subChainPayloadValue",
            "subDeviceCreate",
            "subDeviceRetain",
            "subDeviceRelease",
            "subDeviceSubmit",
            "subDeviceSetLogger",
            "subDeviceSetLabel",
            "subSliceChecksumF32",
            "subSliceChecksumI32",
            "subSliceChecksumF64",
            "subSliceChecksumI64",
            "subSliceChecksumU8",
            "subSliceChecksumI8",
            "subSliceChecksumU16",
            "subSliceChecksumI16",
            "subSliceChecksumF16",
            // P6.2 shapes: flag bit test, embedded-array struct consumer,
            // untyped bulk API + its typed facade.
            "subAccessMatches",
            "subDrawListTotal",
            "subBulkConsume",
            "subBulkConsumeF32",
            // P6.3 async model: deferred-fire register + host pump; plus a
            // production-scale embedded-array consumer.
            "subDeviceOnComplete",
            "subDevicePump",
            "subCommandBufferTotal",
            // P7.1 async/Future shapes (§14): chained-flag bit test,
            // by-value struct returns, and an out-field writer.
            "subStageMatches",
            "subFutureMake",
            "subStatsMake",
            "subDeviceQuery",
            // P7.2 composed async capstone (§14.4/§14.5): kick returns a
            // future by value + two-userdata callback-info; wait takes the
            // out-array of SubWaitEntry.
            "subDeviceKickAsync",
            "subDeviceWait",
        ]
    );

    // The typed-slice facades map their `{const T*; size_t}` descriptor to
    // `T[]` for each primitive element type, and return i32.
    let f32_slice = module.foreign_fns.iter().find(|f| f.name == "subSliceChecksumF32").unwrap();
    assert_eq!(f32_slice.params[0].ty, Type::Array(Box::new(Type::F32)));
    assert_eq!(f32_slice.ret, Type::I32);
    let i64_slice = module.foreign_fns.iter().find(|f| f.name == "subSliceChecksumI64").unwrap();
    assert_eq!(i64_slice.params[0].ty, Type::Array(Box::new(Type::I64)));
    let f16_slice = module
        .foreign_fns
        .iter()
        .find(|f| f.name == "subSliceChecksumF16")
        .unwrap();
    assert_eq!(f16_slice.params[0].ty, Type::Array(Box::new(Type::F16)));
    let packet = module
        .classes
        .iter()
        .find(|c| c.name == "SubNarrowPacket")
        .expect("production-shaped narrow packet");
    assert_eq!(
        packet.fields.iter().map(|f| f.ty.clone()).collect::<Vec<_>>(),
        vec![
            Type::U8,
            Type::I16,
            Type::F16,
            Type::U64,
            Type::I8,
            Type::U16,
            Type::F32,
        ]
    );

    // subDeviceCreate returns the branded handle (a nominal class type),
    // and its chain parameter is the `Struct | null` boundary form.
    let create = module
        .foreign_fns
        .iter()
        .find(|f| f.name == "subDeviceCreate")
        .expect("subDeviceCreate foreign declaration");
    assert!(matches!(create.ret, Type::Class(_)), "handle return type");
    assert!(
        matches!(create.params[0].ty, Type::Nullable(_)),
        "chain param is `SubChainHeader | null`"
    );

    // subDeviceSetLabel takes a mapped `string` (string-view boundary
    // form), and subDeviceSubmit a mapped `u32[]` (array-pair descriptor).
    let set_label = module.foreign_fns.iter().find(|f| f.name == "subDeviceSetLabel").unwrap();
    assert_eq!(set_label.params[1].ty, Type::Str);
    let submit = module.foreign_fns.iter().find(|f| f.name == "subDeviceSubmit").unwrap();
    assert_eq!(submit.params[1].ty, Type::Array(Box::new(Type::U32)));

    // The using program's foreign calls became `Callee::Foreign` calls.
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let mut foreign_calls = 0;
    for stmt in &main.body {
        if let hir::Stmt::Let { init, .. } = stmt {
            if let hir::ExprKind::Call { callee: hir::Callee::Foreign(_), .. } = &init.kind {
                foreign_calls += 1;
            }
        } else if let hir::Stmt::Expr(e) = stmt {
            if let hir::ExprKind::Call { callee: hir::Callee::Foreign(_), .. } = &e.kind {
                foreign_calls += 1;
            }
        }
    }
    assert!(foreign_calls >= 4, "expected several foreign calls, got {foreign_calls}");
}

fn first_code(program: &str) -> RuleCode {
    let diags = check_with_mirror(program).expect_err("expected rejection");
    diags[0].code
}

#[test]
fn nullable_handle_used_without_narrowing_is_rejected() {
    // A `SubDevice | null` passed where a bare `SubDevice` is expected:
    // the nullable handle must be narrowed first.
    let code = first_code(
        "export function main(): void {\n  let d: SubDevice | null = null;\n  subDeviceRetain(d);\n}\n",
    );
    assert_eq!(code, RuleCode::S005);
}

#[test]
fn cross_assigning_two_handle_types_is_rejected() {
    // Two distinct opaque handles are nominal and non-interchangeable.
    let extra = "// @subscript-c-header include=\"handles.h\"\n\
                 interface HandleA { readonly __sub_handle_HandleA: never; }\n\
                 interface HandleB { readonly __sub_handle_HandleB: never; }\n\
                 declare function getA(): HandleA;\n\
                 declare function takeB(b: HandleB): void;\n";
    let diags = check_with_two_mirrors(
        extra,
        "export function main(): void {\n  takeB(getA());\n}\n",
    )
    .expect_err("cross-assignment must be rejected");
    assert_eq!(diags[0].code, RuleCode::S005);
}

#[test]
fn general_union_in_program_is_still_s011() {
    let code = first_code("let x: i32 | string = 1;\n");
    assert_eq!(code, RuleCode::S011);
}

#[test]
fn value_class_with_null_in_ordinary_code_is_rejected() {
    // The `Struct | null` boundary form is legal in the mirror but not in
    // ordinary program declarations (C7 unchanged for non-boundary code).
    let code = first_code(
        "export function main(): void {\n  let c: SubChainHeader | null = null;\n  print(`${c === null}`);\n}\n",
    );
    assert_eq!(code, RuleCode::S011);
}

#[test]
fn constructing_an_opaque_handle_is_rejected() {
    let code = first_code("export function main(): void {\n  const d: SubDevice = new SubDevice();\n}\n");
    assert_eq!(code, RuleCode::S100);
}

#[test]
fn flag_set_alias_and_ambient_const_ingest() {
    // The flag-set Q13 rule (u64 type alias + `declare const` members) has
    // no instance in interop.h; this exercises the ingestion path via an
    // inline mirror. A flag typedef becomes a `u64` alias; the constants
    // become ambient globals of that alias, usable in `u64` bitwise ops.
    let extra = "type SubFlags = u64;\n\
                 declare const SUB_FLAG_A: SubFlags;\n\
                 declare const SUB_FLAG_B: SubFlags;\n";
    check_with_two_mirrors(
        extra,
        "export function main(): void {\n  const f: u64 = SUB_FLAG_A | SUB_FLAG_B;\n  print(`${f}`);\n}\n",
    )
    .expect("flag-set alias and ambient constants type-check");
}

#[test]
fn foreign_function_used_as_a_value_is_rejected() {
    let code = first_code(
        "export function main(): void {\n  const f: (chain: SubChainHeader | null) => SubDevice = subDeviceCreate;\n}\n",
    );
    // The annotation itself uses the boundary `Struct | null` form in
    // ordinary code, which is rejected first (S011); either way the
    // program is rejected. Assert on the simpler form below instead.
    let _ = code;
    let code2 = first_code("export function main(): void {\n  subDeviceRelease;\n}\n");
    assert_eq!(code2, RuleCode::S100);
}
