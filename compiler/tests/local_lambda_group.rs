//! Checker facts for compiler.md §293.
//! Cost: twelve small checker calls plus ten reject probes; measured 0.02 s (debug).
use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

fn check(source: &str) -> Result<hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new("group.ts", source)])
}

#[test]
fn members_have_the_same_capture_union_and_ownership() {
    for (call, owned) in [("first();", false), ("return first;", true)] {
        let result = if owned { "() => i32" } else { "void" };
        let module = check(&format!("function make(): {result} {{ const a: i32 = 4; const b: i32 = 5; const first = (): i32 => second() + a; const second = (n: i32 = b): i32 => n; {call} }} export function main(): void {{}}" )).unwrap();
        let function = module.functions.iter().find(|f| f.name == "make").unwrap();
        let members: Vec<_> = function
            .body
            .iter()
            .filter_map(|s| match s {
                hir::Stmt::Let { init, .. } => match &init.kind {
                    hir::ExprKind::Lambda {
                        id,
                        group,
                        captures,
                        owns_environment,
                        ..
                    } => Some((id, group, captures, owns_environment)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].1, members[1].1);
        assert_ne!(members[0].0, members[1].0);
        for (_, _, captures, owns) in members {
            assert_eq!(
                captures.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
                ["a", "b"]
            );
            assert_eq!(*owns, owned);
        }
    }
}

#[test]
fn async_or_a_member_value_escape_owns_every_member() {
    for declarations in [
        "const first = async (): Promise<i32> => second(); const second = (): i32 => captured; await first();",
        "const first = (): i32 => second(); const second = (): i32 => { values.push(first); return captured; }; first();",
    ] {
        let module = check(&format!("export async function main(): Promise<void> {{ const values: (() => i32)[] = []; const captured: i32 = 7; {declarations} }}")).unwrap();
        for s in &module.functions[0].body {
            if let hir::Stmt::Let { init, .. } = s {
                if let hir::ExprKind::Lambda { owns_environment, captures, .. } = &init.kind {
                    assert!(*owns_environment);
                    assert!(captures.iter().any(|c| c.name == "captured"));
                }
            }
        }
    }
}

#[test]
fn undecided_peer_results_and_nested_member_reads_reject() {
    for body in [
        "const a = (): void => { b(); }; const b = () => 1; a();",
        "const a = () => b(); const b = (): i32 => a(); a();",
        "const a = (): void => { const b = (): void => { a(); }; b(); }; a();",
        "const a = (): void => { const b = (): void => { const value = a; value(); }; b(); }; a();",
        "a(); const a = (): void => {};",
        "const a = (): void => b(); const b = (): void => { const nested = (): void => a(); nested(); }; a();",
        "const a = (): void => { b(); }; print(\"gap\"); const b = (): void => { a(); }; a();",
        "const a = (n: i32 = b(1)): i32 => n; const b = (n: i32): i32 => n + 1; print(`${a()}`);",
        "const b = (n: i32 = c(1)): i32 => n; const c = (n: i32): i32 => b(n) + 1; b();",
        "const a = (f: () => i32 = b): i32 => f(); const b = (): i32 => 2; print(`${a()}`);",
    ] {
        let errors = check(&format!("export function main(): void {{ {body} }}")).expect_err(body);
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
    }
}

#[test]
fn shadowing_and_inferred_default_parameter_types_check() {
    for body in [
        "const a = (): i32 => b(); const b = (n = 7): i32 => n; a();",
        "const a = (b: i32): i32 => b; const b = (): i32 => a(3); b();",
        "const a = (): void => { const b: i32 = 3; print(`${b}`); }; const b = (): void => { a(); }; b();",
        "const a = (): void => { b(); }; const b = () => {}; a();",
        "const a = (): i32 => 3; const b = (): i32 => { const nested = (): i32 => a(); return nested(); }; b();",
    ] {
        check(&format!("export function main(): void {{ {body} }}")).unwrap_or_else(|e| panic!("{body}: {e:?}"));
    }
}

#[test]
fn only_linked_members_share_captures_and_ownership_within_a_run() {
    let module = check(r#"
export async function main(): Promise<void> {
    const left: i32 = 4;
    const right: i32 = 5;
    const alone = (): i32 => left;
    const selfCall = (n: i32): i32 => { if (n > 0) return selfCall(n - 1); return left; };
    const selfValue = (): i32 => { const value = selfValue; return left; };
    const first = (): i32 => second();
    const second = (): i32 => left;
    const third = async (): Promise<i32> => fourth();
    const fourth = (): i32 => right;
    const shadow = (third: i32): i32 => third;
    const localShadow = (): i32 => { const third: i32 = left; return third; };
    print(`${alone()}:${selfCall(1)}:${selfValue()}:${first()}:${await third()}:${shadow(1)}:${localShadow()}`);
}
"#).unwrap();
    let members: Vec<_> = module.functions[0]
        .body
        .iter()
        .filter_map(|s| match s {
            hir::Stmt::Let { init, .. } => match &init.kind {
                hir::ExprKind::Lambda {
                    group,
                    captures,
                    owns_environment,
                    ..
                } => Some((
                    *group,
                    captures.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
                    *owns_environment,
                )),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(members.len(), 9);
    for index in [0, 1, 7, 8] {
        assert_eq!(members[index].0, None, "unlinked member {index}");
        assert!(
            !members[index].2,
            "unlinked member {index} owns its environment"
        );
    }
    assert!(members[2].0.is_some());
    assert_eq!(members[3].0, members[4].0);
    assert_eq!(members[5].0, members[6].0);
    assert!(members[3].0.is_some());
    assert!(members[5].0.is_some());
    assert_ne!(members[2].0, members[3].0);
    assert_ne!(members[3].0, members[5].0);
    for member in &members[..5] {
        assert_eq!(member.1, ["left"]);
        assert!(!member.2);
    }
    for member in &members[5..7] {
        assert_eq!(member.1, ["right"]);
        assert!(member.2);
    }
    assert!(members[7].1.is_empty());
    assert_eq!(members[8].1, ["left"]);
}

/// Earlier reads retain ordinary const captures, while a later forward link can join them.
#[test]
fn backward_reads_capture_consts_unless_forward_links_connect_the_members() {
    for (declarations, expected_grouped) in [
        ("const first = (): i32 => base; const second = (): i32 => first();", false),
        ("const first = (): i32 => third(); const second = (): i32 => { const value = first; return third(); }; const third = (): i32 => base;", true),
    ] {
        let module = check(&format!("export function main(): void {{ const base: i32 = 7; {declarations} print(`${{second()}}`); }}")).unwrap();
        let members: Vec<_> = module.functions[0].body.iter().filter_map(|s| match s {
            hir::Stmt::Let { init, .. } => match &init.kind {
                hir::ExprKind::Lambda { group, captures, .. } => Some((group, captures)),
                _ => None,
            },
            _ => None,
        }).collect();
        for (group, _) in &members {
            assert_eq!(group.is_some(), expected_grouped);
            assert_eq!(*group, members[0].0);
        }
        assert_eq!(members[1].1.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            if expected_grouped { vec!["base"] } else { vec!["first"] });
    }
}
