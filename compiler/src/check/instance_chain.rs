//! The active generic instance chain (compiler.md §140).

use super::*;

/// An edge from a requesting parameter to one requested argument (§140 rule 1).
#[derive(Clone)]
pub(crate) struct ParameterEdge {
    pub(super) source: usize,
    pub(super) expanding: bool,
}

/// One active request with its incoming edges and ordered parameter names.
#[derive(Clone)]
pub(crate) struct InstanceRequest {
    key: String,
    pub(super) args: Vec<Type>,
    parameters: Vec<String>,
    site: Pos,
    edges: Vec<Vec<ParameterEdge>>,
}

/// Resolved arguments with source positions and per-position edges (§140 rule 1).
pub(crate) struct InstanceArguments {
    pub types: Vec<Type>,
    pub positions: Vec<Pos>,
    pub ends: Vec<Pos>,
    pub edges: Vec<Vec<ParameterEdge>>,
}

impl<'p> Checker<'p> {
    /// Records edges before substitution, then resolves each argument (§140 rule 1).
    pub(super) fn resolve_instance_arguments(
        &mut self,
        arguments: &ast::TsTypeParamInstantiation,
    ) -> InstanceArguments {
        let edges = arguments
            .params
            .iter()
            .map(|ty| {
                let mut edges = Vec::new();
                self.record_parameter_edges(ty, false, &mut edges);
                edges
            })
            .collect();
        let types = arguments
            .params
            .iter()
            .map(|ty| self.resolve_type(ty))
            .collect();
        let positions = arguments
            .params
            .iter()
            .map(|ty| self.pos(ty.span()))
            .collect();
        InstanceArguments {
            types,
            positions,
            ends: arguments
                .params
                .iter()
                .map(|ty| {
                    let span = ty.span();
                    self.pos(swc_common::Span::new(span.hi, span.hi))
                })
                .collect(),
            edges,
        }
    }

    /// Visits every type form; rejected forms cannot request an instance (§140 rule 4).
    fn record_parameter_edges(
        &self,
        ty: &ast::TsType,
        expanding: bool,
        edges: &mut Vec<ParameterEdge>,
    ) {
        let visit = |ty: &ast::TsType, edges: &mut Vec<ParameterEdge>| {
            self.record_parameter_edges(ty, true, edges)
        };
        match ty {
            ast::TsType::TsTypeRef(reference) => {
                if let ast::TsEntityName::Ident(name) = &reference.type_name {
                    if let Some(source) = self.instance_chain.last().and_then(|request| {
                        request
                            .parameters
                            .iter()
                            .position(|parameter| parameter == name.sym.as_ref())
                    }) {
                        edges.push(ParameterEdge { source, expanding });
                    }
                }
                if let Some(arguments) = &reference.type_params {
                    for ty in &arguments.params {
                        visit(ty, edges);
                    }
                }
            }
            ast::TsType::TsArrayType(array) => visit(&array.elem_type, edges),
            ast::TsType::TsParenthesizedType(parenthesized) => {
                self.record_parameter_edges(&parenthesized.type_ann, expanding, edges)
            }
            ast::TsType::TsUnionOrIntersectionType(union) => {
                let types = match union {
                    ast::TsUnionOrIntersectionType::TsUnionType(union) => &union.types,
                    ast::TsUnionOrIntersectionType::TsIntersectionType(intersection) => {
                        &intersection.types
                    }
                };
                for ty in types {
                    visit(ty, edges);
                }
            }
            ast::TsType::TsFnOrConstructorType(function) => {
                let (params, annotation) = match function {
                    ast::TsFnOrConstructorType::TsFnType(function) => {
                        (&function.params, &function.type_ann)
                    }
                    ast::TsFnOrConstructorType::TsConstructorType(function) => {
                        (&function.params, &function.type_ann)
                    }
                };
                visit(&annotation.type_ann, edges);
                for param in params {
                    let annotation = match param {
                        ast::TsFnParam::Ident(binding) => &binding.type_ann,
                        ast::TsFnParam::Array(pattern) => &pattern.type_ann,
                        ast::TsFnParam::Rest(pattern) => &pattern.type_ann,
                        ast::TsFnParam::Object(pattern) => &pattern.type_ann,
                    };
                    if let Some(annotation) = annotation {
                        visit(&annotation.type_ann, edges);
                    }
                }
            }
            ast::TsType::TsKeywordType(_)
            | ast::TsType::TsThisType(_)
            | ast::TsType::TsTypeQuery(_)
            | ast::TsType::TsTypeLit(_)
            | ast::TsType::TsTupleType(_)
            | ast::TsType::TsOptionalType(_)
            | ast::TsType::TsRestType(_)
            | ast::TsType::TsConditionalType(_)
            | ast::TsType::TsInferType(_)
            | ast::TsType::TsTypeOperator(_)
            | ast::TsType::TsIndexedAccessType(_)
            | ast::TsType::TsMappedType(_)
            | ast::TsType::TsLitType(_)
            | ast::TsType::TsTypePredicate(_)
            | ast::TsType::TsImportType(_) => {}
        }
    }

