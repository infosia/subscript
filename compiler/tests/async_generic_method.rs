//! Async method type parameters (`compiler.md` §93).

use subscript_compiler::{
    check_program, divergence::Divergence, hir, render_diagnostics, RuleCode, SourceFile,
};

const LOADER: &str = "class Box {\n  async load<T>(value: T): Promise<T> {\n    await Context.suspend();\n    return value;\n  }\n}\n";

fn accept(source: &str) -> hir::Module {
    check_program(&[SourceFile::new("test.ts", source)]).expect("positive control must check")
}

fn reject(source: &str, code: RuleCode, message: &str, line: u32, column: u32) {
    let files = [SourceFile::new("test.ts", source)];
    let diagnostics = check_program(&files).expect_err("program must fail");
    println!("{}", render_diagnostics(&files, &diagnostics));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.message, message);
    assert_eq!((diagnostic.pos.line, diagnostic.pos.col), (line, column));
}

fn main_with(statement: &str) -> String {
    format!("{LOADER}export async function main(): Promise<void> {{\n  const box: Box = new Box();\n  {statement}\n}}\n")
}

#[test]
fn floating_call_reports_at_the_statement() {
    accept(&main_with("await box.load<i32>(1);"));
    let source = main_with("box.load<i32>(1);");
    let files = [SourceFile::new("test.ts", &source)];
    let diagnostics = check_program(&files).expect_err("floating handle must fail");
    println!("{}", render_diagnostics(&files, &diagnostics));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S013);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (9, 3));
}

#[test]
fn template_read_reports_the_async_value_rule() {
    accept(&main_with("await box.load<i32>(1);"));
    reject(
        &main_with("box.load;"),
        RuleCode::S100,
        "async method `load` is not a first-class value; call it directly in await position",
        9,
        7,
    );
}

#[test]
fn missing_type_arguments_report_at_the_call() {
    accept(&main_with("await box.load<i32>(1);"));
    reject(
        &main_with("await box.load(1);"),
        RuleCode::S100,
        "generic method `load` requires explicit type arguments",
        9,
        13,
    );
    let source = main_with("await box.load(1);");
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::GenericMethodTypeArguments)
    );
}

#[test]
fn distinct_type_lists_produce_distinct_async_hir_methods() {
    let module = accept(&main_with(
        "await box.load<i32>(1);\n  await box.load<string>(\"x\");\n  await box.load<i32>(2);",
    ));
    let methods = &module.classes[0].methods;
    assert_eq!(
        methods
            .iter()
            .map(|method| method.name.as_str())
            .collect::<Vec<_>>(),
        ["load<i32>", "load<string>"]
    );
    assert!(methods.iter().all(|method| method.is_async));
}

#[test]
fn generic_class_keeps_its_generic_method_rejection() {
    accept(&main_with("await box.load<i32>(1);"));
    let source = main_with("await box.load<i32>(1);")
        .replace("class Box", "class Box<U>")
        .replace("box: Box = new Box()", "box: Box<i32> = new Box<i32>()");
    reject(
        &source,
        RuleCode::S100,
        "generic classes cannot declare generic methods",
        2,
        3,
    );
}

#[test]
fn static_async_generic_method_keeps_its_shape_rejection() {
    accept(&main_with("await box.load<i32>(1);"));
    accept(
        "class Box { static load<T>(value: T): T { return value; } }
export function main(): void { Box.load<i32>(1); }",
    );
    for call in [
        "Box.load<i32>(1);",
        "await Box.load<i32>(1);",
        "Box.load(1);",
        "await Box.load(1);",
    ] {
        let source = main_with(call).replace("async load", "static async load");
        reject(
            &source,
            RuleCode::S100,
            "async static methods are not in the decided surface",
            2,
            3,
        );
    }
}

#[test]
fn bodiless_declared_async_template_reports_the_body_rule() {
    accept(&main_with("await box.load<i32>(1);"));
    for (modifier, ret, column, divergence, call) in [
        ("async ", "Promise<T>", 9, None, "await box.load<i32>(1);"),
        (
            "",
            "T",
            3,
            Some(Divergence::BodilessDeclareGenericMethod),
            "box.load<i32>(1);",
        ),
    ] {
        let source = format!(
            "declare class Box {{
  {modifier}load<T>(value: T): {ret};
}}
export async function main(): Promise<void> {{
  const box: Box = new Box();
  {call}
}}
"
        );
        reject(
            &source,
            RuleCode::S100,
            "function bodies are required",
            2,
            column,
        );
        let files = [SourceFile::new("test.ts", source)];
        let diagnostics = check_program(&files).unwrap_err();
        assert_eq!(diagnostics[0].divergence, divergence);
        assert_eq!(
            render_diagnostics(&files, &diagnostics).contains("= TypeScript accepts:"),
            divergence.is_some()
        );
    }
}

