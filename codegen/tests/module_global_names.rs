//! Every module declaration kind keeps its identity on all engines (compiler.md §125).

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn program(base: i32) -> Vec<SourceFile> {
    vec![
        SourceFile::new(
            "main.ts",
            r#"
            import { libSnapshot } from "./lib";
            import { identity } from "./shared";
            enum E { Value = 12 }
            let x: i32 = 7;
            function read(): i32 { return x + 1; }
            class C { value: i32 = 9; static value: i32 = 13; static read(): i32 { return C.value + 1; } }
            function pick<T>(first: T, second: T): T { return first; }
            class Box<T> { value: T; constructor(first: T, second: T) { this.value = first; } }
            export function snapshot(): string {
                return `${x},${read()},${new C().value},${pick<i32>(10, 1000)},${new Box<i32>(11, 2000).value},${C.value},${C.read()},${identity<E>(E.Value)}`;
            }
            export function main(): void { print(`main=${snapshot()} lib=${libSnapshot()}`); }
        "#,
        ),
        SourceFile::new(
            "lib.ts",
            format!(
                r#"
            import {{ identity }} from "./shared";
            enum E {{ Value = 105 }}
            let x: i32 = {base};
            function read(): i32 {{ return x + 1; }}
            class C {{ value: i32 = {class_value}; static value: i32 = 106; static read(): i32 {{ return C.value + 1; }} }}
            function pick<T>(first: T, second: T): T {{ return second; }}
            class Box<T> {{ value: T; constructor(first: T, second: T) {{ this.value = second; }} }}
            export function snapshot(): string {{
                return `${{x}},${{read()}},${{new C().value}},${{pick<i32>(999, {function_value})}},${{new Box<i32>(999, {generic_value}).value}},${{C.value}},${{C.read()}},${{identity<E>(E.Value)}}`;
            }}
            export function libSnapshot(): string {{ return snapshot(); }}
        "#,
                class_value = base + 2,
                function_value = base + 3,
                generic_value = base + 4,
            ),
        ),
        SourceFile::new(
            "shared.ts",
            "export function identity<T>(value: T): T { return value; }",
        ),
    ]
}

#[test]
fn every_declaration_kind_agrees_on_all_engines() {
    let start = std::time::Instant::now();
    let base = 100;
    let expected = "main=7,8,9,10,11,13,14,12 lib=100,101,102,103,104,106,107,105\n";
    let files = program(base);
    let hir = check_program(&files).expect("independent declarations");
    let module = lower_module(&hir).expect("LIR");
    assert_eq!(
        interpret(&module).expect("interpreter"),
        expected.as_bytes()
    );
    assert_eq!(run_jit(&files).expect("dev JIT"), expected.as_bytes());
    assert_eq!(run_c_aot(&files).expect("ship C"), expected.as_bytes());
    // Workers require the runtime adapter supplied by the two production engines.
    let workers = worker_program(base);
    let expected = format!("main=7\nlib={base}\n");
    assert_eq!(
        run_jit(&workers).expect("worker dev JIT"),
        expected.as_bytes()
    );
    assert_eq!(
        run_c_aot(&workers).expect("worker ship C"),
        expected.as_bytes()
    );
    eprintln!("total declaration test: {:?}", start.elapsed());
}

#[test]
fn reload_keeps_each_modules_storage_and_replaces_its_bodies() {
    let mut session = subscript_codegen::ReloadSession::new(&program(100)).expect("reload session");
    session.call_main().expect("initial main");
    assert_eq!(
        session.take_output(),
        b"main=7,8,9,10,11,13,14,12 lib=100,101,102,103,104,106,107,105\n"
    );
    session
        .reload(&program(200))
        .expect("body and initializer edits");
    session.call_main().expect("reloaded main");
    assert_eq!(
        session.take_output(),
        b"main=7,8,9,10,11,13,14,12 lib=100,101,202,203,204,106,107,105\n"
    );
}

#[test]
fn a_non_host_main_does_not_replace_the_host_entry() {
    let files = [
        SourceFile::new("lib.ts", "export function main(): i32 { return 100; }"),
        SourceFile::new(
            "main.ts",
            "export function main(): void { print(\"host\"); }",
        ),
    ];
    let module = lower_module(&check_program(&files).expect("one host entry")).expect("LIR");
    assert_eq!(interpret(&module).expect("interpreter"), b"host\n");
    assert_eq!(run_jit(&files).expect("dev JIT"), b"host\n");
    assert_eq!(run_c_aot(&files).expect("ship C"), b"host\n");
}

