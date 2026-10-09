//! Standard-file operation, provider, verifier, and reload witnesses (§185).
#[allow(dead_code)]
#[path = "support/files.rs"]
mod fixture;

use subscript_codegen::{
    run_c_aot_configured, run_jit_configured, ReloadSession, RunConfig, RunError,
};
use subscript_compiler::{hir::StandardHostOperation, lir, SourceFile};
use subscript_runtime::TrapKind;

/// Rule 3: the provider is set before the module initializer runs. The
/// initializer starts a read; `main` awaits it.
#[test]
fn a_pre_init_hook_provider_is_visible_to_the_module_initializer() {
    let sources = [SourceFile::entry(
        "main.ts",
        "import {readFile} from 'node:fs/promises';\n\
         const early: Promise<string> = readFile('init.txt', 'utf8');\n\
         export async function main(): Promise<void> {\n\
         try { print(await early); } catch (e) {\n\
         if (e instanceof Error) { print(`${e.name}:${e.message}`); } }\n\
         }\n",
    )];
    let mut host = fixture::Fixture::new();
    std::fs::write(host.root().join("init.txt"), "from init").expect("write init file");
    let found = b"from init\n";
    assert_eq!(
        run_jit_configured(&sources, host.config())
            .expect("dev initializer read")
            .stdout,
        found
    );
    let library = [host.library()];
    let mut config = RunConfig::default().with_enabled_modules(&["node:fs/promises"]);
    config.native_libraries = &library;
    config.pre_init_hook = Some("subscript_test_files_setup");
    assert_eq!(
        run_c_aot_configured(&sources, config)
            .expect("ship initializer read")
            .stdout,
        found
    );
    // The firing control: the same provider set after the initializer is
    // not visible to it, so the read completes with the rule 5 Error.
    config.pre_init_hook = None;
    config.pre_entry_hook = Some("subscript_test_files_setup");
    assert_eq!(
        run_c_aot_configured(&sources, config)
            .expect("ship read after initializer")
            .stdout,
        b"Error:missing file provider\n"
    );
}

/// The shipping tier cannot call a Rust provider, so it refuses one.
#[test]
fn the_shipping_tier_refuses_a_rust_file_provider() {
    let sources = [SourceFile::entry(
        "main.ts",
        "export function main(): void { print(\"x\"); }\n",
    )];
    let mut host = fixture::Fixture::new();
    let refused = run_c_aot_configured(&sources, host.config());
    assert!(
        matches!(refused, Err(RunError::Internal(ref message)) if message.contains("file provider")),
        "{refused:?}"
    );
}

#[test]
fn no_provider_is_a_caught_error_in_both_tiers() {
    let sources = [SourceFile::entry("main.ts", "import {readFile} from 'node:fs/promises'; export async function main():Promise<void>{try{await readFile('absent','utf8');}catch(e){if(e instanceof Error)print(`${e.name}:${e.message}`);}}")];
    let config = RunConfig::default().with_enabled_modules(&["node:fs/promises"]);
    let expected = b"Error:missing file provider\n";
    assert_eq!(
        run_jit_configured(&sources, config)
            .expect("dev error")
            .stdout,
        expected
    );
    assert_eq!(
        run_c_aot_configured(&sources, config)
            .expect("ship error")
            .stdout,
        expected
    );
}

#[test]
fn standard_result_verifier_uses_the_operation_contract() {
    let source = [SourceFile::entry("main.ts", "import {readFile} from 'node:fs/promises'; export async function main():Promise<void>{print(await readFile('file','utf8'));}")];
    let mut options = subscript_compiler::CheckOptions::default();
    options.enabled_modules.push("node:fs/promises".into());
    let hir = subscript_compiler::check_program_with(&source, &options).expect("file HIR");
    let mut module = subscript_codegen::lir::lower_module(&hir).expect("file LIR");
    subscript_codegen::lir::verify_module(&module).expect("read-text control");
    let instruction = module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, lir::InstructionKind::HostCompletion { .. }))
        .expect("completion");
    // Build a bytes-read operation with the text-read instruction's actual result.
    if let lir::InstructionKind::HostCompletion { target, .. } = &mut instruction.kind {
        *target = lir::HostCompletionTarget::Standard(StandardHostOperation::ReadBytes);
    }
    let errors =
        subscript_codegen::lir::verify_module(&module).expect_err("operation/result mismatch");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("result is invalid")),
        "{errors:?}"
    );
}

