//! Argument inference for generic calls and constructions (§149, §197, §212).

use super::*;
use crate::check::expr::unparen_expr;
use crate::check::instance_chain::ParameterEdge;
use crate::check::rejection::RejectionSite;

fn deferred_literal(expr: &ast::Expression) -> bool {
    match unparen_expr(expr) {
        ast::Expression::NumericLiteral(_) | ast::Expression::StringLiteral(_) => true,
        ast::Expression::UnaryExpression(unary)
            if matches!(
                unary.operator,
                ast::UnaryOperator::UnaryNegation | ast::UnaryOperator::UnaryPlus
            ) =>
        {
            matches!(
                unparen_expr(&unary.argument),
                ast::Expression::NumericLiteral(_)
            )
        }
        ast::Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .all(|element| element.as_expression().is_some_and(deferred_literal)),
        _ => false,
    }
}

/// Whether the operand keeps its contextual type (compiler.md §232 rule 1).
fn literal_free(expr: &ast::Expression) -> bool {
    match unparen_expr(expr) {
        ast::Expression::NullLiteral(_) => true,
        ast::Expression::NumericLiteral(_)
        | ast::Expression::StringLiteral(_)
        | ast::Expression::BooleanLiteral(_)
        | ast::Expression::BigIntLiteral(_)
        | ast::Expression::RegExpLiteral(_)
        | ast::Expression::TemplateLiteral(_)
        | ast::Expression::ArrayExpression(_) => false,
        ast::Expression::UnaryExpression(unary) => !matches!(
            unary.operator,
            ast::UnaryOperator::UnaryNegation | ast::UnaryOperator::UnaryPlus
        ),
        ast::Expression::ConditionalExpression(conditional) => {
            literal_free(&conditional.consequent) && literal_free(&conditional.alternate)
        }
        _ => true,
    }
}

/// Whether `expr` holds a lambda with a parameter that has no type
/// annotation, as its value or in an element, a branch, a member, or a
/// lambda result. Such an argument of a generic call is checked after the
/// arguments that fix its type parameters (compiler.md §249 rule 2a).
fn context_sensitive(expr: &ast::Expression) -> bool {
    match unparen_expr(expr) {
        ast::Expression::ArrowFunctionExpression(arrow) => {
            arrow.params.items.iter().any(|parameter| matches!(parameter.pattern, ast::BindingPattern::BindingIdentifier(_)) && parameter.initializer.is_none() && parameter.type_annotation.is_none()) || arrow.body.as_expression().is_some_and(context_sensitive)
        }
        ast::Expression::ConditionalExpression(conditional) => {
            context_sensitive(&conditional.consequent) || context_sensitive(&conditional.alternate)
        }
        ast::Expression::ArrayExpression(array) => array
            .elements.iter().filter_map(array_element_expression).any(context_sensitive),
        ast::Expression::ObjectExpression(object) => object.properties.iter().any(|property| matches!(property, ast::ObjectPropertyKind::ObjectProperty(property) if !property.method && property.kind == ast::PropertyKind::Init && context_sensitive(&property.value))),
        _ => false,
    }
}

/// Whether the argument `argument` of a generic callee takes the site of
/// a generic callback (compiler.md §252 rule 1): a lambda in a place of a
/// function type `F`, or `F | null`, with `function_params` parameters,
/// and the lambda has no more parameters than `F`.
pub(super) fn generic_callback_place(
    function_params: Option<usize>,
    argument: &ast::Expression,
) -> bool {
    match unparen_expr(argument) {
        ast::Expression::ArrowFunctionExpression(lambda) => function_params.is_some_and(|count| {
            (lambda.params.items.len() + usize::from(lambda.params.rest.is_some())) <= count
        }),
        _ => false,
    }
}

fn formal_annotations<'s, 'a>(
    params: &'s ast::FormalParameters<'a>,
) -> impl Iterator<Item = Option<&'s ast::TSTypeAnnotation<'a>>> {
    params
        .items
        .iter()
        .map(|parameter| parameter.type_annotation.as_deref())
        .chain(
            params
                .rest
                .iter()
                .map(|rest| rest.type_annotation.as_deref()),
        )
}

fn pattern_annotation<'s, 'a>(
    parameter: &'s ast::FormalParameter<'a>,
) -> Option<&'s ast::TSType<'a>> {
    parameter
        .type_annotation
        .as_ref()
        .map(|annotation| &annotation.type_annotation)
}

