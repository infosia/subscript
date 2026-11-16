//! The pre-store zero value in each tier (compiler.md §322 rule 2).

use super::*;
use cranelift_jit::{JITBuilder, JITModule};
use subscript_compiler::{check_program, hir};
use subscript_runtime::Context;

fn before_store_module() -> hir::Module {
    let source = "class A { x: i32 = 1; } function mk(): A | null { return new A(); } class K { static readonly R: A | null = mk(); } export function main(): void { print(`${K.R === null}`); }";
    let mut module = check_program(&[SourceFile::entry("main.ts", source)])
        .expect("initialized static field control");
    let read = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap()
        .body[0]
        .clone();
    // §71 rejects the source route before execution. Build the §322 pre-store read in HIR for the tier test.
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "mk")
        .unwrap()
        .body
        .insert(0, read);
    module
}

fn jit_output(hir: &hir::Module) -> Vec<u8> {
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(crate::lower::dev_flags().unwrap())
        .unwrap();
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    crate::jit::install_reservation(&mut builder, hir.source_bytes).unwrap();
    crate::jit::register_runtime(&mut builder);
    let mut module = JITModule::new(builder);
    let lowered =
        crate::lower::lower_module_with(&mut module, hir, crate::lower::LowerOptions::default())
            .unwrap();
    module.finalize_definitions().unwrap();
    let mut output = Vec::<u8>::new();
    unsafe extern "C" fn capture(userdata: *mut std::ffi::c_void, line: *const u8, len: u64) {
        // SAFETY: the observer receives this test's live output vector and a runtime line slice.
        let output = unsafe { &mut *userdata.cast::<Vec<u8>>() };
        let line = unsafe { std::slice::from_raw_parts(line, len as usize) };
        output.extend_from_slice(line);
        output.push(b'\n');
    }
    {
        let mut ctx = Context::new();
        // SAFETY: the callback and output vector outlive the Context and every script call.
        unsafe { ctx.set_print_observer(Some(capture), std::ptr::from_mut(&mut output).cast()) };
        for function in [lowered.init, lowered.main_id().unwrap()] {
            // SAFETY: both finalized entries have the `(Context*) -> void` host ABI and the module remains live.
            let entry: unsafe extern "C" fn(*mut Context) =
                unsafe { std::mem::transmute(module.get_finalized_function(function)) };
            ctx.enter_script();
            unsafe { entry(&mut *ctx) };
            ctx.exit_script();
            assert!(!ctx.trapped(), "{:?}", ctx.trap_record());
        }
    }
    // SAFETY: every script call returned, the Context is dropped, and no code pointer escapes.
    unsafe { module.free_memory() };
    output
}

fn ship_output(hir: &hir::Module) -> Vec<u8> {
    let program = crate::emit_c(hir).expect("pre-store C emission");
    let dir = TempDir::new("static-readonly").unwrap();
    let source = dir.path.join("program.c");
    let entry = dir.path.join("entry.c");
    let executable = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&source, program.source.as_bytes()).unwrap();
    write_file(&dir.path.join("program.h"), program.host_header.as_bytes()).unwrap();
    write_file(&entry, AOT_ENTRY_C.as_bytes()).unwrap();
    let cc = host_c_compiler().unwrap();
    let mut command = cc.command();
    add_c11_optimized_flags(&mut command, cc.style());
    add_object_directory(&mut command, &dir.path, cc.style());
    command.arg(include_directory_arg(cc.style(), &dir.path));
    command
        .arg(source)
        .arg(entry)
        .arg(runtime_staticlib().unwrap());
    command.args(runtime_system_libraries(cc.style()));
    add_executable_output(&mut command, &executable, cc.style());
    command.args(unreferenced_code_removal_arguments(cc.style()));
    let compiled = command.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        tool_output_report(&compiled)
    );
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success(), "{}", tool_output_report(&output));
    output.stdout
}

/// Cost: 0.52 seconds; one checker program, one interpreter run, one JIT run, and one ship-C compile and run.
#[test]
fn static_readonly_field_is_null_before_its_initializer_store() {
    let started = std::time::Instant::now();
    let module = before_store_module();
    let lir = crate::lir::lower_module(&module).expect("pre-store LIR");
    let expected = b"true\nfalse\n";
    assert_eq!(
        crate::interpreter::interpret(&lir).expect("interpreter"),
        expected
    );
    assert_eq!(jit_output(&module), expected, "dev JIT");
    assert_eq!(ship_output(&module), expected, "ship C");
    eprintln!("pre-store tier test: {:?}", started.elapsed());
}