    /// Composes the edges from an earlier instance to this request (§140 rule 2).
    fn growing_position(&self, index: usize, arguments: &InstanceArguments) -> Option<usize> {
        let previous = &self.instance_chain[index];
        for position in 0..previous.args.len() {
            let mut paths = vec![None; previous.args.len()];
            paths[position] = Some(false);
            for edges in self.instance_chain[index + 1..]
                .iter()
                .map(|request| &request.edges)
                .chain(std::iter::once(&arguments.edges))
            {
                paths = edges
                    .iter()
                    .map(|edges| {
                        edges
                            .iter()
                            .filter_map(|edge| {
                                paths
                                    .get(edge.source)
                                    .copied()
                                    .flatten()
                                    .map(|expanded| expanded || edge.expanding)
                            })
                            .max()
                    })
                    .collect();
            }
            if paths.get(position) == Some(&Some(true)) {
                return Some(position);
            }
        }
        None
    }

    /// Rejects an expanding path back to the same template position (§140 rule 2).
    pub(super) fn enter_instance(
        &mut self,
        key: &str,
        parameters: &[String],
        arguments: &InstanceArguments,
        pos: &Pos,
    ) -> bool {
        let args = &arguments.types;
        if args.iter().any(|ty| self.argument_has_error(ty)) {
            return false;
        }
        let growing = self
            .instance_chain
            .iter()
            .enumerate()
            .filter(|(_, request)| request.key == key)
            .find_map(|(index, _)| {
                self.growing_position(index, arguments)
                    .map(|position| (index, position))
            });
        if let Some((index, position)) = growing {
            let mut cycle: Vec<_> = self.instance_chain[index + 1..]
                .iter()
                .map(|request| request.site.clone())
                .collect();
            cycle.push(pos.clone());
            cycle.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));
            cycle.dedup();
            if let Some((_, site)) = self
                .growing_cycles
                .iter()
                .find(|(known, _)| *known == cycle)
            {
                if site != pos {
                    return false;
                }
            } else {
                self.growing_cycles.push((cycle, pos.clone()));
            }
            let render = |arguments: &[Type]| {
                let names: Vec<_> = arguments.iter().map(|ty| self.type_name(ty)).collect();
                format!("{}<{}>", source_name(key), names.join(", "))
            };
            let message = format!(
                "generic template `{}`: the chain of instances grows without bound from `{}` to `{}` through argument `{}`",
                source_name(key), render(&self.instance_chain[index].args), render(args), self.type_name(&args[position]),
            );
            let first = self.diags.len();
            self.error_diverging(
                RuleCode::S100,
                message,
                pos.clone(),
                Divergence::GrowingInstanceChain,
            );
            if let (Some(start), Some(end)) = (
                arguments.positions.get(position),
                arguments.ends.get(position),
            ) {
                self.growth_reports
                    .push((first, start.clone(), end.clone()));
            }
            return false;
        }
        self.instance_chain.push(InstanceRequest {
            key: key.to_owned(),
            args: args.to_vec(),
            parameters: parameters.to_vec(),
            site: pos.clone(),
            edges: arguments.edges.clone(),
        });
        true
    }
    /// An invalid nested argument gives no instance (§140 acceptance 2).
    fn argument_has_error(&self, ty: &Type) -> bool {
        *ty == Type::Error
            || matches!(ty, Type::Class(id) if self.instance_arguments.get(id).is_some_and(|(_, args)| args.iter().any(|ty| self.argument_has_error(ty))))
            || ty
                .contained_types()
                .iter()
                .any(|ty| self.argument_has_error(ty))
    }
}
