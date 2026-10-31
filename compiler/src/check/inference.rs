//! Argument inference for generic calls and constructions (§149, §197, §212).

use super::*;
use crate::check::expr::unparen_expr;
use crate::check::instance_chain::ParameterEdge;
use crate::check::rejection::RejectionSite;

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

fn pattern_annotation(pattern: &ast::Pat) -> Option<&ast::TsType> {
    match pattern {
        ast::Pat::Ident(binding) => binding.type_ann.as_ref().map(|ann| &*ann.type_ann),
        ast::Pat::Assign(assign) => match &*assign.left {
            ast::Pat::Ident(binding) => binding.type_ann.as_ref().map(|ann| &*ann.type_ann),
            _ => None,
        },
        _ => None,
    }
}

/// The declared parameters that one inference reads (§149 rule 2, §197 rule 2).
struct InferenceTarget<'a> {
    file: usize,
    type_params: &'a [String],
    /// The annotation of each declared parameter, in order.
    annotations: Vec<Option<&'a ast::TsType>>,
    /// The declared result annotation of a function or a method (§212).
    result: Option<&'a ast::TsType>,
    /// The count of parameters up to the last one with no default.
    required: usize,
    /// The name that a rejection text gives the callee.
    name: String,
}

impl<'a> InferenceTarget<'a> {
    fn function(
        file: usize,
        type_params: &'a [String],
        function: &'a ast::Function,
        name: String,
    ) -> Self {
        Self {
            file,
            type_params,
            annotations: function
                .params
                .iter()
                .map(|param| pattern_annotation(&param.pat))
                .collect(),
            result: function
                .return_type
                .as_ref()
                .map(|annotation| &*annotation.type_ann),
            required: function
                .params
                .iter()
                .rposition(|param| !matches!(param.pat, ast::Pat::Assign(_)))
                .map_or(0, |index| index + 1),
            name,
        }
    }

    /// The shape keeps the last constructor, and its plain parameters, as
    /// the constructor signature does. A class with no constructor has no
    /// parameter, so no argument gives a candidate (§197 rule 2).
    fn constructor(template: &'a GenericClass, name: String) -> Self {
        let mut parameters: Vec<(Option<&'a ast::TsType>, bool)> = Vec::new();
        for member in &template.class.body {
            let ast::ClassMember::Constructor(constructor) = member else {
                continue;
            };
            parameters = constructor
                .params
                .iter()
                .filter_map(|parameter| match parameter {
                    ast::ParamOrTsParamProp::Param(parameter) => Some((
                        pattern_annotation(&parameter.pat),
                        matches!(parameter.pat, ast::Pat::Assign(_)),
                    )),
                    ast::ParamOrTsParamProp::TsParamProp(_) => None,
                })
                .collect();
        }
        Self {
            file: template.file,
            type_params: &template.type_params,
            result: None,
            required: parameters
                .iter()
                .rposition(|(_, default)| !default)
                .map_or(0, |index| index + 1),
            annotations: parameters
                .into_iter()
                .map(|(annotation, _)| annotation)
                .collect(),
            name,
        }
    }
}

/// The inferred type arguments, and each argument that inference checked.
type Inferred = (InstanceArguments, Vec<Option<hir::Expr>>);

impl<'p> Checker<'p> {
    /// Infers the type arguments of a directly named generic function call (§149).
    pub(super) fn infer_call_arguments(
        &mut self,
        key: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<Inferred> {
        let template = self.generic_fns.get(key)?.clone();
        if template.rejected {
            return None;
        }
        let target = InferenceTarget::function(
            template.file,
            &template.type_params,
            &template.function,
            source_name(key),
        );
        let contextual = self.contextual_result_candidates(&target, call);
        self.infer_arguments(&target, &call.args, &contextual, fx, pos)
    }

    /// Infers the type arguments of a generic method call (§197 rule 1,
    /// §212).
    pub(super) fn infer_method_arguments(
        &mut self,
        class: ClassId,
        name: &str,
        call: &ast::CallExpr,
        is_static: bool,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<Inferred> {
        let signatures = &self.class_sigs[class.0];
        let template = if is_static {
            signatures.static_generic_methods.get(name)
        } else {
            signatures.generic_methods.get(name)
        }?
        .clone();
        if template.rejected {
            return None;
        }
        let owner = source_name(&self.classes[class.0].name);
        let target = InferenceTarget::function(
            template.file,
            &template.type_params,
            &template.function,
            format!("{owner}.{name}"),
        );
        let contextual = self.contextual_result_candidates(&target, call);
        self.infer_arguments(&target, &call.args, &contextual, fx, pos)
    }

    /// Records the contextual type of the call that starts, and returns
    /// the context of the enclosing call. A call that `mark_operand_calls`
    /// marks has no contextual type (§212 rule 1a).
    pub(super) fn enter_call_context(
        &mut self,
        call: &ast::CallExpr,
        ctx: Option<&Type>,
    ) -> Option<Type> {
        let operand = self.operand_calls.remove(&call.span);
        let context = ctx.filter(|_| !operand && call.type_args.is_none());
        std::mem::replace(&mut self.active_call_context, context.cloned())
    }

    /// Marks each call whose value `expr` gives. The context that an
    /// operator operand, an index, or a later element of an array literal
    /// receives types a literal. `tsc` gives such an expression no
    /// contextual type, so it decides no type argument (§212 rule 1a,
    /// §215 rule 4).
    pub(super) fn mark_operand_calls(&mut self, expr: &ast::Expr) {
        match unparen_expr(expr) {
            ast::Expr::Call(call) => {
                self.operand_calls.insert(call.span);
            }
            ast::Expr::Cond(conditional) => {
                self.mark_operand_calls(&conditional.cons);
                self.mark_operand_calls(&conditional.alt);
            }
            ast::Expr::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.mark_operand_calls(&element.expr);
                }
            }
            ast::Expr::Await(awaited) => self.mark_operand_calls(&awaited.arg),
            _ => {}
        }
    }