#[test]
fn new_frame_awaits_a_pre_reload_source_but_old_waiter_traps() {
    let old = "import {readFile,writeFile} from 'node:fs/promises'; let held:Promise<string>[]=[]; export async function main():Promise<void>{await writeFile('pending.txt','old');held.push(readFile('pending.txt','utf8'));} export async function consume():Promise<void>{print(await held[0]);} export async function release():Promise<void>{await writeFile('release.txt','x');}";
    let sources = |text: &str| [SourceFile::entry("main.ts", text)];
    let mut host = fixture::Fixture::new();
    let mut session = ReloadSession::new_configured(&sources(old), host.config()).expect("session");
    session.call_export("main").expect("source before reload");
    while session.async_pending() != 0 {
        session.async_step().expect("start source");
    }
    session
        .reload(&sources(&old.replace("'old'", "'new'")))
        .expect("reload");
    session.call_export("consume").expect("new waiter");
    session.call_export("release").expect("complete source");
    while session.async_pending() != 0 {
        session.async_step().expect("resume new waiter");
    }
    assert_eq!(session.take_output(), b"old\n");
    drop(session);

    let mut host = fixture::Fixture::new();
    let mut session =
        ReloadSession::new_configured(&sources(old), host.config()).expect("control session");
    session.call_export("main").expect("control source");
    while session.async_pending() != 0 {
        session.async_step().expect("start control source");
    }
    session.call_export("consume").expect("old waiter");
    session
        .reload(&sources(&old.replace("'old'", "'new'")))
        .expect("control reload");
    session.call_export("release").expect("complete old source");
    let error = session.async_step().expect_err("old waiter must trap");
    assert!(
        matches!(error, RunError::Trap(ref report) if report.rule == TrapKind::StaleCoroutine),
        "{error:?}"
    );
}

/// The configured capturing constructor applies the modules and the provider,
/// and reports an initializer trap.
#[test]
fn a_configured_capturing_session_reports_an_initializer_trap() {
    let text = "import {readFile} from 'node:fs/promises';\n\
                const xs: i32[] = [];\n\
                const bad: i32 = xs[1];\n\
                export async function main(): Promise<void> { print(await readFile('init.txt', 'utf8')); }\n";
    let mut host = fixture::Fixture::new();
    std::fs::write(host.root().join("init.txt"), "ok").expect("write init file");
    let (_, trap) = ReloadSession::new_capturing_initializer_trap_configured(
        &[SourceFile::entry("main.ts", text)],
        host.config(),
    )
    .expect("trapped session");
    let trap = trap.expect("initializer trap");
    assert_eq!(trap.rule, TrapKind::IndexOutOfBounds, "{trap}");
    // The firing control: without the trap there is no report, and `main`
    // reads through the provider.
    let clean = text.replace("const bad: i32 = xs[1];\n", "");
    let (mut session, trap) = ReloadSession::new_capturing_initializer_trap_configured(
        &[SourceFile::entry("main.ts", clean)],
        host.config(),
    )
    .expect("clean session");
    assert!(trap.is_none());
    session.call_export("main").expect("main");
    while session.async_pending() != 0 {
        session.async_step().expect("complete read");
    }
    assert_eq!(session.take_output(), b"ok\n");
}

/// The verifier compares the operands with the operation signature.
#[test]
fn standard_operand_verifier_uses_the_operation_signature() {
    let source = [SourceFile::entry("main.ts", "import {writeFile} from 'node:fs/promises'; export async function main():Promise<void>{await writeFile('file','text');}")];
    let mut options = subscript_compiler::CheckOptions::default();
    options.enabled_modules.push("node:fs/promises".into());
    let hir = subscript_compiler::check_program_with(&source, &options).expect("file HIR");
    let mut module = subscript_codegen::lir::lower_module(&hir).expect("file LIR");
    subscript_codegen::lir::verify_module(&module).expect("write-text control");
    let instruction = module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, lir::InstructionKind::HostCompletion { .. }))
        .expect("completion");
    // Build a bytes write with the text write's operands; the void result still agrees.
    if let lir::InstructionKind::HostCompletion { target, .. } = &mut instruction.kind {
        *target = lir::HostCompletionTarget::Standard(StandardHostOperation::WriteBytes);
    }
    let errors = subscript_codegen::lir::verify_module(&module).expect_err("operand mismatch");
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("operand types disagree with its signature")),
        "{errors:?}"
    );
}

/// A reload session refuses an option that it does not apply.
#[test]
fn a_reload_session_refuses_an_option_it_does_not_apply() {
    let sources = [SourceFile::entry(
        "main.ts",
        "export function main(): void { print(\"x\"); }\n",
    )];
    let base = RunConfig::default().with_enabled_modules(&["node:fs/promises"]);
    let mut refused = Vec::new();
    for option in 0..6 {
        let mut config = base;
        match option {
            0 => config.pre_init_hook = Some("hook"),
            1 => config.pre_entry_hook = Some("hook"),
            2 => config.post_run_hook = Some("hook"),
            3 => config.fail_alloc_after = Some(1),
            4 => config.freed_handle_diagnostics = true,
            _ => config.memory_accounting = true,
        }
        refused.push(matches!(
            ReloadSession::new_configured(&sources, config).err(),
            Some(RunError::Internal(_))
        ));
    }
    assert_eq!(refused, [true; 6]);
    // The firing control: the base record builds a session.
    let mut host = fixture::Fixture::new();
    let mut session = ReloadSession::new_configured(&sources, host.config()).expect("session");
    session.call_export("main").expect("main");
    assert_eq!(session.take_output(), b"x\n");
}
