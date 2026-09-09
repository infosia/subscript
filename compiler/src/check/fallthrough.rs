//! Statement exit analysis for scope-exit disposal (compiler.md §101).

use crate::hir::{AmbientFn, Callee, ExprKind, Stmt};

#[derive(Clone, Copy)]
struct Exits {
    next: bool,
    breaks: bool,
}

impl Exits {
    const NEXT: Self = Self {
        next: true,
        breaks: false,
    };
    const STOP: Self = Self {
        next: false,
        breaks: false,
    };

    fn followed_by(self, tail: Self) -> Self {
        Self {
            next: self.next && tail.next,
            breaks: self.breaks || (self.next && tail.breaks),
        }
    }

    fn either(self, other: Self) -> Self {
        Self {
            next: self.next || other.next,
            breaks: self.breaks || other.breaks,
        }
    }
}

/// True when a statement has a normal exit, separate from return, break, or continue.
pub(super) fn can_fall_through(statement: &Stmt) -> bool {
    exits(statement).next
}

pub(super) fn sequence_can_fall_through(statements: &[Stmt]) -> bool {
    sequence_exits(statements).next
}

fn sequence_exits(statements: &[Stmt]) -> Exits {
    let mut result = Exits::NEXT;
    for statement in statements {
        if !result.next {
            break;
        }
        result = result.followed_by(exits(statement));
    }
    result
}

fn loop_exits(condition: Option<&crate::hir::Expr>, body: &[Stmt]) -> Exits {
    Exits {
        next: condition.is_some_and(|cond| !matches!(cond.kind, ExprKind::Bool(true)))
            || sequence_exits(body).breaks,
        breaks: false,
    }
}

