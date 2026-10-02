//! Argument inference for directly named generic functions (§149).

use super::*;
use crate::check::expr::unparen_expr;
use crate::check::instance_chain::ParameterEdge;

fn deferred_literal(expr: &ast::Expr) -> bool {
    match unparen_expr(expr) {
        ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::Str(_)) => true,
        ast::Expr::Unary(unary) if matches!(unary.op, ast::UnaryOp::Minus | ast::UnaryOp::Plus) => {
            matches!(unparen_expr(&unary.arg), ast::Expr::Lit(ast::Lit::Num(_)))
        }
        ast::Expr::Array(array) => array.elems.iter().all(|element| {
            element
                .as_ref()
                .is_some_and(|element| element.spread.is_none() && deferred_literal(&element.expr))
        }),
        _ => false,
    }
}

impl<'p> Checker<'p> {
    /// Checks each non-literal once and gives deferred literals the instance's context.
    pub(super) fn infer_call_arguments(
        &mut self,
        key: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<(InstanceArguments, Vec<Option<hir::Expr>>)> {
        let template = self.generic_fns.get(key)?.clone();
        if template.rejected {
            return None;
        }
        let annotations: Vec<_> = template
            .function
            .params
            .iter()
            .map(|param| match &param.pat {
                ast::Pat::Ident(binding) => binding.type_ann.as_ref().map(|ann| &*ann.type_ann),
                ast::Pat::Assign(assign) => match &*assign.left {
                    ast::Pat::Ident(binding) => binding.type_ann.as_ref().map(|ann| &*ann.type_ann),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let mut candidates = vec![Vec::new(); template.type_params.len()];
        let mut checked = vec![None; call.args.len()];
        for (index, argument) in call.args.iter().enumerate() {
            if argument.spread.is_some() || deferred_literal(&argument.expr) {
                continue;
            }
            let context = annotations
                .get(index)
                .copied()
                .flatten()
                .and_then(|annotation| {
                    if type_mentions_missing(
                        annotation,
                        &template.type_params,
                        &vec![false; candidates.len()],
                    ) {
                        None
                    } else {
                        let file = self.cur_file;
                        self.cur_file = template.file;
                        let context = self.resolve_type(annotation);
                        self.cur_file = file;
                        Some(context)
                    }
                });
            let value = self.check_expr(&argument.expr, context.as_ref(), fx);
            if value.ty == Type::Error {
                return None;
            }
            if let Some(Some(annotation)) = annotations.get(index) {
                let file = self.cur_file;
                self.cur_file = template.file;
                self.match_inference_type(
                    annotation,
                    &value.ty,
                    &template.type_params,
                    &mut candidates,
                );
                self.cur_file = file;
            }
            checked[index] = Some(value);
        }
        // Defaults contribute only where no non-literal supplies a candidate.
        let nonliteral: Vec<_> = candidates
            .iter()
            .map(|values| values.iter().any(|value| *value != Type::Null))
            .collect();
        for (index, argument) in call.args.iter().enumerate() {
            if argument.spread.is_some() || !deferred_literal(&argument.expr) {
                continue;
            }
            let Some(Some(annotation)) = annotations.get(index) else {
                continue;
            };
            if !type_mentions_missing(annotation, &template.type_params, &nonliteral) {
                continue;
            }
            if matches!(unparen_expr(&argument.expr), ast::Expr::Array(array) if array.elems.is_empty())
            {
                continue;
            }
            let diagnostics = self.diags.len();
            let value = self.check_expr(&argument.expr, None, fx);
            if value.ty == Type::Error || self.diags.len() != diagnostics {
                return None;
            }
            let mut defaults = vec![Vec::new(); candidates.len()];
            let file = self.cur_file;
            self.cur_file = template.file;
            self.match_inference_type(annotation, &value.ty, &template.type_params, &mut defaults);
            self.cur_file = file;
            for (index, values) in defaults.into_iter().enumerate() {
                if !nonliteral[index] {
                    candidates[index].extend(values);
                }
            }
        }
        let mut types = Vec::new();
        for (parameter, values) in template.type_params.iter().zip(candidates) {
            let has_null = values.contains(&Type::Null);
            let mut values = values.into_iter().filter(|value| *value != Type::Null);
            let Some(mut inferred) = values.next() else {
                self.error_diverging(RuleCode::S100, format!(
                    "cannot infer type parameter `{parameter}` of `{}`: no candidate; use explicit type arguments",
                    source_name(key)), pos.clone(), crate::divergence::Divergence::GenericInferenceMissing);
                return None;
            };
            for candidate in values {
                if candidate == inferred {
                    continue;
                }
                let joined = match (&inferred, &candidate) {
                    (Type::Nullable(inner), value) if inner.as_ref() == value => {
                        Some(inferred.clone())
                    }
                    (value, Type::Nullable(inner)) if inner.as_ref() == value => {
                        Some(candidate.clone())
                    }
                    _ => None,
                };
                let Some(joined) = joined else {
                    self.error_diverging(RuleCode::S100, format!(
                        "cannot infer type parameter `{parameter}` of `{}`: conflicting candidates `{}` and `{}`; use explicit type arguments",
                        source_name(key), self.type_name(&inferred), self.type_name(&candidate)), pos.clone(), crate::divergence::Divergence::GenericInferenceCandidates);
                    return None;
                };
                inferred = joined;
            }
            if has_null && !matches!(inferred, Type::Nullable(_)) && self.allows_nullable(&inferred)
            {
                inferred = Type::nullable(inferred);
            }
            types.push(inferred);
        }
        let site = (pos.file.clone(), pos.line, pos.col);
        let edges = if types.iter().any(|ty| self.involves_type_parameter(ty)) {
            let edges: Vec<_> = types
                .iter()
                .map(|ty| {
                    let mut edges = Vec::new();
                    self.inferred_parameter_edges(ty, false, &mut edges);
                    edges
                })
                .collect();
            self.inferred_edges.insert(site, edges.clone());
            edges
        } else {
            self.inferred_edges
                .get(&site)
                .cloned()
                .unwrap_or_else(|| vec![Vec::new(); types.len()])
        };
        Some((
            InstanceArguments {
                positions: vec![pos.clone(); types.len()],
                ends: vec![pos.clone(); types.len()],
                edges,
                types,
            },
            checked,
        ))
    }

    fn match_inference_type(
        &self,
        pattern: &ast::TsType,
        actual: &Type,
        parameters: &[String],
        candidates: &mut [Vec<Type>],
    ) {
        if *actual == Type::Error {
            return;
        }
        match pattern {
            ast::TsType::TsParenthesizedType(parenthesized) => {
                self.match_inference_type(&parenthesized.type_ann, actual, parameters, candidates);
            }
            ast::TsType::TsArrayType(array) => {
                if let Type::Array(element) = self.apparent_type(actual) {
                    self.match_inference_type(&array.elem_type, &element, parameters, candidates);
                }
            }
            ast::TsType::TsUnionOrIntersectionType(
                ast::TsUnionOrIntersectionType::TsUnionType(union),
            ) => {
                if *actual == Type::Null {
                    return;
                }
                let actual = if let Type::Nullable(inner) = actual {
                    inner.as_ref()
                } else {
                    actual
                };
                for ty in &union.types {
                    self.match_inference_type(ty, actual, parameters, candidates);
                }
            }
            ast::TsType::TsTypeRef(reference) => {
                let ast::TsEntityName::Ident(name) = &reference.type_name else {
                    return;
                };
                if let Some(index) = parameters
                    .iter()
                    .position(|parameter| parameter == name.sym.as_ref())
                {
                    candidates[index].push(actual.clone());
                    return;
                }
                let Some(arguments) = &reference.type_params else {
                    return;
                };
                let shape = self.apparent_type(actual);
                let components: Vec<&Type> = match (name.sym.as_ref(), &shape) {
                    ("FixedArray", Type::FixedArray(element, _) | Type::Array(element))
                    | ("Set", Type::Set(element)) => vec![element],
                    ("Map", Type::Map(key, value)) => vec![key, value],
                    (_, Type::Class(id)) => {
                        let Some((key, types)) = self.instance_arguments.get(id) else {
                            return;
                        };
                        if !matches!(self.type_scope_item(name.sym.as_ref()), Some(ScopeItem::GenericClass(expected)) if expected == *key)
                        {
                            return;
                        }
                        types.iter().collect()
                    }
                    _ => return,
                };
                for (pattern, actual) in arguments.params.iter().zip(components) {
                    self.match_inference_type(pattern, actual, parameters, candidates);
                }
            }
            ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function)) => {
                if let Type::Func(signature) = self.apparent_type(actual) {
                    for (parameter, actual) in function.params.iter().zip(&signature.params) {
                        let annotation = match parameter {
                            ast::TsFnParam::Ident(binding) => &binding.type_ann,
                            ast::TsFnParam::Array(array) => &array.type_ann,
                            ast::TsFnParam::Rest(rest) => &rest.type_ann,
                            ast::TsFnParam::Object(object) => &object.type_ann,
                        };
                        if let Some(annotation) = annotation {
                            self.match_inference_type(
                                &annotation.type_ann,
                                actual,
                                parameters,
                                candidates,
                            );
                        }
                    }
                    self.match_inference_type(
                        &function.type_ann.type_ann,
                        &signature.ret,
                        parameters,
                        candidates,
                    );
                }
            }
            _ => {}
        }
    }

    // Only symbolic types establish provenance. Equal concrete types establish no edge (§140).
    fn inferred_parameter_edges(&self, ty: &Type, expanding: bool, edges: &mut Vec<ParameterEdge>) {
        if let Some(source) = self.instance_chain.last().and_then(|request| {
            request.args.iter().position(|argument| {
                argument == ty
                    || matches!((argument, ty),
                    (Type::TypeParameter(a), Type::TypeParameter(b)) if a.identity == b.identity)
            })
        }) {
            if self.involves_type_parameter(ty) {
                edges.push(ParameterEdge { source, expanding });
                return;
            }
        }
        let components: Vec<&Type> = match ty {
            Type::Array(element)
            | Type::FixedArray(element, _)
            | Type::Set(element)
            | Type::Nullable(element) => vec![element],
            Type::Map(key, value) => vec![key, value],
            Type::Class(id) => self
                .instance_arguments
                .get(id)
                .map(|(_, types)| types.iter().collect())
                .unwrap_or_default(),
            Type::Func(function) => function
                .params
                .iter()
                .chain(std::iter::once(&function.ret))
                .collect(),
            _ => Vec::new(),
        };
        for component in components {
            self.inferred_parameter_edges(component, true, edges);
        }
    }
}

fn type_mentions_missing(ty: &ast::TsType, parameters: &[String], nonliteral: &[bool]) -> bool {
    match ty {
        ast::TsType::TsTypeRef(reference) => {
            matches!(&reference.type_name, ast::TsEntityName::Ident(name) if parameters.iter().enumerate().any(|(index, parameter)| !nonliteral[index] && parameter == name.sym.as_ref()))
                || reference.type_params.as_ref().is_some_and(|args| {
                    args.params
                        .iter()
                        .any(|ty| type_mentions_missing(ty, parameters, nonliteral))
                })
        }
        ast::TsType::TsArrayType(array) => {
            type_mentions_missing(&array.elem_type, parameters, nonliteral)
        }
        ast::TsType::TsParenthesizedType(parenthesized) => {
            type_mentions_missing(&parenthesized.type_ann, parameters, nonliteral)
        }
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => union
            .types
            .iter()
            .any(|ty| type_mentions_missing(ty, parameters, nonliteral)),
        ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function)) => {
            type_mentions_missing(&function.type_ann.type_ann, parameters, nonliteral)
                || function.params.iter().any(|parameter| {
                    let annotation = match parameter {
                        ast::TsFnParam::Ident(binding) => &binding.type_ann,
                        ast::TsFnParam::Array(array) => &array.type_ann,
                        ast::TsFnParam::Rest(rest) => &rest.type_ann,
                        ast::TsFnParam::Object(object) => &object.type_ann,
                    };
                    annotation.as_ref().is_some_and(|annotation| {
                        type_mentions_missing(&annotation.type_ann, parameters, nonliteral)
                    })
                })
        }
        _ => false,
    }
}
