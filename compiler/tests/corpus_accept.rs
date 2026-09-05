//! Gate test (compiler.md §6): every accept-corpus entry checks with
//! zero diagnostics and produces a well-formed typed HIR; a19-modules
//! is one two-file program. Spot assertions verify resolved types.

#[path = "corpus/mod.rs"]
mod corpus;

use std::fs;
use std::path::PathBuf;

use subscript_compiler::{check_program, hir, SourceFile, Type};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus")
}

/// The committed ambient mirror generated from the pinned synthetic
/// header. Interop entries (a25+) are written against these global
/// ambient declarations exactly as the language prelude, so the checker
/// gate ingests it as an ambient source for any entry that uses it
/// (`specs/blocks/compiler.md` §12.4).
fn interop_mirror() -> SourceFile {
    let path = corpus_dir().join("interop/interop.generated.d.ts");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    SourceFile::ambient("interop.generated.d.ts", source)
}

fn external_device_mirror() -> SourceFile {
    let path = corpus_dir().join("interop/external-device.generated.d.ts");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    SourceFile::ambient("external-device.generated.d.ts", source)
}

fn wire_enum_mirror() -> SourceFile {
    let path = corpus_dir().join("interop/wire-enum.generated.d.ts");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    SourceFile::ambient("wire-enum.generated.d.ts", source)
}

fn wire_enum_aliases() -> SourceFile {
    let path = corpus_dir().join("interop/wire-enum-aliases.d.ts");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    SourceFile::ambient("wire-enum-aliases.d.ts", source)
}

fn check_entry(files: &[(&str, PathBuf)]) -> hir::Module {
    let mut sources: Vec<SourceFile> = files
        .iter()
        .map(|(name, path)| {
            let source = fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            SourceFile::new(*name, source)
        })
        .collect();
    // Interop entries name a foreign function, boundary struct, or flag
    // member of the synthetic header (P5 device/slice APIs plus the P6.2
    // shapes); prepend the mirror ambient surface so those names resolve. A
    // false negative is not silent — the entry then fails to check with an
    // unresolved identifier.
    let uses_external = sources
        .iter()
        .any(|source| source.source.contains("subExternalDevice"));
    let uses_wire_enum = sources.iter().any(|source| {
        source.source.contains("subWireMode")
            || source.source.contains("SubWireMode")
            || source.source.contains("subBindTone")
            || source.source.contains("SubBindTone")
    });
    let uses_interop = uses_external
        || sources
            .iter()
            .any(|source| corpus::references_interop(&source.source));
    if uses_external {
        sources.insert(0, external_device_mirror());
    }
    if uses_interop {
        sources.insert(0, interop_mirror());
    }
    if uses_wire_enum {
        sources.insert(0, wire_enum_mirror());
        sources.insert(0, wire_enum_aliases());
    }
    match check_program(&sources) {
        Ok(module) => module,
        Err(diags) => {
            let rendered: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
            panic!(
                "{} rejected with {} diagnostic(s):\n{}",
                files[0].0,
                rendered.len(),
                rendered.join("\n")
            );
        }
    }
}

fn find_fn<'m>(module: &'m hir::Module, name: &str) -> &'m hir::Function {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function `{}` missing from HIR", name))
}

fn find_class<'m>(module: &'m hir::Module, name: &str) -> &'m hir::ClassDef {
    module
        .classes
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("class `{}` missing from HIR", name))
}

#[test]
fn every_accept_entry_checks_clean_and_produces_hir() {
    let accept = corpus_dir().join("accept");
    let mut single_files: Vec<String> = fs::read_dir(&accept)
        .expect("read corpus/accept")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ts"))
        .collect();
    single_files.sort();
    for name in &single_files {
        let module = check_entry(&[(name.as_str(), accept.join(name))]);
        assert!(
            !module.functions.is_empty(),
            "{}: HIR has no functions",
            name
        );
        assert!(
            module.functions.iter().any(|f| f.exported),
            "{}: no exported entry point in HIR",
            name
        );
    }
    // a19-modules is one two-file program.
    let dir = accept.join("a19-modules");
    let module = check_entry(&[
        ("main.ts", dir.join("main.ts")),
        ("math.ts", dir.join("math.ts")),
    ]);
    assert!(find_fn(&module, "main").exported);
    assert!(find_fn(&module, "triangular").exported);
}

