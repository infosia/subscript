//! Reads the exception payload from actual suspended JIT frames.
use super::compile::compile_jit;
use cranelift_module::{FuncOrDataId, Module};
use subscript_compiler::SourceFile;
use subscript_runtime::Context;

#[test]
fn s180_jit_suspended_finalizers_have_distinct_frame_slots() {
    let sources = [SourceFile::new(
        "slots.ts",
        crate::finally_tests::FRAME_SOURCE,
    )];
    let hir = subscript_compiler::check_program(&sources).unwrap();
    let lir = crate::lir::lower_module(&hir).unwrap();
    let free_index = lir
        .functions
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                subscript_compiler::lir::FunctionKind::Free
                    | subscript_compiler::lir::FunctionKind::SynthesizedHelper
            )
        })
        .position(|f| f.source_name == "f")
        .unwrap();
    let (module, _) = compile_jit(&sources, &[]).unwrap();
    let FuncOrDataId::Func(id) = module
        .get_name(&format!("subscript_f{free_index}"))
        .unwrap()
    else {
        panic!("creator function");
    };
    let code = module.get_finalized_function(id);
    // The C ABI prefix is 16 bytes. The string parameter precedes the
    // object, report-string, and position locals in this frame.
    let check = |ctx: &Context, frame: *mut u8, expected: &[u8]| {
        // SAFETY: the frame remains registered and suspended throughout the read.
        unsafe {
            let object = frame.add(24).cast::<*mut u8>().read();
            let text = frame.add(32).cast::<*mut u8>().read();
            let position = frame.add(40).cast::<u32>().read();
            assert!(!object.is_null());
            assert_ne!(position, 0);
            assert_eq!(ctx.str_bytes(text), expected);
            object
        }
    };
    for count in 1..=2 {
        let mut ctx = Context::new();
        let a = ctx.alloc_str(b"A", 0);
        // SAFETY: finalized creator has the declared (Context*, string) ABI.
        let create: unsafe extern "C" fn(*mut Context, *mut u8) -> *mut u8 =
            unsafe { std::mem::transmute(code) };
        let first = unsafe { create(&mut *ctx, a) };
        // SAFETY: registered creator frame and exclusive Context.
        assert_eq!(
            unsafe { ctx.async_start(first, std::ptr::null_mut(), 0) },
            0
        );
        let second = if count == 2 {
            let b = ctx.alloc_str(b"B", 0);
            // SAFETY: same creator ABI and live parameter.
            let frame = unsafe { create(&mut *ctx, b) };
            // SAFETY: registered creator frame and exclusive Context.
            assert_eq!(
                unsafe { ctx.async_start(frame, std::ptr::null_mut(), 0) },
                0
            );
            Some(frame)
        } else {
            None
        };
        let object = check(&ctx, first, b"Error: A");
        if let Some(second) = second {
            assert_ne!(object, check(&ctx, second, b"Error: B"));
        }
        assert_eq!(ctx.parked_exception_count(), 0);
    }
    // SAFETY: all Contexts and frames are gone.
    unsafe { module.free_memory() };
}
