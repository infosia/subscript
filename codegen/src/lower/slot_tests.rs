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