#[test]
fn a40_math_calls_are_intrinsics_and_constants_fold_to_literals() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a40-math.ts", accept.join("a40-math.ts"))]);
    let main = find_fn(&module, "main");

    fn walk_expr(e: &hir::Expr, maths: &mut Vec<hir::MathFn>, floats: &mut Vec<u64>) {
        match &e.kind {
            hir::ExprKind::Float(v) => floats.push(v.to_bits()),
            hir::ExprKind::Call { callee, args } => {
                if let hir::Callee::Math(f) = callee {
                    maths.push(*f);
                }
                for a in args {
                    walk_expr(a, maths, floats);
                }
            }
            hir::ExprKind::Template(parts) => {
                for p in parts {
                    if let hir::TplPart::Expr(e) = p {
                        walk_expr(e, maths, floats);
                    }
                }
            }
            hir::ExprKind::Unary { operand, .. } => walk_expr(operand, maths, floats),
            hir::ExprKind::Binary { left, right, .. } => {
                walk_expr(left, maths, floats);
                walk_expr(right, maths, floats);
            }
            _ => {}
        }
    }
    let mut maths = Vec::new();
    let mut floats = Vec::new();
    for s in &main.body {
        match s {
            hir::Stmt::Expr(e) => walk_expr(e, &mut maths, &mut floats),
            hir::Stmt::Let { init, .. } => walk_expr(init, &mut maths, &mut floats),
            _ => {}
        }
    }
    // Every §1 function is exercised at least once, as an intrinsic
    // call — never the foreign or method path.
    for f in hir::MathFn::ALL {
        if matches!(
            f,
            hir::MathFn::Random
                | hir::MathFn::Clz32
                | hir::MathFn::Imul
                | hir::MathFn::Fround
                | hir::MathFn::F32ToBits
                | hir::MathFn::F32FromBits
        ) {
            continue; // Other entries exercise these functions.
        }
        assert!(
            maths.contains(&f),
            "a40 lacks a Math.{} intrinsic call",
            f.name()
        );
    }
    // The 8 constants folded to f64 literals with the exact
    // std::f64::consts bit patterns (stdlib.md §1).
    use std::f64::consts;
    for (name, v) in [
        ("E", consts::E),
        ("LN2", consts::LN_2),
        ("LN10", consts::LN_10),
        ("LOG2E", consts::LOG2_E),
        ("LOG10E", consts::LOG10_E),
        ("PI", consts::PI),
        ("SQRT1_2", consts::FRAC_1_SQRT_2),
        ("SQRT2", consts::SQRT_2),
    ] {
        assert!(
            floats.contains(&v.to_bits()),
            "Math.{name} did not fold to its literal"
        );
    }
}

#[test]
fn a42_date_operations_are_intrinsics_and_the_type_is_nominal() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a42-date.ts", accept.join("a42-date.ts"))]);
    // A Date-typed class field and a Date[] element type survive to HIR
    // as the nominal Date type (it erases to i64 only in codegen).
    let stamp = find_class(&module, "Stamp");
    assert_eq!(stamp.fields[0].ty, Type::Date);
    let main = find_fn(&module, "main");

    fn walk_expr(e: &hir::Expr, dates: &mut Vec<hir::DateFn>, methods: &mut Vec<String>) {
        match &e.kind {
            hir::ExprKind::Call { callee, args } => {
                match callee {
                    hir::Callee::Date(f) => dates.push(*f),
                    hir::Callee::Method { recv, name } => {
                        if recv.ty == Type::Date {
                            methods.push(name.clone());
                        }
                        walk_expr(recv, dates, methods);
                    }
                    _ => {}
                }
                for a in args {
                    walk_expr(a, dates, methods);
                }
            }
            hir::ExprKind::Template(parts) => {
                for p in parts {
                    if let hir::TplPart::Expr(e) = p {
                        walk_expr(e, dates, methods);
                    }
                }
            }
            hir::ExprKind::New { args, .. } => {
                for a in args {
                    walk_expr(a, dates, methods);
                }
            }
            hir::ExprKind::Index { obj, index, .. } => {
                walk_expr(obj, dates, methods);
                walk_expr(index, dates, methods);
            }
            hir::ExprKind::Field { obj, .. } => walk_expr(obj, dates, methods),
            _ => {}
        }
    }
    let mut dates = Vec::new();
    let mut methods = Vec::new();
    for s in &main.body {
        match s {
            hir::Stmt::Expr(e) => walk_expr(e, &mut dates, &mut methods),
            hir::Stmt::Let { init, .. } => walk_expr(init, &mut dates, &mut methods),
            _ => {}
        }
    }
    // Every accepted Date operation appears as an intrinsic call —
    // never the class-method path — and getTime never survives as a
    // call (it folds to the receiver at check time).
    for f in [
        hir::DateFn::New,
        hir::DateFn::Utc,
        hir::DateFn::GetUtcFullYear,
        hir::DateFn::GetUtcMonth,
        hir::DateFn::GetUtcDate,
        hir::DateFn::GetUtcDay,
        hir::DateFn::GetUtcHours,
        hir::DateFn::GetUtcMinutes,
        hir::DateFn::GetUtcSeconds,
        hir::DateFn::GetUtcMilliseconds,
        hir::DateFn::ToIso,
    ] {
        assert!(
            dates.contains(&f),
            "a42 lacks a Date intrinsic for {}",
            f.name()
        );
    }
    assert!(
        methods.is_empty(),
        "Date operations must not take the method path: {methods:?}"
    );
}

