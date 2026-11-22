//! `Worker.receive()` on the spawning side (`specs/blocks/compiler.md` §349).

use super::*;

const DECLS: &str = "class Msg { v: i32 = 0; }\n\
     function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {\n\
     \x20 const m: Msg | null = inbox.wait();\n\
     \x20 if (m !== null) { outbox.post(m); }\n\
     }\n";

fn program(body: &str) -> String {
    format!(
        "{DECLS}export async function main(): Promise<void> {{\n\
         \x20 const w: Worker<Msg, Msg> = Worker.spawn(echo);\n\
         {body}\n\
         \x20 w.close();\n\
         \x20 w.join();\n\
         }}\n"
    )
}

fn main_body(module: &hir::Module) -> &[hir::Stmt] {
    &module
        .functions
        .iter()
        .find(|function| function.symbol.source_name() == "main")
        .expect("main is in the module")
        .body
}

fn let_init<'m>(module: &'m hir::Module, name: &str) -> &'m hir::Expr {
    main_body(module)
        .iter()
        .find_map(|statement| match statement {
            hir::Stmt::Let {
                name: local, init, ..
            } if local.as_str() == name => Some(init),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no local `{name}`"))
}

fn is_receive_call(expr: &hir::Expr) -> bool {
    matches!(
        expr.kind,
        hir::ExprKind::Call {
            callee: hir::Callee::Worker(hir::WorkerFn::Receive),
            ..
        }
    )
}

fn message_class(module: &hir::Module) -> ClassId {
    let index = module
        .classes
        .iter()
        .position(|class| class.name == "Msg")
        .expect("Msg is in the module");
    ClassId(index)
}

// Cost: one check of a 13-line program; no compilation or external process.
#[test]
fn rule_1_receive_has_the_type_of_a_nullable_message_handle() {
    let module = check_one(&program(
        "  const h: Promise<Msg | null> = w.receive();\n  await h;",
    ))
    .expect("a held receive handle checks");
    let init = let_init(&module, "h");
    let expected = Type::async_handle(Type::Nullable(
        Box::new(Type::Class(message_class(&module))),
        crate::types::Absence::Null,
    ));
    assert_eq!(init.ty, expected);
    // The call passes through the handle transfer that S013 tracks.
    let hir::ExprKind::AsyncHandleTransfer { value, .. } = &init.kind else {
        panic!("the receive call is not a tracked handle: {:?}", init.kind);
    };
    assert!(is_receive_call(value), "{:?}", value.kind);
}

// Cost: one check of a 12-line program.
#[test]
fn rule_1_await_receive_gives_the_nullable_message() {
    let module = check_one(&program("  const m: Msg | null = await w.receive();"))
        .expect("an awaited receive checks");
    let init = let_init(&module, "m");
    assert_eq!(
        init.ty,
        Type::Nullable(
            Box::new(Type::Class(message_class(&module))),
            crate::types::Absence::Null,
        )
    );
    let hir::ExprKind::AsyncHandleAwait(handle) = &init.kind else {
        panic!("the await is not a handle await: {:?}", init.kind);
    };
    let hir::ExprKind::AsyncHandleTransfer { value, .. } = &handle.kind else {
        panic!(
            "the awaited receive is not a tracked handle: {:?}",
            handle.kind
        );
    };
    assert!(is_receive_call(value), "{:?}", value.kind);
}

// Cost: three checks of short programs.
#[test]
fn rule_1_a_receive_handle_composes_as_a_promise() {
    let all = format!(
        "{DECLS}export async function main(): Promise<void> {{\n\
         \x20 const a: Worker<Msg, Msg> = Worker.spawn(echo);\n\
         \x20 const b: Worker<Msg, Msg> = Worker.spawn(echo);\n\
         \x20 const rs: (Msg | null)[] = await Promise.all([a.receive(), b.receive()]);\n\
         \x20 a.close(); b.close(); a.join(); b.join();\n\
         }}\n"
    );
    assert!(check_one(&all).is_ok(), "{:?}", check_one(&all).err());
    // A TaskGroup task awaits a receive (§349.2 acceptance 2d).
    let group = format!(
        "{DECLS}async function take(w: Worker<Msg, Msg>): Promise<void> {{\n\
         \x20 const m: Msg | null = await w.receive();\n\
         }}\n\
         export async function main(): Promise<void> {{\n\
         \x20 const w: Worker<Msg, Msg> = Worker.spawn(echo);\n\
         \x20 const g = new TaskGroup();\n\
         \x20 g.add(take(w));\n\
         \x20 await g.join();\n\
         \x20 w.close(); w.join();\n\
         }}\n"
    );
    let checked = check_one(&group);
    assert!(checked.is_ok(), "{:?}", checked.err());
    let returned = format!(
        "{DECLS}function next(w: Worker<Msg, Msg>): Promise<Msg | null> {{ return w.receive(); }}\n\
         export async function main(): Promise<void> {{\n\
         \x20 const w: Worker<Msg, Msg> = Worker.spawn(echo);\n\
         \x20 const m: Msg | null = await next(w);\n\
         \x20 w.close(); w.join();\n\
         }}\n"
    );
    let checked = check_one(&returned);
    assert!(checked.is_ok(), "{:?}", checked.err());
}

// Cost: two checks of short programs.
#[test]
fn rule_1_a_dropped_receive_handle_is_s013() {
    let diagnostics = check_one(&program("  w.receive();")).expect_err("a bare receive statement");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S013, "{diagnostics:?}");
    // Control: the awaited statement form is accepted.
    let awaited = check_one(&program("  await w.receive();"));
    assert!(awaited.is_ok(), "{:?}", awaited.err());
}

// Cost: three checks of short programs.
#[test]
fn receive_with_an_argument_has_the_diagnostic_of_poll() {
    let message = |body: &str| {
        let diagnostics = check_one(&program(body)).expect_err("an extra argument");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{diagnostics:?}");
        diagnostics[0].message.clone()
    };
    let poll = message("  const p: Msg | null = w.poll(1);");
    let held = message("  const h: Promise<Msg | null> = w.receive(1);\n  await h;");
    let awaited = message("  const m: Msg | null = await w.receive(1);");
    assert_eq!(poll.replace("poll", "receive"), held);
    assert_eq!(held, awaited);
}

// Cost: two checks of short programs.
#[test]
fn receive_with_a_type_argument_has_the_builtin_diagnostic_in_each_form() {
    let message = |body: &str| {
        let diagnostics = check_one(&program(body)).expect_err("a type argument");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        diagnostics[0].message.clone()
    };
    let held = message("  const h: Promise<Msg | null> = w.receive<Msg>();\n  await h;");
    let awaited = message("  const m: Msg | null = await w.receive<Msg>();");
    assert_eq!(held, "`receive` takes no type arguments, got 1");
    assert_eq!(held, awaited);
}
