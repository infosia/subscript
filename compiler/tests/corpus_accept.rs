//! Gate test (compiler.md §6): every accept-corpus entry checks with
//! zero diagnostics and produces a well-formed typed HIR; a19-modules
//! is one two-file program. Spot assertions verify resolved types.

use std::fs;
use std::path::PathBuf;

use subscript_compiler::{check_program, hir, SourceFile, Type};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus")
}

fn check_entry(files: &[(&str, PathBuf)]) -> hir::Module {
    let sources: Vec<SourceFile> = files
        .iter()
        .map(|(name, path)| {
            let source = fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            SourceFile::new(*name, source)
        })
        .collect();
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
    assert_eq!(
        single_files.len(),
        23,
        "expected 23 single-file accept entries plus a19-modules"
    );
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
fn a02_conversions_resolve_to_distinct_sized_types() {
    let accept = corpus_dir().join("accept");
    let module = check_entry(&[(
        "a02-integer-types.ts",
        accept.join("a02-integer-types.ts"),
    )]);
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
    let module = check_entry(&[(
        "a12-generics-mono.ts",
        accept.join("a12-generics-mono.ts"),
    )]);
    assert_eq!(find_fn(&module, "identity<i32>").ret, Type::I32);
    assert_eq!(find_fn(&module, "identity<f64>").ret, Type::F64);
    assert_eq!(find_class(&module, "Box<i32>").fields[0].ty, Type::I32);
    assert_eq!(find_class(&module, "Box<f64>").fields[0].ty, Type::F64);
    // Templates never survive monomorphization.
    assert!(module.functions.iter().all(|f| f.name != "identity"));
    assert!(module.classes.iter().all(|c| c.name != "Box"));
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