#[test]
fn a02_conversions_resolve_to_distinct_sized_types() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a02-integer-types.ts", accept.join("a02-integer-types.ts"))]);
    let main = find_fn(&module, "main");
    // const narrowed: f32 = realSum as f32;  (f64 -> f32)
    let hir::Stmt::Let { name, ty, init, .. } = &main.body[6] else {
        panic!("expected `narrowed` let");
    };
    assert_eq!(name, "narrowed");
    assert_eq!(*ty, Type::F32);
    let hir::ExprKind::Cast(inner) = &init.kind else {
        panic!("expected a cast initializer");
    };
    assert_eq!(init.ty, Type::F32);
    assert_eq!(inner.ty, Type::F64);
    // const converted: i32 = narrowed as i32;  (f32 -> i32)
    let hir::Stmt::Let { name, ty, init, .. } = &main.body[7] else {
        panic!("expected `converted` let");
    };
    assert_eq!(name, "converted");
    assert_eq!(*ty, Type::I32);
    let hir::ExprKind::Cast(inner) = &init.kind else {
        panic!("expected a cast initializer");
    };
    assert_eq!(inner.ty, Type::F32);
    // Positions are carried: the `converted` cast sits on line 14.
    assert_eq!(init.pos.line, 14);
}

#[test]
fn a04_value_class_type_is_nominal_and_by_value() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a04-value-struct.ts", accept.join("a04-value-struct.ts"))]);
    let vec3 = find_class(&module, "Vec3");
    assert!(vec3.is_value);
    assert_eq!(vec3.fields.len(), 3);
    assert!(vec3.fields.iter().all(|f| f.ty == Type::F32));
    // const copy: Vec3 = original;  — both sides the same nominal type.
    let main = find_fn(&module, "main");
    let hir::Stmt::Let { name, ty, init, .. } = &main.body[1] else {
        panic!("expected `copy` let");
    };
    assert_eq!(name, "copy");
    assert!(matches!(ty, Type::Class(_)));
    assert_eq!(init.ty, *ty);
}

#[test]
fn a12_generics_are_monomorphized_in_hir() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a12-generics-mono.ts", accept.join("a12-generics-mono.ts"))]);
    assert_eq!(find_fn(&module, "identity<i32>").ret, Type::I32);
    assert_eq!(find_fn(&module, "identity<f64>").ret, Type::F64);
    assert_eq!(find_class(&module, "Box<i32>").fields[0].ty, Type::I32);
    assert_eq!(find_class(&module, "Box<f64>").fields[0].ty, Type::F64);
    // Templates never survive monomorphization.
    assert!(module.functions.iter().all(|f| f.name != "identity"));
    assert!(module.classes.iter().all(|c| c.name != "Box"));
}

#[test]
fn generic_value_classes_carry_alignment_overrides() {
    let source = "@CStruct({ align: 16 })\nclass Box<T> { value: T; constructor(value: T) { this.value = value; } }\nexport function main(): void { const integer: Box<i32> = new Box<i32>(1); const float: Box<f64> = new Box<f64>(2.0); print(`${integer.value}:${float.value}`); }\n";
    let module = subscript_compiler::check_program(&[SourceFile::new("generic-align.ts", source)])
        .expect("generic aligned classes check clean");
    for name in ["Box<i32>", "Box<f64>"] {
        let override_ = find_class(&module, name)
            .alignment_override
            .as_ref()
            .expect("alignment override");
        assert_eq!(override_.value, 16, "{name}");
    }
}

#[test]
fn a17_narrowing_rewrites_nullable_to_the_narrowed_type() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[("a17-null-story.ts", accept.join("a17-null-story.ts"))]);
    let next_value = find_fn(&module, "nextValue");
    // Last statement: `return node.next.value;` — after the null checks
    // `node` and `node.next` carry the narrowed class type, not
    // `ListNode | null`.
    let hir::Stmt::Return {
        value: Some(value), ..
    } = next_value.body.last().expect("body")
    else {
        panic!("expected trailing return");
    };
    assert_eq!(value.ty, Type::I32);
    let hir::ExprKind::Field { obj: next, name } = &value.kind else {
        panic!("expected field access on node.next");
    };
    assert_eq!(name, "value");
    assert!(
        matches!(next.ty, Type::Class(_)),
        "node.next should be narrowed to ListNode, got {:?}",
        next.ty
    );
    let hir::ExprKind::Field { obj: node, .. } = &next.kind else {
        panic!("expected node.next to be a field access");
    };
    assert!(
        matches!(node.ty, Type::Class(_)),
        "node should be narrowed to ListNode, got {:?}",
        node.ty
    );
}

