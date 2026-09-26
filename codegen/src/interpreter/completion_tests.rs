use super::*;

#[test]
fn async_completion_layout() {
    assert_eq!(std::mem::size_of::<Option<Completion>>(), 56);
    assert_eq!(std::mem::size_of::<Option<Value>>(), 56);
}

#[test]
fn a_direct_await_releases_its_only_owner() {
    for held in [false, true] {
        let source = format!(
            r#"const fault: Error = new Error("failure");
async function fails(): Promise<i32> {{ throw fault; }}
export async function main(): Promise<void> {{
    {}
    try {{ await {}; }} catch {{ print("caught"); }}
}}"#,
            if held {
                "const h: Promise<i32> = fails();"
            } else {
                ""
            },
            if held { "h" } else { "fails()" },
        );
        let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "completion.ts",
            source,
        )])
        .expect("checked source");
        let module = crate::lir::lower_module(&hir).expect("lowered source");
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        if let Some(initializer) = module.initializer {
            interpreter
                .call_function(initializer, Vec::new())
                .expect("initializer");
        }
        let Value::Coroutine(root) = interpreter
            .call_function(module.entry.expect("entry"), Vec::new())
            .expect("root")
        else {
            panic!("async root");
        };
        interpreter.async_kick(&root).expect("kick");
        let child = Rc::clone(
            &root
                .borrow()
                .awaiting
                .as_ref()
                .expect("registration")
                .handle,
        );
        assert_eq!(child.borrow().owners, if held { 2 } else { 1 });
        assert!(interpreter
            .async_handles
            .borrow()
            .contains_key(&(Rc::as_ptr(&child) as usize)));
        interpreter.async_step().expect("completion read");
        assert_eq!(child.borrow().owners, 0);
        assert!(!interpreter
            .async_handles
            .borrow()
            .contains_key(&(Rc::as_ptr(&child) as usize)));
        assert_eq!(interpreter.context.take_stdout(), b"caught\n");
    }
}