#[test]
fn reload_hash_detects_nominal_type_changes_between_same_name_classes() {
    for (declaration, label) in [
        ("export function read(value: C): void {}", "function read"),
        (
            "export function read(): C { return new C(); }",
            "function read",
        ),
        ("let value: C = new C();", "variable value"),
        ("class Holder { value: C = new C(); }", "class Holder"),
    ] {
        let hash = |import: &str, body: &str| {
            let files = [
                SourceFile::new("main.ts", format!("import {{ C }} from \"./{import}\"; {declaration} export function main(): void {{ {body} }}")),
                SourceFile::new("first.ts", "export class C { value: i32 = 7; }"),
                SourceFile::new("second.ts", "export class C { value: i32 = 100; }"),
            ];
            subscript_codegen::declaration_hash(
                &check_program(&files).expect("nominal type program"),
            )
        };
        let before = hash("first", "");
        let after = hash("second", "");
        assert_eq!(before.first_difference(&after).as_deref(), Some(label));
        assert_eq!(before, hash("first", "print(\"body\");"));
    }
}

fn worker_program(base: i32) -> Vec<SourceFile> {
    let module = |label: &str, value: i32, export: &str| {
        format!(
            r#"
        class Message {{ value: i32; constructor(value: i32) {{ this.value = value; }} }}
        function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {{
            const message: Message | null = inbox.wait();
            if (message !== null) {{ outbox.post(new Message(message.value + {value})); }}
        }}
        export function {export}(): void {{
            const worker: Worker<Message, Message> = Worker.spawn(echo);
            worker.post(new Message(0));
            worker.close();
            worker.join();
            const reply: Message | null = worker.poll();
            if (reply !== null) {{ print(`{label}=${{reply.value}}`); }}
        }}
    "#
        )
    };
    vec![
        SourceFile::new("main.ts", format!("import {{ libWorker }} from \"./lib\"; {} export function main(): void {{ mainWorker(); libWorker(); }}", module("main", 7, "mainWorker"))),
        SourceFile::new("lib.ts", module("lib", base, "libWorker")),
    ]
}

#[test]
fn allocation_metadata_names_modules_only_for_shared_class_names() {
    for (other, expected) in [("C", "C (main.ts)"), ("D", "C")] {
        let files = [
            SourceFile::new("main.ts", "class C {} export function main(): void {}"),
            SourceFile::new("lib.ts", format!("class {other} {{}}")),
        ];
        let hir = check_program(&files).expect("classes");
        let emitted = subscript_codegen::emit_c(&hir).expect("C metadata");
        assert!(emitted
            .allocation_metadata_source
            .contains(&format!("\"{expected}\"")));
        if other == "C" {
            assert!(emitted.allocation_metadata_source.contains("C (lib.ts)"));
        } else {
            assert!(!emitted.allocation_metadata_source.contains("C (main.ts)"));
        }
    }
}

#[test]
fn allocation_metadata_distinguishes_builtin_and_module_error() {
    let hir = check_program(&[SourceFile::new(
        "main.ts",
        "class Error {} export function main(): void {}",
    )])
    .expect("module Error");
    let emitted = subscript_codegen::emit_c(&hir).expect("C metadata");
    for label in ["Error (main.ts)", "Error (prelude/lang.d.ts)"] {
        assert!(emitted.allocation_metadata_source.contains(label));
    }
}

#[test]
fn typed_mirror_global_lowering_errors_use_source_names() {
    let body = "print(`${K}`);";
    let files = [
        SourceFile::ambient("mirror.d.ts", "declare const K: i32;"),
        SourceFile::new(
            "main.ts",
            format!("export function main(): void {{ {body} }}"),
        ),
    ];
    let hir = check_program(&files).expect("typed mirror global checks");
    let error = lower_module(&hir).expect_err("typed mirror global has no storage");
    assert_eq!(error.message, "unknown global `K`");
    eprintln!("typed mirror global: {error}");
}