#[test]
fn a20_generator_types_flow_through_next() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[(
        "a20-coroutine-generator.ts",
        accept.join("a20-coroutine-generator.ts"),
    )]);
    let sequence = find_fn(&module, "sequence");
    assert!(sequence.is_generator);
    assert_eq!(sequence.ret, Type::Generator(Box::new(Type::I32)));
    // main: `const generator = sequence(4);` infers Generator<i32>.
    let main = find_fn(&module, "main");
    let hir::Stmt::Let { name, ty, .. } = &main.body[0] else {
        panic!("expected `generator` let");
    };
    assert_eq!(name, "generator");
    assert_eq!(*ty, Type::Generator(Box::new(Type::I32)));
}

#[test]
fn r26_full_width_literals_store_twos_complement_bits() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[(
        "a132-int-literal-64bit.ts",
        accept.join("a132-int-literal-64bit.ts"),
    )]);
    let decimal_max = module
        .globals
        .iter()
        .find(|global| global.name == "decimalMax")
        .expect("decimalMax global");
    assert_eq!(decimal_max.ty, Type::U64);
    assert!(matches!(&decimal_max.init.kind, hir::ExprKind::Int(-1)));

    let signed_min = module
        .globals
        .iter()
        .find(|global| global.name == "signedMin")
        .expect("signedMin global");
    assert_eq!(signed_min.ty, Type::I64);
    assert!(matches!(
        &signed_min.init.kind,
        hir::ExprKind::Int(value) if *value == i64::MIN
    ));
}

#[test]
fn r26_ambient_flag_above_f64_exact_range_reaches_program_exactly() {
    let module = check_program(&[
        SourceFile::ambient(
            "flags.generated.d.ts",
            "type SubFlags = u64;\ndeclare const SUB_BIG_ONE = 9007199254740993;\n",
        ),
        SourceFile::new(
            "program.ts",
            "export function main(): void {\n  const exact: u64 = SUB_BIG_ONE;\n  print(`${exact}`);\n}\n",
        ),
    ])
    .expect("full-width ambient flag checks");
    let main = find_fn(&module, "main");
    let hir::Stmt::Let { init, .. } = &main.body[0] else {
        panic!("expected exact flag local");
    };
    assert!(matches!(
        &init.kind,
        hir::ExprKind::Int(value) if *value == 9_007_199_254_740_993
    ));
    assert_eq!(init.ty, Type::U64);
}

#[test]
fn r29_index_sugar_and_spelled_calls_have_identical_hir_bodies() {
    let class = "class Values {\n  [index: u32]: i32;\n  get(index: u32): i32 { return index as i32; }\n  set(index: u32, value: i32): void {}\n}\n";
    let sugar = format!(
        "{class}export function main(): void {{\n  const values: Values = new Values();\n  const index: u32 = 0;\n  const read: i32 = values[    index];\n  values[    index] = read;\n}}\n"
    );
    let spelled = format!(
        "{class}export function main(): void {{\n  const values: Values = new Values();\n  const index: u32 = 0;\n  const read: i32 = values.get(index);\n  values.set(index,   read);\n}}\n"
    );
    let sugar_module =
        check_program(&[SourceFile::new("identity.ts", sugar)]).expect("class index sugar checks");
    let spelled_module = check_program(&[SourceFile::new("identity.ts", spelled)])
        .expect("spelled index accessors check");
    assert_eq!(
        find_fn(&sugar_module, "main").body,
        find_fn(&spelled_module, "main").body
    );
}

#[test]
fn r31_multi_binding_using_appends_reverse_dispose_calls_in_hir() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[(
        "a138-using-dispose.ts",
        accept.join("a138-using-dispose.ts"),
    )]);
    let main = find_fn(&module, "main");
    let hir::Stmt::Block(body) = &main.body[0] else {
        panic!("expected the using block");
    };
    assert!(matches!(&body[0], hir::Stmt::Let { dispose: true, .. }));
    assert!(matches!(&body[1], hir::Stmt::Let { dispose: true, .. }));
    let disposed = body
        .iter()
        .filter_map(|statement| {
            let hir::Stmt::Expr(hir::Expr {
                kind:
                    hir::ExprKind::Call {
                        callee: hir::Callee::Method { recv, name },
                        ..
                    },
                ..
            }) = statement
            else {
                return None;
            };
            if name != hir::DISPOSE_METHOD_NAME {
                return None;
            }
            let hir::ExprKind::Local(local) = &recv.kind else {
                return None;
            };
            Some(local.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(disposed, ["b", "a"]);
}