    /// Matches the declared result type against the contextual type of
    /// `call` (§212 rule 1). The match goes through `C | null` in the
    /// contextual type, and in the value type of a contextual
    /// `Promise<C>`. A shape that does not match gives no candidate.
    /// The first match of a type parameter is its candidate. `void` gives
    /// no candidate.
    fn contextual_result_candidates(
        &mut self,
        target: &InferenceTarget<'_>,
        call: &ast::CallExpr,
    ) -> Vec<Option<Type>> {
        if self.active_call != Some(call.span) {
            return Vec::new();
        }
        let (Some(result), Some(context)) = (target.result, self.active_call_context.clone())
        else {
            return Vec::new();
        };
        let mut candidates = vec![Vec::new(); target.type_params.len()];
        let file = std::mem::replace(&mut self.cur_file, target.file);
        self.match_inference_type(result, &context, target.type_params, &mut candidates);
        if let Type::Nullable(inner, _) = &context {
            self.match_inference_type(result, inner, target.type_params, &mut candidates);
        }
        if let Some(handle) = self.non_null_handle(&context) {
            self.match_inference_type(result, &handle, target.type_params, &mut candidates);
        }
        self.cur_file = file;
        candidates
            .into_iter()
            .map(|values| {
                values
                    .into_iter()
                    .next()
                    .filter(|value| self.apparent_type(value) != Type::Void)
            })
            .collect()
    }

    /// `Promise<C>` for a contextual `Promise<C | null>` (§215 rule 3).
    fn non_null_handle(&self, context: &Type) -> Option<Type> {
        let Type::AsyncHandle(value) = self.apparent_type(context) else {
            return None;
        };
        match self.apparent_type(&value) {
            Type::Nullable(inner, _) => Some(Type::AsyncHandle(inner)),
            _ => None,
        }
    }

