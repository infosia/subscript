//! Loop heads and flow edges (§162, §315).
use super::stmt::root_of;
use super::{Checker, FactSet, FnCtx, Local, NarrowingFacts};
use crate::check::rejection::RejectionSite;
use crate::hir::{self, ExprKind};
use crate::types::Type;
use oxc_ast::ast;
use oxc_span::GetSpan;
use std::collections::HashSet;

impl<'p> Checker<'p> {
    fn begin_loop_transaction(&mut self) {
        self.class_ids.begin();
        self.fn_sigs.begin();
        self.instance_symbols.begin();
        self.global_sigs.begin();
        self.regex_literals.begin();
        self.deferred_captures.begin();
        self.earlier_member_reads.begin();
        self.inferred_edges.begin();
        self.instance_arguments.begin();
        self.classes.begin();
        self.class_sigs.begin();
        self.deferred_expressions.begin();
        self.pending_function_bodies.begin();
        self.pending_instance_bodies.begin();
        self.growing_cycles.begin();
        self.pending_instance_shapes.begin();
        self.result_dependencies.begin();
        self.type_only_value_uses.begin();
        self.static_method_owners.begin();
        self.local_function_classes.begin();
        self.type_argument_takes.begin();
        self.type_argument_user_calls.begin();
        self.operand_calls.begin();
        self.type_argument_watch.begin();
    }

    fn commit_loop_transaction(&mut self) {
        self.class_ids.commit();
        self.fn_sigs.commit();
        self.instance_symbols.commit();
        self.global_sigs.commit();
        self.regex_literals.commit();
        self.deferred_captures.commit();
        self.earlier_member_reads.commit();
        self.inferred_edges.commit();
        self.instance_arguments.commit();
        self.classes.commit();
        self.class_sigs.commit();
        self.deferred_expressions.commit();
        self.pending_function_bodies.commit();
        self.pending_instance_bodies.commit();
        self.growing_cycles.commit();
        self.pending_instance_shapes.commit();
        self.result_dependencies.commit();
        self.type_only_value_uses.commit();
        self.static_method_owners.commit();
        self.local_function_classes.commit();
        self.type_argument_takes.commit();
        self.type_argument_user_calls.commit();
        self.operand_calls.commit();
        self.type_argument_watch.commit();
    }

    fn rollback_loop_transaction(&mut self) {
        self.class_ids.rollback();
        self.fn_sigs.rollback();
        self.instance_symbols.rollback();
        self.global_sigs.rollback();
        self.regex_literals.rollback();
        self.deferred_captures.rollback();
        self.earlier_member_reads.rollback();
        self.inferred_edges.rollback();
        self.instance_arguments.rollback();
        self.classes.rollback();
        self.class_sigs.rollback();
        self.deferred_expressions.rollback();
        self.pending_function_bodies.rollback();
        self.pending_instance_bodies.rollback();
        self.growing_cycles.rollback();
        self.pending_instance_shapes.rollback();
        self.result_dependencies.rollback();
        self.type_only_value_uses.rollback();
        self.static_method_owners.rollback();
        self.local_function_classes.rollback();
        self.type_argument_takes.rollback();
        self.type_argument_user_calls.rollback();
        self.operand_calls.rollback();
        self.type_argument_watch.rollback();
    }

