//! §129: host names select implementations, and reload preserves that boundary.
use subscript_codegen::{emit_c_files, emit_c_without_main, ReloadError, ReloadSession};
use subscript_compiler::{check_program, SourceFile};

fn files(target: &str) -> Vec<SourceFile> {
    let mut api = SourceFile::new(
        "api.ts",
        format!("export {{ {target} as first, {target} as second }} from './lib';"),
    );
    api.entry = true;
    vec![api, SourceFile::new("lib.ts", "let count: i32 = 0; export function update(): void { count += 1; print(`${count}`); } export function other(): void { count += 10; print(`${count}`); } export function parameterized(value: i32): void { count += value; }")]
}

#[test]
fn uncalled_aliases_share_state_header_and_dev_names_without_main() {
    let input = files("update");
    let module = check_program(&input).unwrap();
    let program = emit_c_without_main(&module).unwrap();
    let mut session = ReloadSession::new(&input).unwrap();
    assert_eq!(session.host_entry_names(), ["first", "second"]);
    for name in session.host_entry_names() {
        assert!(program
            .host_header
            .contains(&format!("#define SUBSCRIPT_HOST_ENTRY_{name} 1")));
        assert!(program
            .host_header
            .contains(&format!("void subscript_export_{name}(")));
        assert!(program
            .source
            .contains(&format!("void subscript_export_{name}(")));
    }
    assert_eq!(
        program
            .host_header
            .matches("compiler.md §142 rules 1 and 2:")
            .count(),
        1
    );
    assert!(program.host_header.contains(
        r#"/**
 * C calling convention shared by the module initializer (`subscript_init`) and every
 * supported host export.
 *
 * A host that may clear traps brackets each call with
 * `subscript_rt_ctx_enter_script` and `subscript_rt_ctx_exit_script`.
 *
 * An ordinary run entry uses `subscript_export_main`; a host-owned entry may instead
 * drive other zero-argument `void` exports using the symbol
 * `subscript_export_<name>` and this same C signature.
 *
 * compiler.md §142 rules 1 and 2:
 * A handle value from the host transfers no ownership, by any route:
 * an entry parameter, a foreign-call result, or a field or element of
 * a value that the host fills.
 * The script can copy the handle, keep it, and use it in a later call.
 * The host keeps the object valid while any script code of that Context
 * can use the handle.
 * A script object that holds the handle keeps nothing alive on the host side.
 * The language and runtime do not detect use after the host destroys the object.
 */
typedef void (*subscript_main_entry)(subscript_rt_context* ctx);
"#
    ));
    assert!(!program.host_header.contains("subscript_export_update"));
    assert!(!program.host_header.contains("SUBSCRIPT_HOST_ENTRY_update"));
    assert!(!program.source.contains("subscript_export_other"));
    session.call_export("first").unwrap();
    session.call_export("second").unwrap();
    assert_eq!(session.take_output(), b"1\n2\n");
    assert!(session.call_export("update").is_err());
    assert!(session.call_main().is_err());
    assert!(session.take_output().is_empty());
    let dir = std::env::temp_dir().join(format!("s129-header-{}", std::process::id()));
    let emitted = emit_c_files(&input, &dir, "program", false).unwrap();
    assert_eq!(
        std::fs::read_to_string(&emitted.host_header).unwrap(),
        program.host_header
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reload_resolves_alias_target_and_rejects_public_signature_changes() {
    let mut session = ReloadSession::new(&files("update")).unwrap();
    session.call_export("first").unwrap();
    session.reload(&files("other")).unwrap();
    session.call_export("second").unwrap();
    assert_eq!(session.take_output(), b"1\n11\n");
    let changed = files("parameterized");
    assert_eq!(changed[1].source, files("other")[1].source);
    // Only the host rows change; all target declarations are identical.
    assert_eq!(
        check_program(&changed).unwrap().host_entries[0]
            .signature
            .parameters,
        [subscript_compiler::types::Type::I32]
    );
    assert!(matches!(
        session.reload(&changed),
        Err(ReloadError::DeclarationChanged { .. })
    ));
    session.call_export("first").unwrap();
    assert_eq!(session.take_output(), b"21\n");
}

#[test]
fn zero_argument_async_alias_keeps_host_call_behavior() {
    let input = [SourceFile::new("api.ts", "async function work(): Promise<void> { print('start'); await Context.suspend(); print('end'); } export { work as launch };")];
    let mut session = ReloadSession::new(&input).unwrap();
    assert_eq!(session.host_entry_names(), ["launch"]);
    assert!(session.call_export("work").is_err());
    session.call_export("launch").unwrap();
    assert_eq!(session.take_output(), b"start\n");
    session.async_step().unwrap();
    assert_eq!(session.take_output(), b"end\n");
}

#[test]
fn main_with_parameters_is_a_host_entry_but_not_a_runner_entry() {
    use subscript_codegen::{emit_c, EntryArg};
    let files = [SourceFile::entry(
        "api.ts",
        "export function main(value: i32): void { print(`${value}`); }",
    )];
    let checked = check_program(&files).unwrap();
    assert!(emit_c(&checked).unwrap_err().contains("main(): void"));
    let emitted = emit_c_without_main(&checked).unwrap();
    assert!(emitted
        .host_header
        .contains("void subscript_export_main(subscript_rt_context* ctx, int32_t a0);"));
    let mut session = ReloadSession::new(&files).unwrap();
    assert!(session.call_main().is_err());
    session
        .call_export_with("main", &[EntryArg::I32(7)])
        .unwrap();
    assert_eq!(session.take_output(), b"7\n");
}

#[test]
fn hot_reload_example_runs_and_preserves_state_with_a_refusal_control() {
    use subscript_codegen::{run_c_aot, run_jit};
    let source = include_str!("../../examples/hot-reload/demo.ts");
    let original = [SourceFile::entry("demo.ts", source)];
    let expected = b"hot reload: run 1, editable result 10\n";
    assert_eq!(run_jit(&original).unwrap(), expected);
    assert_eq!(run_c_aot(&original).unwrap(), expected);
    let mut session = ReloadSession::new(&original).unwrap();
    session.call_main().unwrap();
    assert_eq!(session.take_output(), expected);
    let changed = [SourceFile::entry(
        "demo.ts",
        source.replace("return 10;", "return 20;"),
    )];
    session.reload(&changed).unwrap();
    session.call_main().unwrap();
    assert_eq!(
        session.take_output(),
        b"hot reload: run 2, editable result 20\n"
    );
    let incompatible = [SourceFile::entry(
        "demo.ts",
        source.replace("label", "text"),
    )];
    assert!(matches!(
        session.reload(&incompatible),
        Err(ReloadError::DeclarationChanged { .. })
    ));
}

#[test]
fn async_alias_reload_keeps_retained_function_slots_stable() {
    fn input(target: &str) -> Vec<SourceFile> {
        vec![SourceFile::entry("api.ts", format!("export {{ {target} as launch, invoke }} from './lib';")), SourceFile::new("lib.ts", "export async function a(): Promise<void> { print('a'); } function retained(): void { print('retained'); } let saved: () => void = retained; export async function b(): Promise<void> { print('b'); } export function invoke(): void { saved(); }")]
    }
    let mut session = ReloadSession::new(&input("a")).unwrap();
    session.call_export("launch").unwrap();
    session.call_export("invoke").unwrap();
    assert_eq!(session.take_output(), b"a\nretained\n");
    session.reload(&input("b")).unwrap();
    session.call_export("launch").unwrap();
    session.call_export("invoke").unwrap();
    assert_eq!(session.take_output(), b"b\nretained\n");
    let mut changed = input("b");
    changed[0].source = changed[0].source.replace("launch", "renamed");
    assert!(matches!(
        session.reload(&changed),
        Err(ReloadError::DeclarationChanged { .. })
    ));
}

#[test]
fn async_host_roots_agree_on_all_three_engines() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
    for id in [
        "a294-async-host-order",
        "a295-async-host-aliases",
        "a296-async-main-alias",
    ] {
        let source = std::fs::read_to_string(root.join(format!("{id}.ts"))).unwrap();
        let files = [SourceFile::entry(format!("{id}.ts"), source)];
        let expected = std::fs::read(root.join(format!("{id}.expected"))).unwrap();
        let checked = check_program(&files).unwrap();
        let lir = subscript_codegen::lir::lower_module(&checked).unwrap();
        let interpreted = subscript_codegen::interpreter::interpret(&lir).unwrap();
        assert_eq!(interpreted, expected, "{id}: interpreter");
        assert_eq!(
            subscript_codegen::run_jit(&files).unwrap(),
            expected,
            "{id}: dev"
        );
        assert_eq!(
            subscript_codegen::run_c_aot(&files).unwrap(),
            expected,
            "{id}: ship"
        );
        let mut added = files.clone();
        added[0]
            .source
            .push_str("\nexport async function extra(): Promise<void> { print('extra'); }\n");
        let mut with_extra = expected.clone();
        with_extra.extend_from_slice(b"extra\n");
        assert_eq!(subscript_codegen::run_jit(&added).unwrap(), with_extra);
        assert_ne!(with_extra, expected, "additional root firing control");
    }
}

#[test]
fn capture_discovers_a_directory_entry_and_reports_a_missing_entry() {
    let captured = std::process::Command::new(env!("CARGO_BIN_EXE_capture"))
        .arg("a291-host-api-aliases")
        .output()
        .unwrap();
    assert!(
        captured.status.success(),
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert_eq!(captured.stdout, b"physics 1\naudio 2\n");
    let missing = std::process::Command::new(env!("CARGO_BIN_EXE_capture"))
        .arg("s129-nonexistent-entry")
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
}

#[test]
fn runtime_and_program_headers_accept_a_parameterized_main_in_both_orders() {
    use subscript_codegen::{add_c11_optimized_flags, host_c_compiler, include_directory_arg};
    let directory = std::env::temp_dir().join(format!("s129-headers-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program = emit_c_without_main(
        &check_program(&[SourceFile::entry(
            "api.ts",
            "export function main(value: i32): void { print(`${value}`); }",
        )])
        .unwrap(),
    )
    .unwrap();
    assert!(program
        .host_header
        .starts_with("/* DO NOT EDIT. Generated by subscript codegen"));
    std::fs::write(directory.join("program.h"), &program.host_header).unwrap();
    std::fs::write(
        directory.join("subscript_runtime.h"),
        subscript_runtime::host_header::render().unwrap(),
    )
    .unwrap();
    let compiler = host_c_compiler().unwrap();
    for includes in [
        "#include \"subscript_runtime.h\"\n#include \"program.h\"",
        "#include \"program.h\"\n#include \"subscript_runtime.h\"",
    ] {
        for (arguments, succeeds) in [("ctx, 7", true), ("ctx", false)] {
            let body = format!("{includes}\nvoid host(subscript_rt_context* ctx) {{ subscript_export_main({arguments}); }}\n");
            let path = directory.join("host.c");
            std::fs::write(&path, body).unwrap();
            let mut command = compiler.command();
            add_c11_optimized_flags(&mut command, compiler.style());
            command.arg(include_directory_arg(compiler.style(), &directory));
            command
                .arg(if compiler.style().is_msvc() {
                    "/Zs"
                } else {
                    "-fsyntax-only"
                })
                .arg(&path);
            let result = command.output().unwrap();
            assert_eq!(
                result.status.success(),
                succeeds,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cross_module_async_roots_follow_export_sites_for_every_input_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus/accept/a297-async-export-site-order");
    let files = [
        SourceFile::entry(
            "main.ts",
            std::fs::read_to_string(root.join("main.ts")).unwrap(),
        ),
        SourceFile::new(
            "zlib.ts",
            std::fs::read_to_string(root.join("zlib.ts")).unwrap(),
        ),
        SourceFile::new(
            "alib.ts",
            std::fs::read_to_string(root.join("alib.ts")).unwrap(),
        ),
    ];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let inputs = order.map(|index| files[index].clone());
        let hir = check_program(&inputs).unwrap();
        let lir = subscript_codegen::lir::lower_module(&hir).unwrap();
        let names: Vec<_> = lir
            .async_roots
            .iter()
            .map(|id| lir.functions[id.0 as usize].source_name.as_str())
            .collect();
        assert_eq!(names, ["zeta", "beta", "alpha"]);
        let mut reversed = lir.clone();
        reversed.async_roots.reverse();
        assert!(subscript_codegen::lir::verify_module(&reversed)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("export-site order")));
        let expected = b"main\nzeta\nbeta\nalpha\n";
        assert_eq!(subscript_codegen::run_jit(&inputs).unwrap(), expected);
        assert_eq!(subscript_codegen::run_c_aot(&inputs).unwrap(), expected);
        assert_eq!(
            subscript_codegen::interpreter::interpret(&lir).unwrap(),
            expected
        );
    }
    let mut changed = files.clone();
    changed[0].source = "export { beta } from './alib'; export { root as main, root as start, zeta, alpha, zeta as again } from './zlib';".into();
    assert_eq!(
        subscript_codegen::run_jit(&changed).unwrap(),
        b"main\nbeta\nzeta\nalpha\n"
    );
}

#[test]
fn directory_reader_requires_exact_main_file() {
    #[allow(dead_code)]
    #[path = "corpus/mod.rs"]
    mod corpus;
    let directory = std::env::temp_dir().join(format!("s129-directory-{}", std::process::id()));
    let entry = directory.join("entry");
    std::fs::create_dir_all(&entry).unwrap();
    std::fs::write(
        entry.join("a-main.ts"),
        "export function helper(): void { print('helper'); }",
    )
    .unwrap();
    std::fs::write(
        entry.join("main.ts"),
        "import { helper } from './a-main'; export function main(): void { helper(); }",
    )
    .unwrap();
    let files = corpus::entry_sources(&directory, "entry");
    assert_eq!(
        files.iter().find(|source| source.entry).unwrap().name,
        "main.ts"
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), b"helper\n");
    std::fs::remove_file(entry.join("main.ts")).unwrap();
    assert!(std::panic::catch_unwind(|| corpus::entry_sources(&directory, "entry")).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn reload_runner_kicks_roots_pumps_and_uses_the_current_generation() {
    let source = "export function main(): void { print('main'); } export async function work(): Promise<void> { print('before'); }";
    let input = [SourceFile::entry("main.ts", source)];
    let mut session = ReloadSession::new(&input).unwrap();
    session.run_main().unwrap();
    assert_eq!(session.take_output(), b"main\nbefore\n");
    assert_eq!(session.async_pending(), 0);
    session
        .reload(&[SourceFile::entry(
            "main.ts",
            source.replace("'before'", "'after'"),
        )])
        .unwrap();
    session.run_main().unwrap();
    assert_eq!(session.take_output(), b"main\nafter\n");
    let mut missing = ReloadSession::new(&[SourceFile::entry(
        "api.ts",
        "export async function work(): Promise<void> { print('wrong'); }",
    )])
    .unwrap();
    assert!(matches!(
        missing.run_main(),
        Err(subscript_codegen::RunError::Rejected(_))
    ));
    assert!(missing.take_output().is_empty());
}