#[test]
fn async_generic_generator_keeps_its_shape_rejection() {
    accept(&main_with("await box.load<i32>(1);"));
    for call in [
        "box.load<i32>(1);",
        "await box.load<i32>(1);",
        "box.load(1);",
    ] {
        let source = main_with(call)
            .replace("async load", "async *load")
            .replace("Promise<T>", "AsyncGenerator<T>");
        reject(
            &source,
            RuleCode::S100,
            "async generator methods are not in the decided surface",
            2,
            3,
        );
    }
}

#[test]
fn value_class_async_generic_method_reports_only_its_declaration() {
    accept(&main_with("await box.load<i32>(1);"));
    for call in [
        "box.load<i32>(1);",
        "await box.load<i32>(1);",
        "box.load(1);",
    ] {
        let source = format!("@CStruct\n{}", main_with(call));
        reject(
            &source,
            RuleCode::S100,
            "async methods on `@CStruct` value classes are not in the decided surface",
            3,
            3,
        );
    }
}

#[test]
fn descriptor_generic_method_reports_only_its_declaration() {
    accept(&main_with("await box.load<i32>(1);"));
    let source = format!(
        "@Descriptor\n{}",
        main_with("await box.load<i32>(1);").replace("new Box()", "{}")
    );
    reject(
        &source,
        RuleCode::S100,
        "descriptor classes cannot declare methods",
        3,
        9,
    );
}

#[test]
fn sync_generic_generator_reports_only_its_declaration() {
    accept(&main_with("await box.load<i32>(1);"));
    let source = main_with("box.load<i32>(1);")
        .replace("async load", "*load")
        .replace("await Context.suspend();", "yield value;")
        .replace("Promise<T>", "Generator<T>");
    reject(
        &source,
        RuleCode::S100,
        "generator methods are not in the decided surface",
        2,
        3,
    );
}

#[test]
fn rejected_literal_method_names_report_only_the_declaration() {
    accept(&main_with("await box.load<i32>(1);"));
    for key in ["\"load\"", "[\"load\"]"] {
        let source =
            main_with("await box.load<i32>(1);").replace("async load", &format!("async {key}"));
        reject(
            &source,
            RuleCode::S100,
            "computed method names are not decided",
            2,
            3,
        );
    }
}

#[test]
fn duplicate_generic_method_reports_only_the_declaration() {
    let control = "class Box {\n  load<T>(value: T): T { return value; }\n}\nexport function main(): void { const box: Box = new Box(); box.load<i32>(1); }\n";
    accept(control);
    let source = control.replacen(
        "}\n}\n",
        "}\n  load<T>(value: T): T { return value; }\n}\n",
        1,
    );
    reject(
        &source,
        RuleCode::S017,
        "a method cannot share the member name `load` with a method",
        3,
        3,
    );
}

#[test]
fn generic_class_static_generic_method_reports_only_the_first_rule() {
    accept("class Box { static load<T>(value: T): T { return value; } }\nexport function main(): void { Box.load<i32>(1); }");
    for call in ["Box.load<i32>(1);", "await Box.load<i32>(1);"] {
        let source = main_with(call)
            .replace("class Box", "class Box<U>")
            .replace("async load", "static async load")
            .replace("box: Box = new Box()", "box: Box<i32> = new Box<i32>()");
        reject(
            &source,
            RuleCode::S100,
            "generic classes cannot declare static members",
            2,
            3,
        );
    }
}

#[test]
fn r187_has_one_diagnostic_and_no_call_cascade() {
    accept(include_str!(
        "../../corpus/accept/a187-async-generic-method.ts"
    ));
    let files = [SourceFile::new(
        "r187-async-generic-method-on-value-class.ts",
        include_str!("../../corpus/reject/r187-async-generic-method-on-value-class.ts"),
    )];
    let diagnostics = check_program(&files).expect_err("r187 must fail");
    println!("{}", render_diagnostics(&files, &diagnostics));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "async methods on `@CStruct` value classes are not in the decided surface"
    );
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (12, 3));
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::AsyncFunctionShape)
    );
}

#[test]
fn mirror_static_generic_method_reports_only_its_declaration() {
    accept("class Box { static load<T>(value: T): T { return value; } }\nexport function main(): void { Box.load<i32>(1); }");
    let files = [
        SourceFile::ambient(
            "mirror.d.ts",
            "declare class Box {\n  static load<T>(value: T): T;\n}\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void { Box.load<i32>(1); }\n",
        ),
    ];
    let diagnostics = check_program(&files).expect_err("mirror static method must fail");
    println!("{}", render_diagnostics(&files, &diagnostics));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "mirror classes cannot declare static methods or accessors"
    );
    assert_eq!(
        (
            diagnostics[0].pos.file.as_str(),
            diagnostics[0].pos.line,
            diagnostics[0].pos.col
        ),
        ("mirror.d.ts", 2, 10)
    );
}
