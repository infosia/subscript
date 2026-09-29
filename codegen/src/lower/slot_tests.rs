//! Checks dev-JIT slot order independently of C function ids.

use super::{dev_flags, lower_module_with, LowerOptions};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Module};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn error_constructor_does_not_move_user_function_slots() {
    let source = r#"
class User { constructor() {} method(): void {} }
export function first(): void {}
export function main(): void { first(); const user = new User(); user.method(); }
"#;
    for body in [
        "",
        "print(JSON.stringify(JSON.parse<i32>(\"42\")));",
        "print(decodeURI(\"hello%20world\"));",
        "print(new Error(\"first\").message);",
    ] {
        let source = source.replace("first();", &format!("first(); {body}"));
        let hir = check_program(&[SourceFile::new("slots.ts", &source)]).expect("slot source");
        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let lowered =
            lower_module_with(&mut module, &hir, LowerOptions::default()).expect("dev lowering");
        let slots = lowered
            .slots
            .iter()
            .map(|id| id.and_then(|id| module.declarations().get_function_decl(id).name.clone()))
            .collect::<Vec<_>>();
        // SAFETY: no code address leaves this test, and no function runs.
        unsafe { module.free_memory() };
        for (slot, name) in [
            (0, "subscript_export_first"),
            (2, "subscript_export_main"),
            (4, "subscript_ctor0"),
            (5, "subscript_ctor1"),
            (6, "subscript_m1_0"),
        ] {
            assert_eq!(slots[slot].as_deref(), Some(name), "{body}");
        }
        assert_eq!(slots[7].as_deref(), Some("subscript_init"));
    }
}

#[test]
fn globals_with_one_source_name_keep_distinct_storage() {
    use super::{lower_lir_module_with, LowerOptions};
    use subscript_runtime::Context;

    // compiler.md §125.2 item 4: LIR identity owns the storage slot.
    let mut hir = check_program(&[SourceFile::new(
        "slots.ts",
        "let a: i32 = 7; let b: i32 = 100; function first(): i32 { return a; } function second(): i32 { return b; } export function main(): void { b += 1; print(`${first()} ${second()}`); }",
    )])
    .expect("storage source");
    for global in &mut hir.globals {
        global.name = "x".to_string();
    }
    for function in &mut hir.functions {
        if !function.exported {
            function.name = "read".to_string();
        }
    }
    let lir = crate::lir::lower_module(&hir).expect("storage LIR");
    for reload in [false, true] {
        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        builder.memory_provider(Box::new(
            cranelift_jit::ArenaMemoryProvider::new_with_size(1 << 20).expect("test reservation"),
        ));
        crate::jit::register_runtime(&mut builder);
        let mut module = JITModule::new(builder);
        let lowered = lower_lir_module_with(
            &mut module,
            &lir,
            LowerOptions {
                reload,
                ..LowerOptions::default()
            },
        )
        .expect("storage lowering");
        module.finalize_definitions().expect("finalized code");
        let table = lowered
            .slots
            .iter()
            .map(|id| id.map_or(std::ptr::null(), |id| module.get_finalized_function(id)))
            .collect::<Vec<_>>();
        let mut ctx = Context::new();
        ctx.set_fn_table(table.as_ptr());
        let mut globals = vec![0_u64; (lowered.globals_size as usize).div_ceil(8)];
        if reload {
            ctx.set_globals(globals.as_mut_ptr().cast());
        }
        type Entry = unsafe extern "C" fn(*mut Context);
        // SAFETY: both finalized entries take one live Context; the module outlives each call.
        unsafe {
            let init: Entry = std::mem::transmute(module.get_finalized_function(lowered.init));
            init(&mut *ctx);
            let main: Entry = std::mem::transmute(
                module.get_finalized_function(lowered.main_id().expect("main entry")),
            );
            main(&mut *ctx);
        }
        assert!(ctx.trap_record().is_none());
        assert_eq!(ctx.take_stdout(), b"7 101\n", "reload={reload}");
        drop(ctx);
        // SAFETY: execution has returned, and no generated pointer remains in use.
        unsafe { module.free_memory() };
    }
}

#[test]
fn shared_targets_have_distinct_public_adapter_symbols() {
    for declaration in [
        "function local(): void {}",
        "async function local(): Promise<void> {}",
    ] {
        let source = format!("{declaration} export {{ local as first, local as second }}; export function main(): void {{}}");
        let hir = check_program(&[SourceFile::new("aliases.ts", &source)]).expect("alias source");
        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let lowered =
            lower_module_with(&mut module, &hir, LowerOptions::default()).expect("dev lowering");
        let mut symbols = lowered
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    module
                        .declarations()
                        .get_function_decl(entry.id)
                        .name
                        .clone(),
                )
            })
            .collect::<Vec<_>>();
        symbols.sort();
        assert_eq!(
            symbols,
            vec![
                ("first".into(), Some("subscript_export_first".into())),
                ("main".into(), Some("subscript_export_main".into())),
                ("second".into(), Some("subscript_export_second".into())),
            ]
        );
        assert_eq!(
            hir.functions
                .iter()
                .filter(|function| function.name == "local")
                .count(),
            1
        );
        // SAFETY: no code address leaves this test, and no function runs.
        unsafe { module.free_memory() };
    }
}