    /// Uses normal flow at each back edge for the induction (§315 rules 1–4).
    fn check_inductive_loop(
        &mut self,
        span: oxc_span::Span,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
        check: impl Fn(&mut Self, &mut FnCtx<'p>, &mut Vec<hir::Stmt>, &NarrowingFacts) -> bool,
    ) {
        let assumed: NarrowingFacts = fx
            .narrowed
            .iter()
            .filter(|fact| {
                fact.narrows_type()
                    && !fact.shared
                    && !fact.contains('.')
                    && fx
                        .scopes
                        .iter()
                        .rev()
                        .any(|scope| scope.vars.contains_key(&fact.key))
            })
            .cloned()
            .collect();
        if self.narrowing_analysis.is_none()
            || assumed.is_empty()
            || fx.failed_loop_assumptions.borrow().contains(&span)
        {
            Self::restore_loop_finally_causes(span, fx);
            check(self, fx, out, &Default::default());
            return;
        }
        let context = fx.clone();
        let output = out.len();
        let diagnostics = self.diags.len();
        self.begin_loop_transaction();
        let ranges = self.instance_diagnostic_ranges.len();
        let reports = self.growth_reports.len();
        let accesses = self.diagnosed_class_accesses.len();
        let lengths = (
            self.functions.len(),
            self.type_handle_classes.len(),
            self.pending_layouts.len(),
            self.globals.len(),
            self.worker_entries.len(),
            self.top_level.len(),
        );
        let counters = (
            self.next_for_of_id,
            self.next_lambda_id,
            self.next_lambda_ordinal,
            self.next_regex_literal_id,
            self.next_using_switch_id,
            self.next_compound_local_id,
            self.next_pattern_id,
            self.next_deferred_id,
        );
        if check(self, fx, out, &assumed) {
            self.commit_loop_transaction();
            return;
        }
        fx.failed_loop_assumptions.borrow_mut().insert(span);
        for function in &self.functions[lengths.0..] {
            self.narrowing_helper_symbols.remove(&function.symbol);
        }
        for id in self.classes.transaction_len()..self.classes.len() {
            let id = crate::types::ClassId(id);
            self.handle_classes.remove(&id);
            self.declared_classes.remove(&id);
            self.boundary_classes.remove(&id);
            self.opaque_instances.remove(&id);
        }
        self.rollback_loop_transaction();
        self.functions.truncate(lengths.0);
        self.type_handle_classes.truncate(lengths.1);
        self.pending_layouts.truncate(lengths.2);
        self.globals.truncate(lengths.3);
        self.worker_entries.truncate(lengths.4);
        self.top_level.truncate(lengths.5);
        self.instance_diagnostic_ranges.truncate(ranges);
        self.growth_reports.truncate(reports);
        self.diagnosed_class_accesses.truncate(accesses);
        (
            self.next_for_of_id,
            self.next_lambda_id,
            self.next_lambda_ordinal,
            self.next_regex_literal_id,
            self.next_using_switch_id,
            self.next_compound_local_id,
            self.next_pattern_id,
            self.next_deferred_id,
        ) = counters;
        self.diags.0.borrow_mut().truncate(diagnostics);
        *fx = context;
        out.truncate(output);
        Self::restore_loop_finally_causes(span, fx);
        check(self, fx, out, &Default::default());
    }

    fn restore_loop_finally_causes(span: oxc_span::Span, fx: &mut FnCtx) {
        let back = fx.failed_loop_finally_causes.borrow().get(&span).cloned();
        if let Some(back) = back {
            fx.merge_finally_causes(&back.finally_ended, &back.facts);
        }
    }

    fn loop_back_edges_hold(
        span: oxc_span::Span,
        fx: &mut FnCtx<'p>,
        assumed: &NarrowingFacts,
        falls_through: bool,
    ) -> bool {
        let mut edges = fx
            .loop_break_facts
            .last()
            .map_or_else(Vec::new, |(_, _, edges)| edges.clone());
        if falls_through {
            edges.push(super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            });
        }
        Self::record_loop_back_edge(span, fx, edges);
        assumed.iter().all(|fact| {
            (!falls_through || fx.narrowed.contains(fact))
                && fx
                    .loop_break_facts
                    .last()
                    .is_none_or(|(_, _, edges)| edges.iter().all(|edge| edge.facts.contains(fact)))
        })
    }

    fn record_loop_back_edge(
        span: oxc_span::Span,
        fx: &mut FnCtx<'p>,
        edges: Vec<super::LoopEdge>,
    ) {
        let Some(first) = edges.first() else {
            return;
        };
        let mut facts = first.facts.clone();
        for edge in edges.iter().skip(1) {
            facts.retain(|fact| edge.facts.contains(fact));
        }
        let finally_ended = FnCtx::joined_finally_causes(
            edges.iter().map(|edge| (&edge.facts, &edge.finally_ended)),
        );
        fx.failed_loop_finally_causes.borrow_mut().insert(
            span,
            super::LoopEdge {
                facts,
                notes: Default::default(),
                finally_ended,
            },
        );
    }