/// The declared parameters that one inference reads (§149 rule 2, §197 rule 2).
struct InferenceTarget<'a> {
    file: usize,
    type_params: &'a [String],
    /// The annotation of each declared parameter, in order.
    annotations: Vec<Option<&'a ast::TSType<'a>>>,
    /// The declared result annotation of a function or a method (§212).
    result: Option<&'a ast::TSType<'a>>,
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
                .this_param
                .iter()
                .map(|this| {
                    this.type_annotation
                        .as_ref()
                        .map(|annotation| &annotation.type_annotation)
                })
                .chain(function.params.items.iter().map(pattern_annotation))
                .chain(function.params.rest.iter().map(|rest| {
                    rest.type_annotation
                        .as_ref()
                        .map(|annotation| &annotation.type_annotation)
                }))
                .collect(),
            result: function
                .return_type
                .as_ref()
                .map(|annotation| &annotation.type_annotation),
            required: function
                .params
                .items
                .iter()
                .rposition(|param| param.initializer.is_none())
                .map_or(0, |index| index + 1)
                + usize::from(function.this_param.is_some())
                + usize::from(function.params.rest.is_some()),
            name,
        }
    }

    /// The shape keeps the last constructor, and its plain parameters, as
    /// the constructor signature does. A class with no constructor has no
    /// parameter, so no argument gives a candidate (§197 rule 2).
    fn constructor(template: &'a GenericClass, name: String) -> Self {
        let mut parameters: Vec<(Option<&'a ast::TSType>, bool)> = Vec::new();
        for member in &template.class.body.body {
            let ast::ClassElement::MethodDefinition(constructor) = member else {
                continue;
            };
            if constructor.kind != ast::MethodDefinitionKind::Constructor {
                continue;
            }
            parameters = constructor
                .value
                .params
                .items
                .iter()
                .map(|parameter| {
                    (
                        pattern_annotation(parameter),
                        parameter.initializer.is_some(),
                    )
                })
                .collect();
            if let Some(rest) = &constructor.value.params.rest {
                parameters.push((
                    rest.type_annotation
                        .as_ref()
                        .map(|annotation| &annotation.type_annotation),
                    false,
                ));
            }
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
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
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
        self.infer_arguments(
            &target,
            &call.arguments,
            &contextual,
            self.nullish_fallback_call
                .as_ref()
                .is_some_and(|(span, _)| *span == call.span),
            fx,
            pos,
        )
    }

    /// Infers the type arguments of a generic method call (§197 rule 1,
    /// §212).
    pub(super) fn infer_method_arguments(
        &mut self,
        class: ClassId,
        name: &str,
        call: &ast::CallExpression,
        is_static: bool,
        fx: &mut FnCtx<'p>,
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
        self.infer_arguments(
            &target,
            &call.arguments,
            &contextual,
            self.nullish_fallback_call
                .as_ref()
                .is_some_and(|(span, _)| *span == call.span),
            fx,
            pos,
        )
    }

    /// The explicit `U` of `map<U>` or `flatMap<U>`, as the result type of
    /// the callback: `U` for `map`, `U[]` for `flatMap` (§229 rule 3). A
    /// callback with a block body and no result annotation keeps its
    /// annotation rule, so it gets no result type (§229 rule 5).
    pub(in crate::check) fn callback_result_argument(
        &mut self,
        method: hir::ArrFn,
        call: &ast::CallExpression,
    ) -> Option<Type> {
        let [argument] = self.type_arguments_of(call, 1)? else {
            return None;
        };
        if let [callback] = call.arguments.as_slice() {
            if let ast::Expression::ArrowFunctionExpression(arrow) =
                unparen_expr(argument_expression(callback))
            {
                if arrow.return_type.is_none()
                    && matches!(&arrow.body, ast::ArrowFunctionBody::FunctionBody(_))
                {
                    return None;
                }
            }
        }
        let element = self.resolve_type(argument);
        Some(match (method, self.apparent_type(&element)) {
            (_, Type::Error) => Type::Error,
            (hir::ArrFn::FlatMap, _) => Type::array(element),
            _ => element,
        })
    }

    /// The contextual result of the callback of `map` or `flatMap`: the
    /// element of the contextual array type for `map`, and that array
    /// type for `flatMap` (§229 rule 1). The match goes through
    /// `C | null`, as §212 rule 1 states. Only an arrow callback takes
    /// it, so no other arrow in the argument reads it.
    pub(in crate::check) fn callback_result_context(
        &self,
        method: hir::ArrFn,
        call: &ast::CallExpression,
    ) -> Option<Type> {
        let [callback] = call.arguments.as_slice() else {
            return None;
        };
        if callback.is_spread()
            || !matches!(
                unparen_expr(argument_expression(callback)),
                ast::Expression::ArrowFunctionExpression(_)
            )
        {
            return None;
        }
        if self.active_call != Some(call.span) {
            return None;
        }
        let context = self.contextual_instance(self.active_call_context.as_ref())?;
        let Type::Array(element) = self.apparent_type(&context) else {
            return None;
        };
        Some(match method {
            hir::ArrFn::FlatMap => Type::Array(element),
            _ => *element,
        })
    }

    /// The value type `C` of the contextual `Promise<C>` of a `then` call,
    /// the context of its fulfillment callback result (§233 rules 1 and 2).
    pub(in crate::check) fn reaction_result_context(
        &self,
        call: &ast::CallExpression,
    ) -> Option<Type> {
        if self.active_call != Some(call.span) {
            return None;
        }
        match self.apparent_type(self.active_call_context.as_ref()?) {
            Type::AsyncHandle(value) => Some(*value),
            _ => None,
        }
    }

    /// Records the contextual type of the call that starts, and returns
    /// the context of the enclosing call. A call that `mark_operand_calls`
    /// marks has no contextual type (§212 rule 1a).
    pub(super) fn enter_call_context(
        &mut self,
        call: &ast::CallExpression,
        ctx: Option<&Type>,
    ) -> Option<Type> {
        let operand = self.operand_calls.remove(&call.span);
        let context = ctx.filter(|_| !operand && call.type_arguments.is_none());
        std::mem::replace(&mut self.active_call_context, context.cloned())
    }

    /// Marks each call whose value `expr` gives. The context that an
    /// operator operand, an index, or a later element of an array literal
    /// receives types a literal. `tsc` gives such an expression no
    /// contextual type, so it decides no type argument (§212 rule 1a,
    /// §215 rule 4).
    pub(super) fn mark_operand_calls(&mut self, expr: &ast::Expression) {
        match unparen_expr(expr) {
            ast::Expression::CallExpression(call) => {
                self.operand_calls.insert(call.span);
            }
            ast::Expression::ConditionalExpression(conditional) => {
                self.mark_operand_calls(&conditional.consequent);
                self.mark_operand_calls(&conditional.alternate);
            }
            ast::Expression::ArrayExpression(array) => {
                for element in array.elements.iter().filter_map(array_element_expression) {
                    self.mark_operand_calls(element);
                }
            }
            ast::Expression::AwaitExpression(awaited) => self.mark_operand_calls(&awaited.argument),
            // §232 rule 1: the operands that keep the context of the
            // expression also keep a mark.
            ast::Expression::LogicalExpression(binary) => match binary.operator {
                ast::LogicalOperator::Or => {
                    self.mark_operand_calls(&binary.left);
                    self.mark_operand_calls(&binary.right);
                }
                ast::LogicalOperator::And => self.mark_operand_calls(&binary.right),
                ast::LogicalOperator::Coalesce => self.mark_operand_calls(&binary.left),
            },
            ast::Expression::ChainExpression(chain) => {
                if let ast::ChainElement::CallExpression(call) = &chain.expression {
                    self.operand_calls.insert(call.span);
                }
            }
            _ => {}
        }
    }

    /// The context of an operand that keeps the context of its expression
    /// (§232 rules 1 and 3). By §232 rule 1, a literal other than `null`, a
    /// template, and an array literal take no context, so each keeps its
    /// present type. `null` has one type in each context.
    pub(super) fn operand_context(
        &self,
        operand: &ast::Expression,
        ctx: Option<&Type>,
    ) -> Option<Type> {
        ctx.filter(|_| literal_free(operand)).cloned()
    }

    /// The context of the left operand of `??`: the context with `| null`,
    /// as the left operand can be absent (§232 rule 1). An optional call
    /// there drops the absence again.
    pub(super) fn nullish_left_context(&self, ctx: Option<&Type>) -> Option<Type> {
        let ctx = ctx?;
        Some(match self.apparent_type(ctx) {
            Type::Nullable(_, _) => ctx.clone(),
            _ if self.allows_nullable(ctx) => Type::nullable(ctx.clone()),
            _ => ctx.clone(),
        })
    }

    /// Checks `check` as the call `call` with the contextual type `ctx`, so
    /// §212 inference reads it. An optional call takes the context with
    /// `| undefined` and `| null` dropped (§232 rule 1).
    pub(super) fn with_optional_call_context<T>(
        &mut self,
        call: &ast::CallExpression,
        ctx: Option<&Type>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let context = ctx.map(|ctx| match self.apparent_type(ctx) {
            Type::Nullable(inner, _) => *inner,
            _ => ctx.clone(),
        });
        let active = self.active_call.replace(call.span);
        let active_context = self.enter_call_context(call, context.as_ref());
        let checked = check(self);
        self.active_call = active;
        self.active_call_context = active_context;
        checked
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
        call: &ast::CallExpression,
    ) -> Vec<Option<Type>> {
        if self.active_call != Some(call.span) {
            return Vec::new();
        }
        let context = self
            .nullish_fallback_call
            .as_ref()
            .filter(|(span, _)| *span == call.span)
            .map(|(_, context)| context.clone())
            .or_else(|| self.active_call_context.clone());
        let (Some(result), Some(context)) = (target.result, context) else {
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
        args: &[ast::Argument],
        context: Option<&Type>,
        fx: &mut FnCtx<'p>,
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
        self.infer_arguments(&target, args, &contextual, false, fx, pos)
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
        args: &[ast::Argument],
        context: Option<&Type>,
        fx: &mut FnCtx<'p>,
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
                if !argument.is_spread() {
                    let literal = deferred_literal(argument_expression(argument));
                    if !literal || contextual.is_none() {
                        let source = self.check_expr(argument_expression(argument), None, fx);
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
                    let source = self.check_expr(
                        argument_expression(argument),
                        Some(&Type::array(element.clone())),
                        fx,
                    );
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

    /// The count of parameters of the function type that the declared
    /// annotation of a parameter of `target` names, before the substitution
    /// of its type arguments: a function type `F`, or `F | null` (compiler.md
    /// §252 rule 1, §251). A type parameter names no function type.
    fn declared_function_arity(
        &self,
        annotation: &ast::TSType,
        target: &InferenceTarget<'_>,
    ) -> Option<usize> {
        match annotation {
            ast::TSType::TSParenthesizedType(parenthesized) => {
                self.declared_function_arity(&parenthesized.type_annotation, target)
            }
            ast::TSType::TSFunctionType(function) => Some(
                function.params.items.len()
                    + usize::from(function.params.rest.is_some())
                    + usize::from(function.this_param.is_some()),
            ),

            ast::TSType::TSUnionType(union) => {
                let mut members = union
                    .types
                    .iter()
                    .filter(|member| !matches!(&**member, ast::TSType::TSNullKeyword(_)));
                match (members.next(), members.next()) {
                    (Some(member), None) => self.declared_function_arity(member, target),
                    _ => None,
                }
            }
            ast::TSType::TSTypeReference(reference) => {
                let ast::TSTypeName::IdentifierReference(name) = &reference.type_name else {
                    return None;
                };
                let name = name.name.as_ref();
                if target.type_params.iter().any(|parameter| parameter == name) {
                    return None;
                }
                // A type alias is not generic (compiler.md §202 rule 7), so it
                // resolves with no type argument.
                self.type_aliases
                    .get(name)
                    .and_then(|ty| self.lambda_context(ty))
                    .map(|function| function.params.len())
            }
            _ => None,
        }
    }

    /// The inferred type of a type parameter from its candidates: the
    /// first candidate that is not `null`, joined with each other one,
    /// and nullable when a candidate is `null` (§149 rule 4, §212 rule 3).
    /// Returns no type with no candidate, and the two candidates that
    /// conflict.
    fn join_candidates(&self, values: &[Type]) -> Result<Option<Type>, (Type, Type)> {
        let has_null = values.contains(&Type::Null);
        let mut values = values.iter().filter(|value| **value != Type::Null).cloned();
        let Some(mut inferred) = values.next() else {
            return Ok(None);
        };
        for candidate in values {
            if candidate == inferred {
                continue;
            }
            inferred = match (&inferred, &candidate) {
                (Type::Nullable(inner, _), value) if inner.as_ref() == value => inferred.clone(),
                (value, Type::Nullable(inner, _)) if inner.as_ref() == value => candidate.clone(),
                _ => return Err((inferred, candidate)),
            };
        }
        if has_null && !matches!(inferred, Type::Nullable(_, _)) && self.allows_nullable(&inferred)
        {
            inferred = Type::nullable(inferred);
        }
        Ok(Some(inferred))
    }

    /// Checks each non-literal once and gives deferred literals the instance's context.
    /// A contextual candidate follows the non-literal candidates and
    /// precedes the literal defaults. A non-literal argument has the
    /// context of its parameter type when each type parameter that the
    /// type names has a contextual candidate (§197 rule 3, §212 rule 2).
    fn infer_arguments(
        &mut self,
        target: &InferenceTarget<'_>,
        args: &[ast::Argument],
        contextual: &[Option<Type>],
        missing_only: bool,
        fx: &mut FnCtx<'p>,
        pos: &Pos,
    ) -> Option<Inferred> {
        let parameters = target.type_params;
        let annotations = &target.annotations;
        let mut candidates = vec![Vec::new(); parameters.len()];
        let mut checked = vec![None; args.len()];
        let mut fallback_context = missing_only.then(|| contextual.to_vec());
        let mut contextual = if missing_only {
            Vec::new()
        } else {
            contextual.to_vec()
        };
        let mut has_context: Vec<bool> = (0..parameters.len())
            .map(|index| contextual.get(index).is_some_and(Option::is_some))
            .collect();
        // compiler.md §249 rule 2a: an argument that holds a lambda with a
        // parameter that has no annotation is checked after the others,
        // with the type parameters that they fix.
        let order: Vec<usize> = (0..args.len())
            .filter(|index| !context_sensitive(argument_expression(&args[*index])))
            .chain(
                (0..args.len())
                    .filter(|index| context_sensitive(argument_expression(&args[*index]))),
            )
            .collect();
        let mut fixed_by_others = false;
        for index in order {
            let argument = &args[index];
            if argument.is_spread() || deferred_literal(argument_expression(argument)) {
                continue;
            }
            if context_sensitive(argument_expression(argument)) && !fixed_by_others {
                fixed_by_others = true;
                for (parameter, values) in candidates.iter().enumerate() {
                    // The inferred type of the candidates so far (rule 2a).
                    let fixed = self.join_candidates(values).ok().flatten();
                    if contextual.len() <= parameter {
                        contextual.resize(parameter + 1, None);
                    }
                    if let (Some(fixed), None) = (fixed, &contextual[parameter]) {
                        contextual[parameter] = Some(fixed);
                        has_context[parameter] = true;
                    }
                }
            }
            // A nested undecided operand gets the same missing-only context.
            let nested = match unparen_expr(argument_expression(argument)) {
                ast::Expression::CallExpression(call) => call.type_arguments.is_none(),
                ast::Expression::ArrayExpression(array) => array.elements.is_empty(),
                _ => false,
            };
            // §319 rule 1a: preserve the context supplied at the pin first.
            let use_fallback = nested
                && missing_only
                && annotations
                    .get(index)
                    .copied()
                    .flatten()
                    .is_some_and(|annotation| {
                        type_mentions_missing(annotation, parameters, &has_context)
                    });
            let argument_candidates = if use_fallback {
                fallback_context.as_ref().unwrap_or(&contextual)
            } else {
                &contextual
            };
            let argument_has_context: Vec<_> = (0..parameters.len())
                .map(|index| argument_candidates.get(index).is_some_and(Option::is_some))
                .collect();
            let context = annotations
                .get(index)
                .copied()
                .flatten()
                .and_then(|annotation| {
                    if type_mentions_missing(annotation, parameters, &argument_has_context) {
                        None
                    } else {
                        // §197 rule 3: each contextual candidate replaces its parameter.
                        let file = self.cur_file;
                        self.cur_file = target.file;
                        let saved: Vec<_> = parameters
                            .iter()
                            .zip(argument_candidates)
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
            // compiler.md §252 rule 1: a call that infers its type
            // arguments reads the declared type of the place.
            let callback_place = generic_callback_place(
                annotations
                    .get(index)
                    .copied()
                    .flatten()
                    .and_then(|annotation| self.declared_function_arity(annotation, target)),
                argument_expression(argument),
            );
            let saved = std::mem::replace(&mut self.generic_callback_context, callback_place);
            // §229 rule 1: a callback whose result is a type parameter with a
            // contextual candidate checks its body with that candidate. The
            // body still gives the candidate of the parameter (§229 rule 2).
            // §238 rule 2: a function type with the result `void` gives
            // that result as context, with no candidate of its parameters.
            if context.is_none()
                && matches!(
                    unparen_expr(argument_expression(argument)),
                    ast::Expression::ArrowFunctionExpression(_)
                )
                && annotations
                    .get(index)
                    .copied()
                    .flatten()
                    .is_some_and(returns_void)
            {
                self.lambda_void_context = true;
            }
            if context.is_none()
                && matches!(
                    unparen_expr(argument_expression(argument)),
                    ast::Expression::ArrowFunctionExpression(_)
                )
            {
                self.lambda_result_hint = annotations
                    .get(index)
                    .copied()
                    .flatten()
                    .and_then(|annotation| callback_result_parameter(annotation, parameters))
                    .and_then(|parameter| contextual.get(parameter).cloned().flatten());
            }
            let saved_fallback = self.nullish_fallback_call.clone();
            let nested_call = if use_fallback {
                match unparen_expr(argument_expression(argument)) {
                    ast::Expression::CallExpression(call) if call.type_arguments.is_none() => {
                        context.clone().map(|context| (call.span, context))
                    }
                    _ => None,
                }
            } else {
                None
            };
            let nested_missing = nested_call.is_some();
            if nested_missing {
                self.nullish_fallback_call = nested_call;
            }
            let value = self.check_expr(
                argument_expression(argument),
                if nested_missing {
                    None
                } else {
                    context.as_ref()
                },
                fx,
            );
            self.nullish_fallback_call = saved_fallback;
            self.lambda_result_hint = None;
            self.lambda_void_context = false;
            self.generic_callback_context = saved;
            if value.ty == Type::Error {
                return None;
            }
            if let Some(Some(annotation)) = annotations.get(index) {
                // §212 rule 3: `null` is the candidate of a parameter that
                // is the annotation, or a direct member of its union.
                if target.result.is_some() && value.ty == Type::Null {
                    for (index, parameter) in parameters.iter().enumerate() {
                        if accepts_null_as_parameter(annotation, parameter) {
                            if let Some(candidate) = contextual.get_mut(index) {
                                *candidate = None;
                            }
                            if let Some(candidate) = fallback_context
                                .as_mut()
                                .and_then(|values| values.get_mut(index))
                            {
                                *candidate = None;
                            }
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
            if argument.is_spread() || !deferred_literal(argument_expression(argument)) {
                continue;
            }
            let Some(Some(annotation)) = annotations.get(index) else {
                continue;
            };
            if !type_mentions_missing(annotation, parameters, &nonliteral) {
                continue;
            }
            if matches!(unparen_expr(argument_expression(argument)), ast::Expression::ArrayExpression(array) if array.elements.is_empty())
            {
                continue;
            }
            let diagnostics = self.diags.len();
            let value = self.check_expr(argument_expression(argument), None, fx);
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
        // §319: argument candidates and literal defaults retain their type.
        // Only a missing candidate reads the fallback call's result context.
        if let Some(contextual) = fallback_context {
            for (values, context) in candidates.iter_mut().zip(contextual) {
                if self.join_candidates(values).ok().flatten().is_none() {
                    if let Some(context) = context {
                        values.push(context);
                    }
                }
            }
        }
        let mut types = Vec::new();
        for (parameter, values) in parameters.iter().zip(candidates) {
            let inferred = match self.join_candidates(&values) {
                Ok(inferred) => inferred,
                Err((inferred, candidate)) => {
                    self.reject_subset(if self.apparent_type(&inferred).is_numeric() && self.apparent_type(&candidate).is_numeric() { RejectionSite::GenericInferenceConflictingCandidates } else { RejectionSite::GenericInferenceIncompatibleKinds }, format!(
                        "cannot infer type parameter `{parameter}` of `{}`: conflicting candidates `{}` and `{}`; use explicit type arguments",
                        target.name, self.type_name(&inferred), self.type_name(&candidate)), pos.clone());
                    return None;
                }
            };
            let Some(inferred) = inferred else {
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
        pattern: &ast::TSType,
        actual: &Type,
        parameters: &[String],
        candidates: &mut [Vec<Type>],
    ) {
        if *actual == Type::Error {
            return;
        }
        match pattern {
            ast::TSType::TSParenthesizedType(parenthesized) => self.match_inference_type(
                &parenthesized.type_annotation,
                actual,
                parameters,
                candidates,
            ),

            ast::TSType::TSArrayType(array) => {
                if let Type::Array(element) = self.apparent_type(actual) {
                    self.match_inference_type(
                        &array.element_type,
                        &element,
                        parameters,
                        candidates,
                    );
                }
            }

            ast::TSType::TSUnionType(union) => {
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
            ast::TSType::TSTypeReference(reference) => {
                let ast::TSTypeName::IdentifierReference(name) = &reference.type_name else {
                    return;
                };
                if let Some(index) = parameters
                    .iter()
                    .position(|parameter| parameter == name.name.as_ref())
                {
                    candidates[index].push(actual.clone());
                    return;
                }
                let Some(arguments) = &reference.type_arguments else {
                    return;
                };
                let shape = self.apparent_type(actual);
                let components: Vec<&Type> = match (name.name.as_ref(), &shape) {
                    ("FixedArray", Type::FixedArray(element, _) | Type::Array(element))
                    | ("Array", Type::Array(element))
                    | ("Set", Type::Set(element))
                    | ("Promise", Type::AsyncHandle(element)) => vec![element],
                    ("Map", Type::Map(key, value)) => vec![key, value],
                    (_, Type::Class(id)) => {
                        let Some((key, types)) = self.instance_arguments.get(id) else {
                            return;
                        };
                        if !matches!(self.type_scope_item(name.name.as_ref()), Some(ScopeItem::GenericClass(expected)) if expected == *key)
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
            ast::TSType::TSFunctionType(function) => {
                if let Type::Func(signature) = self.apparent_type(actual) {
                    for (annotation, actual) in function
                        .this_param
                        .iter()
                        .map(|this| this.type_annotation.as_deref())
                        .chain(formal_annotations(&function.params))
                        .zip(&signature.params)
                    {
                        if let Some(annotation) = annotation {
                            self.match_inference_type(
                                &annotation.type_annotation,
                                actual,
                                parameters,
                                candidates,
                            );
                        }
                    }
                    self.match_inference_type(
                        &function.return_type.type_annotation,
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
            | Type::Nullable(element, _)
            | Type::AsyncHandle(element) => vec![element],
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
fn accepts_null_as_parameter(ty: &ast::TSType, parameter: &str) -> bool {
    match ty {
        ast::TSType::TSParenthesizedType(parenthesized) => {
            accepts_null_as_parameter(&parenthesized.type_annotation, parameter)
        }
        ast::TSType::TSTypeReference(reference) => {
            reference.type_arguments.is_none()
                && matches!(&reference.type_name, ast::TSTypeName::IdentifierReference(name) if name.name.as_ref() == parameter)
        }

        ast::TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|ty| accepts_null_as_parameter(ty, parameter)),
        _ => false,
    }
}

/// The index of the type parameter that is the result type of the
/// function type `ty` (§229 rule 1).
fn callback_result_parameter(ty: &ast::TSType, parameters: &[String]) -> Option<usize> {
    let ast::TSType::TSFunctionType(function) = ty else {
        return match ty {
            ast::TSType::TSParenthesizedType(parenthesized) => {
                callback_result_parameter(&parenthesized.type_annotation, parameters)
            }

            _ => None,
        };
    };
    let ast::TSType::TSTypeReference(reference) = &function.return_type.type_annotation else {
        return None;
    };
    let ast::TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return None;
    };
    if reference.type_arguments.is_some() {
        return None;
    }
    parameters
        .iter()
        .position(|parameter| parameter == name.name.as_ref())
}

fn type_mentions_missing(ty: &ast::TSType, parameters: &[String], nonliteral: &[bool]) -> bool {
    match ty {
        ast::TSType::TSParenthesizedType(parenthesized) => {
            type_mentions_missing(&parenthesized.type_annotation, parameters, nonliteral)
        }
        ast::TSType::TSTypeReference(reference) => {
            matches!(&reference.type_name, ast::TSTypeName::IdentifierReference(name) if parameters.iter().enumerate().any(|(index, parameter)| !nonliteral[index] && parameter == name.name.as_ref()))
                || reference.type_arguments.as_ref().is_some_and(|args| {
                    args.params
                        .iter()
                        .any(|ty| type_mentions_missing(ty, parameters, nonliteral))
                })
        }
        ast::TSType::TSArrayType(array) => {
            type_mentions_missing(&array.element_type, parameters, nonliteral)
        }

        ast::TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|ty| type_mentions_missing(ty, parameters, nonliteral)),
        ast::TSType::TSFunctionType(function) => {
            type_mentions_missing(
                &function.return_type.type_annotation,
                parameters,
                nonliteral,
            ) || function
                .this_param
                .iter()
                .map(|this| this.type_annotation.as_deref())
                .chain(formal_annotations(&function.params))
                .any(|annotation| {
                    annotation.is_some_and(|annotation| {
                        type_mentions_missing(&annotation.type_annotation, parameters, nonliteral)
                    })
                })
        }
        _ => false,
    }
}

/// Whether `annotation` is a function type with the result `void`
/// (compiler.md §238 rule 2).
fn returns_void(annotation: &ast::TSType) -> bool {
    match annotation {
        ast::TSType::TSParenthesizedType(parenthesized) => {
            returns_void(&parenthesized.type_annotation)
        }

        ast::TSType::TSFunctionType(function) => {
            matches!(
                &function.return_type.type_annotation,
                ast::TSType::TSVoidKeyword(_)
            )
        }
        _ => false,
    }
}

#[cfg(test)]
mod ast_migration_tests {
    use super::*;
    use oxc_allocator::Allocator;

    fn source_type<'a>(allocator: &'a Allocator, spelling: &str) -> ast::TSType<'a> {
        let source = allocator.alloc_str(&format!("type Probe = {spelling};"));
        let mut parsed = oxc_parser::Parser::new(
            allocator,
            source,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let ast::Statement::TSTypeAliasDeclaration(alias) = parsed.program.body.remove(0) else {
            panic!("type alias");
        };
        alias.unbox().type_annotation
    }

    fn source_arrow<'a>(allocator: &'a Allocator, spelling: &str) -> ast::Expression<'a> {
        let source = allocator.alloc_str(&format!("const probe = {spelling};"));
        let mut parsed = oxc_parser::Parser::new(
            allocator,
            source,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let ast::Statement::VariableDeclaration(declaration) = parsed.program.body.remove(0) else {
            panic!("variable");
        };
        declaration
            .unbox()
            .declarations
            .pop()
            .expect("declarator")
            .init
            .expect("initializer")
    }

    // Cost (debug): <0.01 s in the isolated AST helper harness.
    #[test]
    fn parenthesized_types_keep_parameter_and_void_facts() {
        let allocator = Allocator::default();
        for spelling in ["T", "(T)", "((T))", "(T | null)"] {
            let ty = source_type(&allocator, spelling);
            assert!(accepts_null_as_parameter(&ty, "T"), "{spelling}");
            assert!(type_mentions_missing(&ty, &["T".into()], &[false]));
        }
        assert!(!accepts_null_as_parameter(
            &source_type(&allocator, "(i32)"),
            "T"
        ));
        assert_eq!(
            callback_result_parameter(&source_type(&allocator, "((() => T))"), &["T".into()]),
            Some(0)
        );
        assert!(returns_void(&source_type(&allocator, "((() => void))")));
        assert!(!returns_void(&source_type(&allocator, "((() => i32))")));
    }

    // Cost (debug): <0.01 s in the isolated AST helper harness.
    #[test]
    fn source_parameters_keep_this_defaults_optionality_and_rest_count() {
        let allocator = Allocator::default();
        let parsed = oxc_parser::Parser::new(
            &allocator,
            "function f(this: i32, value: i32 = 1, optional?: i32, ...rest: i32[]): void {}",
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let ast::Statement::FunctionDeclaration(function) = &parsed.program.body[0] else {
            panic!("function");
        };
        assert_eq!(function_parameter_count(function), 4);
        let parameters = function_parameters(function, &allocator);
        assert_eq!(parameters.len(), 3);
        assert_eq!(
            parameter_name_from_pat(&parameters[0].pattern),
            Some("this")
        );
        assert!(parameters[0].type_annotation.is_some());
        assert!(parameters[1].initializer.is_some());
        assert!(parameters[2].optional);
        assert!(function.params.rest.is_some());
    }

    // Cost (debug): <0.01 s in the isolated AST helper harness.
    #[test]
    fn contextual_arrow_parameters_distinguish_defaults_and_annotations() {
        let allocator = Allocator::default();
        assert!(context_sensitive(&source_arrow(
            &allocator,
            "(value) => value"
        )));
        assert!(!context_sensitive(&source_arrow(
            &allocator,
            "(value = 1) => value"
        )));
        assert!(!context_sensitive(&source_arrow(
            &allocator,
            "(value: i32) => value"
        )));
        assert!(context_sensitive(&source_arrow(
            &allocator,
            "[(value) => value]"
        )));
        assert!(!deferred_literal(&source_arrow(
            &allocator,
            "[1, ...other]"
        )));
        assert!(!deferred_literal(&source_arrow(&allocator, "[1,,2]")));
        assert!(deferred_literal(&source_arrow(&allocator, "[1, 2]")));
    }

    // Cost (debug): <0.01 s in the isolated AST helper harness.
    #[test]
    fn function_type_annotations_include_the_rest_parameter() {
        let allocator = Allocator::default();
        let ty = source_type(&allocator, "(first: i32, ...rest: T[]) => void");
        let ast::TSType::TSFunctionType(function) = &ty else {
            panic!("function type");
        };
        let annotations: Vec<_> = formal_annotations(&function.params).collect();
        assert_eq!(annotations.len(), 2);
        assert!(matches!(
            annotations[1].expect("rest annotation").type_annotation,
            ast::TSType::TSArrayType(_)
        ));
        assert!(type_mentions_missing(&ty, &["T".into()], &[false]));
    }
    // Cost (debug): <0.01 s in the isolated AST helper harness.
    #[test]
    fn operand_context_distinguishes_literals_from_value_expressions() {
        let allocator = Allocator::default();
        for expression in ["value", "null", "flag ? first : second"] {
            assert!(
                literal_free(&source_arrow(&allocator, expression)),
                "{expression}"
            );
        }
        for expression in [
            "1",
            "true",
            "\"text\"",
            "[value]",
            "`text`",
            "-value",
            "flag ? first : 1",
        ] {
            assert!(
                !literal_free(&source_arrow(&allocator, expression)),
                "{expression}"
            );
        }
    }
}