    /// Infers the type arguments of `new G(args)` for a generic class
    /// template `key` (§197 rules 1 to 3).
    pub(super) fn infer_constructor_arguments(
        &mut self,
        key: &str,
        args: &[ast::ExprOrSpread],
        context: Option<&Type>,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<Inferred> {
        let template = self.generic_classes.get(key)?.clone();
        let contextual = match self.contextual_instance(context) {
            Some(Type::Class(id)) => match self.instance_arguments.get(&id) {
                Some((instance_key, types)) if instance_key == key => {
                    types.iter().cloned().map(Some).collect()
                }
                _ => Vec::new(),
            },
            // A poisoned context already has its diagnostic.
            Some(Type::Error) => return None,
            _ => Vec::new(),
        };
        let target = InferenceTarget::constructor(&template, source_name(key));
        self.infer_arguments(&target, args, &contextual, fx, pos)
    }

    /// The apparent contextual type with an outer `| null` removed (§197 rule 3).
    pub(super) fn contextual_instance(&self, context: Option<&Type>) -> Option<Type> {
        match self.apparent_type(context?) {
            Type::Nullable(inner, _) => Some(self.apparent_type(&inner)),
            other => Some(other),
        }
    }

    /// Infers the type arguments of `new Map()` or `new Set(source)` with
    /// no type arguments (§197 rules 1, 3, and 5). Returns the key type,
    /// the value type of a `Map`, and the `Set` source that inference
    /// checked.
    pub(super) fn infer_container_arguments(
        &mut self,
        name: &str,
        args: &[ast::ExprOrSpread],
        context: Option<&Type>,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<(Vec<Type>, Option<hir::Expr>)> {
        let contextual = match (name, self.contextual_instance(context)) {
            ("Map", Some(Type::Map(key, value))) => Some(vec![*key, *value]),
            ("Set", Some(Type::Set(element))) => Some(vec![*element]),
            (_, Some(Type::Error)) => return None,
            _ => None,
        };
        if name == "Set" {
            if let [argument] = args {
                if argument.spread.is_none() {
                    let literal = deferred_literal(&argument.expr);
                    if !literal || contextual.is_none() {
                        let source = self.check_expr(&argument.expr, None, fx);
                        let element = match self.apparent_type(&source.ty) {
                            Type::Error => return None,
                            Type::Map(..) | Type::Generator(_) => Type::Error,
                            other => other
                                .iteration_element()
                                .map_or(Type::Error, |(_, element)| element),
                        };
                        return Some((vec![element], Some(source)));
                    }
                    let element = contextual.as_ref()?[0].clone();
                    let source =
                        self.check_expr(&argument.expr, Some(&Type::array(element.clone())), fx);
                    return Some((vec![element], Some(source)));
                }
            }
        }
        if let Some(types) = contextual {
            return Some((types, None));
        }
        if context.is_none() {
            self.reject_subset(
                RejectionSite::ContainerConstructorTypeArgumentsMissing,
                format!("`new {name}` requires explicit type arguments (Q24)"),
                pos.clone(),
            );
        } else {
            let parameter = if name == "Map" { "K" } else { "T" };
            self.reject_subset(
                RejectionSite::GenericInferenceNoCandidate,
                format!(
                    "cannot infer type parameter `{parameter}` of `{name}`: no candidate; use explicit type arguments"
                ),
                pos.clone(),
            );
        }
        None
    }

    /// Checks each non-literal once and gives deferred literals the instance's context.
    /// A contextual candidate follows the non-literal candidates and
    /// precedes the literal defaults. A non-literal argument has the
    /// context of its parameter type when each type parameter that the
    /// type names has a contextual candidate (§197 rule 3, §212 rule 2).
    fn infer_arguments(
        &mut self,
        target: &InferenceTarget<'_>,
        args: &[ast::ExprOrSpread],
        contextual: &[Option<Type>],
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<Inferred> {
        let parameters = target.type_params;
        let annotations = &target.annotations;
        let mut candidates = vec![Vec::new(); parameters.len()];
        let mut checked = vec![None; args.len()];
        let mut contextual = contextual.to_vec();
        let mut has_context: Vec<bool> = (0..parameters.len())
            .map(|index| contextual.get(index).is_some_and(Option::is_some))
            .collect();
        for (index, argument) in args.iter().enumerate() {
            if argument.spread.is_some() || deferred_literal(&argument.expr) {
                continue;
            }
            let context = annotations
                .get(index)
                .copied()
                .flatten()
                .and_then(|annotation| {
                    if type_mentions_missing(annotation, parameters, &has_context) {
                        None
                    } else {
                        // §197 rule 3: each contextual candidate replaces its parameter.
                        let file = self.cur_file;
                        self.cur_file = target.file;
                        let saved: Vec<_> = parameters
                            .iter()
                            .zip(&contextual)
                            .filter_map(|(parameter, candidate)| {
                                let candidate = candidate.clone()?;
                                Some((
                                    parameter.clone(),
                                    self.subst.insert(parameter.clone(), candidate),
                                ))
                            })
                            .collect();
                        let context = self.resolve_type(annotation);
                        for (parameter, previous) in saved {
                            match previous {
                                Some(previous) => self.subst.insert(parameter, previous),
                                None => self.subst.remove(&parameter),
                            };
                        }
                        self.cur_file = file;
                        Some(context)
                    }
                });
            let saved = std::mem::replace(
                &mut self.generic_callback_context,
                index < annotations.len()
                    && matches!(unparen_expr(&argument.expr), ast::Expr::Arrow(_)),
            );
            let value = self.check_expr(&argument.expr, context.as_ref(), fx);
            self.generic_callback_context = saved;
            if value.ty == Type::Error {
                return None;
            }
            if let Some(Some(annotation)) = annotations.get(index) {
                // §212 rule 3: `null` is the candidate of a parameter that
                // is the annotation, or a direct member of its union.
                if target.result.is_some() && value.ty == Type::Null {
                    for (index, candidate) in contextual.iter_mut().enumerate() {
                        if accepts_null_as_parameter(annotation, &parameters[index]) {
                            *candidate = None;
                            has_context[index] = false;
                        }
                    }
                }
                let file = self.cur_file;
                self.cur_file = target.file;
                self.match_inference_type(annotation, &value.ty, parameters, &mut candidates);
                self.cur_file = file;
            }
            checked[index] = Some(value);
        }
        // Defaults contribute only where no non-literal supplies a candidate.
        let mut nonliteral: Vec<_> = candidates
            .iter()
            .map(|values| values.iter().any(|value| *value != Type::Null))
            .collect();
        for (index, candidate) in contextual.iter().enumerate() {
            if let (Some(candidate), Some(false)) = (candidate, nonliteral.get(index).copied()) {
                candidates[index].push(candidate.clone());
                nonliteral[index] = true;
            }
        }
        for (index, argument) in args.iter().enumerate() {
            if argument.spread.is_some() || !deferred_literal(&argument.expr) {
                continue;
            }
            let Some(Some(annotation)) = annotations.get(index) else {
                continue;
            };
            if !type_mentions_missing(annotation, parameters, &nonliteral) {
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
            self.cur_file = target.file;
            self.match_inference_type(annotation, &value.ty, parameters, &mut defaults);
            self.cur_file = file;
            for (index, values) in defaults.into_iter().enumerate() {
                if !nonliteral[index] {
                    candidates[index].extend(values);
                }
            }
        }
        let mut types = Vec::new();
        for (parameter, values) in parameters.iter().zip(candidates) {
            let has_null = values.contains(&Type::Null);
            let mut values = values.into_iter().filter(|value| *value != Type::Null);
            let Some(mut inferred) = values.next() else {
                let site = if args.len() < target.required {
                    RejectionSite::GenericInferenceRequiredArgumentMissing
                } else {
                    RejectionSite::GenericInferenceNoCandidate
                };
                self.reject_subset(
                    site,
                    format!(
                        "cannot infer type parameter `{parameter}` of `{}`: no candidate; use explicit type arguments",
                        target.name
                    ),
                    pos.clone(),
                );
                return None;
            };
            for candidate in values {
                if candidate == inferred {
                    continue;
                }
                let joined = match (&inferred, &candidate) {
                    (Type::Nullable(inner, _), value) if inner.as_ref() == value => {
                        Some(inferred.clone())
                    }
                    (value, Type::Nullable(inner, _)) if inner.as_ref() == value => {
                        Some(candidate.clone())
                    }
                    _ => None,
                };
                let Some(joined) = joined else {
                    self.reject_subset(if self.apparent_type(&inferred).is_numeric() && self.apparent_type(&candidate).is_numeric() { RejectionSite::GenericInferenceConflictingCandidates } else { RejectionSite::GenericInferenceIncompatibleKinds }, format!(
                        "cannot infer type parameter `{parameter}` of `{}`: conflicting candidates `{}` and `{}`; use explicit type arguments",
                        target.name, self.type_name(&inferred), self.type_name(&candidate)), pos.clone());
                    return None;
                };
                inferred = joined;
            }
            if has_null
                && !matches!(inferred, Type::Nullable(_, _))
                && self.allows_nullable(&inferred)
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
                let actual = if let Type::Nullable(inner, _) = actual {
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
                    | ("Array", Type::Array(element))
                    | ("Set", Type::Set(element))
                    | ("Promise", Type::AsyncHandle(element)) => vec![element],
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
            | Type::Nullable(element, _) => vec![element],
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

/// Whether `ty` is the type parameter `parameter`, or a union with that
/// parameter as a direct member.
fn accepts_null_as_parameter(ty: &ast::TsType, parameter: &str) -> bool {
    match ty {
        ast::TsType::TsTypeRef(reference) => {
            reference.type_params.is_none()
                && matches!(&reference.type_name, ast::TsEntityName::Ident(name) if name.sym.as_ref() == parameter)
        }
        ast::TsType::TsParenthesizedType(parenthesized) => {
            accepts_null_as_parameter(&parenthesized.type_ann, parameter)
        }
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => union
            .types
            .iter()
            .any(|ty| accepts_null_as_parameter(ty, parameter)),
        _ => false,
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