    pub(super) fn record_loop_edge(&self, fx: &mut FnCtx<'p>, is_continue: bool) {
        if !fx.flow_reachable {
            return;
        }
        if !fx.has_narrowing_facts() {
            if let Some((_, breaks, continues)) = fx.loop_break_facts.last_mut() {
                let edge = super::LoopEdge {
                    facts: Default::default(),
                    notes: Default::default(),
                    finally_ended: Default::default(),
                };
                if is_continue {
                    continues.push(edge);
                } else {
                    breaks.push(edge);
                }
            }
            return;
        }
        if let Some((scope_depth, _, _)) = fx.loop_break_facts.last() {
            let edge = Self::flow_edge_leaving_scopes(fx, *scope_depth);
            if let Some((_, breaks, continues)) = fx.loop_break_facts.last_mut() {
                if is_continue {
                    continues.push(edge);
                } else {
                    breaks.push(edge);
                }
            }
        }
    }

    pub(super) fn flow_edge_leaving_scopes(fx: &FnCtx<'p>, scope_depth: usize) -> super::LoopEdge {
        let mut edge = super::LoopEdge {
            facts: fx.narrowed.clone(),
            notes: fx.ended_shared_narrowing.clone(),
            finally_ended: fx.finally_ended_narrowing.clone(),
        };
        edge.finally_ended
            .retain(|(_, scope)| scope.is_none_or(|depth| depth < scope_depth));
        let mut disposed = false;
        for scope in fx.scopes[scope_depth..].iter().rev() {
            disposed |= scope.dispose_on_exit;
            if scope.dispose_on_exit
                && edge
                    .facts
                    .iter()
                    .any(|key| key.shared && key.narrows_type())
            {
                edge.facts.retain(|key| {
                    if key.shared && key.narrows_type() {
                        edge.notes.insert_fact(key.clone());
                        false
                    } else {
                        true
                    }
                });
            }
            if !scope.vars.is_empty()
                && edge.facts.iter().any(|key| {
                    scope.vars.contains_key(root_of(key))
                        || key.leaves_const_scope(&scope.const_keys)
                })
            {
                edge.facts.retain(|key| {
                    !scope.vars.contains_key(root_of(key))
                        && !key.leaves_const_scope(&scope.const_keys)
                });
            }
            if !scope.shadowed_narrowing.is_empty() {
                let mut facts = scope.shadowed_narrowing.clone();
                if disposed && facts.iter().any(|key| key.shared && key.narrows_type()) {
                    facts.retain(|key| {
                        if key.shared && key.narrows_type() {
                            edge.notes.insert_fact(key.clone());
                            false
                        } else {
                            true
                        }
                    });
                }
                edge.facts.extend_facts(facts);
            }
            if !scope.shadowed_ended_shared.is_empty() {
                edge.notes.extend_facts(scope.shadowed_ended_shared.clone());
            }
        }
        edge
    }

    fn join_loop_exits(fx: &mut FnCtx<'p>, false_exit: Option<super::LoopEdge>) {
        fx.loop_ended_narrowing.pop();
        let mut exits = fx
            .loop_break_facts
            .pop()
            .map_or_else(Vec::new, |(_, edges, _)| edges);
        exits.extend(false_exit);
        Self::join_flow_edges(fx, exits);
    }