fn exits(statement: &Stmt) -> Exits {
    match statement {
        Stmt::Let { .. } | Stmt::ForOf { .. } => Exits::NEXT,
        Stmt::Return { .. } | Stmt::Continue(_) => Exits::STOP,
        Stmt::Break(_) => Exits {
            next: false,
            breaks: true,
        },
        Stmt::Expr(expr) => match &expr.kind {
            ExprKind::Call {
                callee: Callee::Ambient(AmbientFn::Unreachable),
                ..
            } => Exits::STOP,
            _ => Exits::NEXT,
        },
        Stmt::Block(body) => sequence_exits(body),
        Stmt::If {
            cond, then, els, ..
        } => {
            let then = sequence_exits(then);
            let els = els.as_deref().map_or(Exits::NEXT, sequence_exits);
            match cond.kind {
                ExprKind::Bool(true) => then,
                ExprKind::Bool(false) => els,
                _ => then.either(els),
            }
        }
        Stmt::While { cond, body, .. } => loop_exits(Some(cond), body),
        Stmt::For {
            init, cond, body, ..
        } => init
            .as_deref()
            .map_or(Exits::NEXT, exits)
            .followed_by(loop_exits(cond.as_ref(), body)),
        Stmt::Switch { cases, .. } => {
            let mut result = if cases.iter().any(|case| case.test.is_none()) {
                Exits::STOP
            } else {
                Exits::NEXT
            };
            let mut suffix = Exits::NEXT;
            for case in cases.iter().rev() {
                suffix = sequence_exits(&case.body).followed_by(suffix);
                result = result.either(suffix);
            }
            Exits {
                next: result.next || result.breaks,
                breaks: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_program, hir, SourceFile};

    fn body(source: &str) -> Vec<Stmt> {
        let source =
            format!("function probe(flag: boolean, n: i32, values: i32[]): void {{ {source} }}");
        let module = check_program(&[SourceFile::new("fallthrough.ts", source)])
            .expect("accepted statement fixture");
        module.functions[0].body.clone()
    }

    #[test]
    fn every_statement_kind_has_its_admitted_exits() {
        let cases = [
            ("let local: i32 = 1;", true),
            ("print(\"next\");", true),
            ("unreachable();", false),
            ("return;", false),
            ("{}", true),
            ("{ return; }", false),
            ("if (flag) { return; }", true),
            ("if (flag) { return; } else { return; }", false),
            ("if (true) { return; }", false),
            ("if (false) { return; }", true),
            ("if (false) {} else { return; }", false),
            ("while (flag) { return; }", true),
            ("while (false) { return; }", true),
            ("while (true) { return; }", false),
            ("while (true) {}", false),
            ("while (true) { break; }", true),
            ("for (; flag;) { return; }", true),
            ("for (; false;) { return; }", true),
            ("for (;;) { return; }", false),
            ("for (; true;) {}", false),
            ("for (;;) { break; }", true),
            ("for (let i: i32 = 0;; i += 1) { return; }", false),
            ("switch (n) { case 0: return; }", true),
            ("switch (n) { case 0: return; default: return; }", false),
            ("switch (n) { case 0: default: return; }", false),
            ("switch (n) { default: case 0: return; }", false),
            ("switch (n) { default: return; case 0: break; }", true),
            ("switch (n) { default: if (flag) { break; } return; }", true),
            ("switch (n) { default: {} }", true),
        ];
        for (source, expected) in cases {
            let statements = body(source);
            assert_eq!(statements.len(), 1, "{source}");
            assert_eq!(can_fall_through(&statements[0]), expected, "{source}");
        }
        for source in ["while (true) { break; }", "while (true) { continue; }"] {
            let statements = body(source);
            let Stmt::While { body, .. } = &statements[0] else {
                panic!("while fixture");
            };
            assert!(!can_fall_through(&body[0]), "{source}");
        }
        // A fused traversal admits an empty container, even when its body stops.
        for source in [
            "for (const v of values) {}",
            "for (const v of values) { return; }",
        ] {
            let statements = body(source);
            let Stmt::Block(block) = &statements[0] else {
                panic!("for-of subject scope");
            };
            let statement = block.last().expect("for-of loop");
            assert!(matches!(statement, Stmt::ForOf { .. }));
            assert!(can_fall_through(statement));
        }
    }

    #[test]
    fn only_reachable_breaks_leave_their_own_loop() {
        let cases = [
            ("if (flag) { break; }", true),
            ("{ break; }", true),
            ("if (false) { break; }", false),
            ("if (true) { continue; } break;", false),
            ("return; break;", false),
            ("unreachable(); break;", false),
            ("continue; break;", false),
            ("while (true) { break; }", false),
            ("for (;;) { break; }", false),
            ("for (const v of values) { break; }", false),
            ("switch (n) { default: break; }", false),
            ("switch (n) { default: break; } break;", true),
            ("switch (n) { default: continue; } break;", false),
            (
                "switch (n) { default: return; case 1: break; } break;",
                true,
            ),
            ("{ for (;;) {} } break;", false),
            ("if (flag) { continue; } else { break; }", true),
        ];
        for (inner, expected) in cases {
            for loop_head in ["for (;;)", "while (true)"] {
                let source = format!("{loop_head} {{ {inner} }}");
                let statements = body(&source);
                assert_eq!(can_fall_through(&statements[0]), expected, "{source}");
            }
        }
    }

    #[test]
    fn sequence_stops_before_a_later_leavable_statement() {
        let statements = body("for (;;) {} print(\"later\");");
        assert_eq!(statements.len(), 2);
        assert!(can_fall_through(&statements[1]));
        assert!(!sequence_can_fall_through(&statements));
        assert!(sequence_can_fall_through(&[]));
    }

    #[test]
    fn for_initializer_exits_compose_before_the_loop() {
        let mut statements = body("for (;;) { break; }");
        let Stmt::For { init, .. } = &mut statements[0] else {
            panic!("for fixture");
        };
        *init = Some(Box::new(hir::Stmt::Return {
            value: None,
            pos: crate::Pos::new("fallthrough.ts", 1, 1),
        }));
        assert!(!can_fall_through(&statements[0]));
    }
}