    pub(super) fn join_flow_edges(fx: &mut FnCtx<'p>, mut edges: Vec<super::LoopEdge>) {
        fx.finally_ended_narrowing = FnCtx::joined_finally_causes(
            edges.iter().map(|edge| (&edge.facts, &edge.finally_ended)),
        );
        if edges.iter().all(|edge| edge.notes.is_empty()) {
            fx.narrowed = edges.pop().map_or_else(Default::default, |edge| edge.facts);
            fx.ended_shared_narrowing = Default::default();
            for edge in edges {
                if !std::ptr::eq(&*fx.narrowed, &*edge.facts) && *fx.narrowed != *edge.facts {
                    fx.narrowed.retain(|key| edge.facts.contains(key));
                }
            }
            return;
        }
        let first = edges.pop();
        let mut eligible: HashSet<_> = first.as_ref().map_or_else(HashSet::new, |edge| {
            edge.facts.union(&edge.notes).cloned().collect()
        });
        fx.narrowed = first
            .as_ref()
            .map_or_else(Default::default, |edge| edge.facts.clone());
        fx.ended_shared_narrowing = first.map_or_else(Default::default, |edge| edge.notes);
        for edge in edges {
            eligible.retain(|key| edge.facts.contains(key) || edge.notes.contains(key));
            fx.narrowed.retain(|key| edge.facts.contains(key));
            fx.ended_shared_narrowing.extend_facts(edge.notes);
        }
        fx.finish_narrowing_join(&eligible);
    }

    pub(super) fn check_while(
        &mut self,
        w: &ast::WhileStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        self.check_inductive_loop(w.span, fx, out, |checker, fx, out, assumed| {
            checker.check_while_pass(w, fx, out, assumed)
        });
    }

    fn check_while_pass(
        &mut self,
        w: &ast::WhileStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
        assumed: &super::NarrowingFacts,
    ) -> bool {
        #[cfg(test)]
        super::body_check_cost::record_loop(w.span);
        let pos = self.pos(w.span);
        let assumed: NarrowingFacts = assumed
            .iter()
            .filter(|fact| fx.narrowed.contains(*fact))
            .cloned()
            .collect();
        self.end_loop_narrowing(&pos, fx);
        fx.narrowed.extend_facts(assumed.clone());
        let mut kept = fx.narrowed.clone();
        kept.retain(|fact| !assumed.contains(fact));

        let cond = self.check_truth_expr(&w.test, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);
        let mut false_exit = fx.narrowed.clone();
        if !then_extra.is_empty() {
            false_exit.retain(|key| !then_extra.contains(key));
        }
        if !else_extra.is_empty() {
            false_exit.extend_facts(else_extra);
        }
        let false_exit = (!matches!(cond.kind, ExprKind::Bool(true))).then_some(super::LoopEdge {
            facts: false_exit,
            notes: fx.ended_shared_narrowing.clone(),
            finally_ended: fx.finally_ended_narrowing.clone(),
        });
        if !then_extra.is_empty() {
            fx.narrowed.extend_facts(then_extra.clone());
            fx.replace_finally_causes_at_facts();
        }
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len(), Vec::new(), Vec::new()));
        let (body, terminates) = self.check_branch(&w.body, fx);
        let proven = Self::loop_back_edges_hold(w.span, fx, &assumed, !terminates);
        self.check_loop_stores(&body, std::iter::once(&cond), &kept);
        fx.loop_depth -= 1;
        Self::join_loop_exits(fx, false_exit);

        out.push(hir::Stmt::While { cond, body, pos });
        proven
    }

    /// Checks `do { body } while (test);` (`compiler.md` §203): the body
    /// runs first, and the test sees the join of the body end and each
    /// `continue`.
    pub(super) fn check_do_while(
        &mut self,
        d: &ast::DoWhileStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        let pos = self.pos(d.span);
        let before = fx.narrowed.clone();
        self.end_loop_narrowing(&pos, fx);
        let kept = fx.narrowed.clone();
        self.enter_do_while_body(&pos, &before, fx);
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len(), Vec::new(), Vec::new()));
        let (body, terminates) = self.check_branch(&d.body, fx);
        let mut test_edges = fx
            .loop_break_facts
            .last()
            .map_or_else(Vec::new, |(_, _, edges)| edges.clone());
        if !terminates {
            test_edges.push(super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            });
        }
        Self::join_flow_edges(fx, test_edges);
        let cond = self.check_truth_expr(&d.test, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);
        let mut false_exit = fx.narrowed.clone();
        if !then_extra.is_empty() {
            false_exit.retain(|key| !then_extra.contains(key));
        }
        if !else_extra.is_empty() {
            false_exit.extend_facts(else_extra);
        }
        let false_exit = (!matches!(cond.kind, ExprKind::Bool(true))).then_some(super::LoopEdge {
            facts: false_exit,
            notes: fx.ended_shared_narrowing.clone(),
            finally_ended: fx.finally_ended_narrowing.clone(),
        });
        self.check_loop_stores(&body, std::iter::once(&cond), &kept);
        fx.loop_depth -= 1;
        Self::join_loop_exits(fx, false_exit);

        out.push(hir::Stmt::DoWhile { body, cond, pos });
    }

    pub(super) fn check_for(
        &mut self,
        f: &ast::ForStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        fx.scopes.push(Default::default());
        let init = match &f.init {
            Some(ast::ForStatementInit::VariableDeclaration(v)) => {
                let (init_out, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForInit(self.pos(v.span)),
                    |fx| {
                        let mut init_out = Vec::new();
                        self.check_let(v, fx, &mut init_out);
                        init_out
                    },
                );
                let mut statements = prefix.into_statements();
                statements.extend(init_out);
                let mut init_out = statements;
                let init = init_out.pop().map(Box::new);
                out.extend(init_out);
                init
            }
            Some(e) => {
                let (checked, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForInit(self.pos(e.span())),
                    |fx| self.check_expr(e.to_expression(), None, fx),
                );
                out.extend(prefix);
                Some(Box::new(hir::Stmt::Expr(checked)))
            }
            None => None,
        };

        self.check_inductive_loop(f.span, fx, out, |checker, fx, out, assumed| {
            checker.check_for_pass(f, fx, out, assumed, init.clone())
        });
        fx.pop_scope();
    }

    fn check_for_pass(
        &mut self,
        f: &ast::ForStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
        assumed: &super::NarrowingFacts,
        init: Option<Box<hir::Stmt>>,
    ) -> bool {
        #[cfg(test)]
        super::body_check_cost::record_loop(f.span);
        let pos = self.pos(f.span);
        let assumed: NarrowingFacts = assumed
            .iter()
            .filter(|fact| fx.narrowed.contains(*fact))
            .cloned()
            .collect();
        self.end_loop_narrowing(&pos, fx);
        fx.narrowed.extend_facts(assumed.clone());
        let mut kept = fx.narrowed.clone();
        kept.retain(|fact| !assumed.contains(fact));

        let (cond, cond_prefix) = match &f.test {
            Some(test) => {
                let (checked, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForCond(self.pos(test.span())),
                    |fx| {
                        let checked = self.check_truth_expr(test, fx);
                        self.require_bool(&checked);
                        checked
                    },
                );
                (Some(checked), prefix)
            }
            None => (None, super::SyntheticPrefix::default()),
        };
        let (then_extra, else_extra) = cond
            .as_ref()
            .map(|c| self.narrowing_paths(c, fx))
            .unwrap_or_default();
        let mut false_exit = fx.narrowed.clone();
        if !then_extra.is_empty() {
            false_exit.retain(|key| !then_extra.contains(key));
        }
        if !else_extra.is_empty() {
            false_exit.extend_facts(else_extra);
        }
        let false_exit = cond
            .as_ref()
            .is_some_and(|c| !matches!(c.kind, ExprKind::Bool(true)))
            .then_some(super::LoopEdge {
                facts: false_exit,
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            });
        if !then_extra.is_empty() {
            fx.narrowed.extend_facts(then_extra);
            fx.replace_finally_causes_at_facts();
        }
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len(), Vec::new(), Vec::new()));
        let (body, terminates) = self.check_branch(&f.body, fx);
        let mut update_edges = fx
            .loop_break_facts
            .last()
            .map_or_else(Vec::new, |(_, _, edges)| edges.clone());
        if !terminates {
            update_edges.push(super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            });
        }
        let has_back_edge = !update_edges.is_empty();
        Self::join_flow_edges(fx, update_edges);
        let step_statements = f.update.as_ref().map(|update| {
            let (statements, prefix) = fx.with_synthetic_owner(
                super::SyntheticOwnerKind::ForUpdate(self.pos(update.span())),
                |fx| self.check_expr_stmt(update, fx),
            );
            let mut step = prefix.into_statements();
            step.extend(statements);
            step
        });
        Self::record_loop_back_edge(
            f.span,
            fx,
            if has_back_edge {
                vec![super::LoopEdge {
                    facts: fx.narrowed.clone(),
                    notes: fx.ended_shared_narrowing.clone(),
                    finally_ended: fx.finally_ended_narrowing.clone(),
                }]
            } else {
                Vec::new()
            },
        );
        let proven = !has_back_edge || assumed.iter().all(|fact| fx.narrowed.contains(fact));
        self.check_loop_stores(&body, cond.iter(), &kept);
        if !cond_prefix.is_empty() {
            self.check_loop_stores(&cond_prefix.0, std::iter::empty(), &kept);
        }
        if let Some(step) = &step_statements {
            self.check_loop_stores(step, std::iter::empty(), &kept);
        }
        fx.loop_depth -= 1;
        Self::join_loop_exits(fx, false_exit);

        let step = step_statements.unwrap_or_default();
        if cond_prefix.is_empty() {
            out.push(hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            });
            return proven;
        }

        let cond = cond.unwrap_or_else(|| hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Bool(true),
            ty: Type::Bool,
            pos: pos.clone(),
        });
        let (cond, body) = if cond_prefix.is_empty() {
            (cond, body)
        } else {
            let mut guarded = cond_prefix.into_statements();
            guarded.push(hir::Stmt::If {
                cond,
                then: body,
                els: Some(vec![hir::Stmt::Break(pos.clone())]),
                pos: pos.clone(),
            });
            (
                hir::Expr {
                    pending_work: None,
                    kind: hir::ExprKind::Bool(true),
                    ty: Type::Bool,
                    pos: pos.clone(),
                },
                guarded,
            )
        };
        let mut block = Vec::new();
        if let Some(init) = init {
            block.push(*init);
        }
        block.push(hir::Stmt::For {
            init: None,
            cond: Some(cond),
            step,
            body,
            pos,
        });
        out.push(hir::Stmt::Block(block));
        proven
    }

    /// Checks and binds the closed `for…of` surface (stdlib.md §14).
    /// Container views are recognized here, before ordinary call
    /// checking, because `keys()` / `values()` have no value type outside
    /// this exact subject position.
    pub(super) fn check_for_of(
        &mut self,
        f: &ast::ForOfStatement,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        let pos = self.pos(f.span);
        if f.r#await {
            self.reject_subset(RejectionSite::AsyncForOf, "`for await…of` requires the Promise object/iterator surface, which is not in the language", pos);
            return;
        }

        let Some((pattern, mutable, binding_pos, annotation)) = self.for_of_binding(&f.left) else {
            return;
        };
        // §107.1: a rejected subject keeps its own rule, so the subject
        // runs before the pattern reports.
        let (subject, kind, elem_ty, generator) = self.check_for_of_subject(&f.right, fx);
        if let super::pattern::Pattern::Rejected(rejection) = &pattern {
            // The names bind inside the loop, which this rejection skips.
            fx.scopes.push(Default::default());
            self.reject_pattern(rejection, fx);
            fx.pop_scope();
            return;
        }
        if matches!(self.apparent_type(&subject.ty), Type::Error)
            || matches!(self.apparent_type(&elem_ty), Type::Error)
        {
            return;
        }
        if let Some(annotation) = annotation {
            self.require_assignable(
                &elem_ty,
                &annotation,
                binding_pos.clone(),
                "the `for…of` binding",
            );
            self.require_assignable(
                &annotation,
                &elem_ty,
                binding_pos.clone(),
                "the `for…of` binding",
            );
        }

        self.check_inductive_loop(f.span, fx, out, |checker, fx, out, assumed| {
            #[cfg(test)]
            super::body_check_cost::record_loop(f.span);
            let pos = pos.clone();
            let subject = subject.clone();
            let elem_ty = elem_ty.clone();
            let assumed: NarrowingFacts = assumed
                .iter()
                .filter(|fact| fx.narrowed.contains(*fact))
                .cloned()
                .collect();
            checker.end_loop_narrowing(&pos, fx);
            fx.narrowed.extend_facts(assumed.clone());

            let base = super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            };
            let binding_async_origins = checker.expr_async_origins(&subject, fx);
            fx.scopes.push(Default::default());
            // A pattern binds the element into checker-generated storage and
            // reads its names out of that (§107.2).
            let name = match &pattern {
                super::pattern::Pattern::Name(binding) => binding.name.to_string(),
                _ => {
                    let id = checker.next_pattern_id;
                    checker.next_pattern_id += 1;
                    format!("[[pattern#{id}.element]]")
                }
            };
            let mut prologue = Vec::new();
            if pattern.is_destructuring() {
                checker.declare_pattern_storage(&name, &elem_ty, binding_async_origins, fx);
                let element = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Local(name.clone(), elem_ty.clone(), false),
                    ty: elem_ty.clone(),
                    pos: binding_pos.clone(),
                };
                checker.bind_pattern_from(&pattern, &element, mutable, false, fx, &mut prologue);
            } else {
                checker.declare_local(
                    &name,
                    Local {
                        annotated: false,
                        ty: elem_ty.clone(),
                        mutable,
                        async_origins: binding_async_origins,
                        caught: false,
                        function_value_required: None,
                    },
                    binding_pos.clone(),
                    fx,
                );
            }
            let prefix = format!("{name}.");
            fx.narrowed
                .retain(|key| key != &name && !key.starts_with(&prefix));
            let mut kept = fx.narrowed.clone();
            kept.retain(|fact| !assumed.contains(fact));
            fx.loop_depth += 1;
            fx.loop_break_facts
                .push((fx.scopes.len() - 1, Vec::new(), Vec::new()));
            let (body, terminates) = checker.check_branch(&f.body, fx);
            let proven = Self::loop_back_edges_hold(f.span, fx, &assumed, !terminates);
            checker.check_loop_stores(&body, std::iter::empty(), &kept);
            fx.loop_depth -= 1;
            fx.pop_scope();
            Self::join_loop_exits(fx, Some(base));
            let body = if prologue.is_empty() {
                body
            } else {
                prologue.extend(body);
                prologue
            };

            if generator {
                out.push(hir::Stmt::GeneratorForOf {
                    name,
                    ty: elem_ty,
                    mutable,
                    subject,
                    body,
                    pos,
                });
                return proven;
            }

            let Some(kind) = kind else {
                checker.reject_subset(
                    RejectionSite::ForOfSubject,
                    "the for-of subject has no iteration kind",
                    pos,
                );
                return proven;
            };
            // A `FixedArray` traversal reads its place (compiler.md §220.1 rule 1),
            // so the subject stays an expression and no local copies it.
            if kind == hir::ForOfKind::FixedArrayValues {
                out.push(hir::Stmt::ForOf {
                    name,
                    ty: elem_ty,
                    subject,
                    kind,
                    body,
                    pos,
                });
                return proven;
            }
            let id = checker.next_for_of_id;
            checker.next_for_of_id += 1;
            let subject_name = format!("[[for.of#{id}.subject]]");
            let subject_ty = subject.ty.clone();
            let subject_local = hir::Expr {
                pending_work: None,
                kind: ExprKind::Local(subject_name.clone(), subject_ty.clone(), false),
                ty: subject_ty.clone(),
                pos: subject.pos.clone(),
            };
            let subject_let = hir::Stmt::Let {
                name: subject_name.clone(),
                ty: subject_ty,
                mutable: false,
                dispose: false,
                init: subject,
                pos: pos.clone(),
            };

            let loop_stmt = hir::Stmt::ForOf {
                name,
                ty: elem_ty,
                subject: subject_local,
                kind,
                body,
                pos: pos.clone(),
            };
            out.push(hir::Stmt::Block(vec![subject_let, loop_stmt]));
            proven
        });
    }
}
