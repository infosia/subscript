//! Block-owned lambda group identities and signatures (compiler.md §293).
use super::*;

/// An earlier name is checked as an ordinary const capture until forward links are known.
#[derive(Clone)]
pub(crate) struct EarlierRead {
    pub pos: Pos,
    pub member: Member,
    pub nested: bool,
    pub decided_result: bool,
    pub rejection_site: RejectionSite,
}

#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub name: String,
    pub span: swc_common::Span,
    pub id: hir::LambdaId,
    pub group: hir::LambdaId,
    pub signature: Option<crate::types::FuncType>,
    pub params: Vec<ParamSig>,
    pub is_async: bool,
    pub ready: bool,
}

impl Checker<'_> {
    /// Reserve candidate runs; checked name references select their groups (§293 rule 1).
    pub(super) fn form_lambda_groups(&mut self, statements: &[ast::Stmt], fx: &mut FnCtx) {
        let mut runs = Vec::<Vec<(&ast::BindingIdent, &ast::ArrowExpr)>>::new();
        let mut run = Vec::new();
        for statement in statements {
            let declarations = match statement {
                ast::Stmt::Decl(ast::Decl::Var(v)) if v.kind == ast::VarDeclKind::Const => {
                    Some(&v.decls)
                }
                _ => None,
            };
            if let Some(declarations) = declarations {
                for d in declarations {
                    if let (ast::Pat::Ident(binding), Some(ast::Expr::Arrow(arrow))) =
                        (&d.name, d.init.as_deref().map(expr::unparen_expr))
                    {
                        if binding.type_ann.is_none() {
                            run.push((binding, arrow));
                            continue;
                        }
                    }
                    if !run.is_empty() {
                        runs.push(std::mem::take(&mut run));
                    }
                }
            } else if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        for run in runs {
            let group = hir::LambdaId(self.next_lambda_id);
            let mut members = Vec::new();
            for (binding, arrow) in run {
                let id = hir::LambdaId(self.next_lambda_id);
                self.next_lambda_id += 1;
                // Resolve reserved signatures with the same borrow context as
                // ordinary lambdas, including candidates that remain ungrouped.
                let saved_group_parameters = self.task_group_parameters;
                self.task_group_parameters = !arrow.is_async
                    && !arrow.is_generator
                    && !fx.frames.iter().any(|frame| frame.is_generator);
                let params: Vec<_> = arrow
                    .params
                    .iter()
                    .map(|p| self.resolve_param_pat(p))
                    .collect();
                self.task_group_parameters = saved_group_parameters;
                let result = arrow
                    .return_type
                    .as_ref()
                    .map(|a| self.resolve_result_type(&a.type_ann))
                    .or_else(|| match &*arrow.body {
                        ast::BlockStmtOrExpr::BlockStmt(block)
                            if !expr::lambda::returns_matching(&block.stmts, |_| true) =>
                        {
                            Some(if arrow.is_async {
                                Type::async_handle(Type::Void)
                            } else {
                                Type::Void
                            })
                        }
                        _ => None,
                    });
                let signature = result.map(|ret| crate::types::FuncType {
                    params: params.iter().map(|p| p.ty().clone()).collect(),
                    ret,
                    callback_typedef: None,
                });
                members.push(Member {
                    name: binding.id.sym.to_string(),
                    span: arrow.span,
                    id,
                    group,
                    signature,
                    params,
                    is_async: arrow.is_async,
                    ready: false,
                });
            }
            if let Some(scope) = fx.scopes.last_mut() {
                for member in &members {
                    scope.lambda_groups.insert(member.span, members.clone());
                }
            }
        }
    }

    pub(super) fn prepare_lambda_group(
        &mut self,
        span: swc_common::Span,
        fx: &mut FnCtx,
    ) -> Option<Member> {
        let scope = fx
            .scopes
            .iter()
            .rposition(|scope| scope.lambda_groups.contains_key(&span))?;
        let mut members = fx.scopes[scope].lambda_groups[&span].clone();
        if members.iter().any(|m| !m.ready) {
            for member in &mut members {
                self.decide_lambda_parameters(&mut member.params, Evaluation::Deferred, fx);
                if let Some(signature) = &mut member.signature {
                    signature.params = member.params.iter().map(|p| p.ty().clone()).collect();
                }
                member.ready = true;
            }
            for member in &members {
                fx.scopes[scope]
                    .lambda_groups
                    .insert(member.span, members.clone());
            }
        }
        members.into_iter().find(|member| member.span == span)
    }

    pub(super) fn lambda_group_member(&self, span: swc_common::Span, fx: &FnCtx) -> Option<Member> {
        fx.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.lambda_groups.get(&span))?
            .iter()
            .find(|member| member.span == span)
            .cloned()
    }

    fn group_read(&mut self, name: &str, fx: &FnCtx, pos: &Pos) -> Option<Result<Member, ()>> {
        let mut crossed = false;
        for scope in fx.scopes.iter().rev() {
            if scope.owns(name) {
                return None;
            }
            if let Some(member) = scope.group_members.get(name) {
                let owner = fx
                    .frames
                    .iter()
                    .rev()
                    .filter_map(|f| f.lambda_id)
                    .find(|id| scope.group_members.values().any(|m| m.id == *id));
                if let Some(owner) = owner.filter(|owner| member.id.0 < owner.0) {
                    let decided_result = member.signature.is_some();
                    let mut member = member.clone();
                    if let Some(local) = fx.scopes.iter().rev().find_map(|s| s.vars.get(name)) {
                        if let Type::Func(signature) = self.apparent_type(&local.ty) {
                            member.signature = Some(*signature);
                        }
                    }
                    let rejection_site = self.pending_capture_site(name, fx);
                    let reads = self.earlier_member_reads.entry(owner).or_default();
                    if !reads.iter().any(|read| read.pos == *pos) {
                        reads.push(EarlierRead {
                            pos: pos.clone(),
                            member,
                            nested: crossed,
                            decided_result,
                            rejection_site,
                        });
                    }
                    return None;
                }
                if crossed {
                    self.reject_subset(
                        self.pending_capture_site(name, fx),
                        format!("`{name}` is read before its declaration in this block"),
                        pos.clone(),
                    );
                    return Some(Err(()));
                }
                if member.signature.is_none()
                    && fx.frames.last().and_then(|f| f.lambda_id) != Some(member.id)
                {
                    self.cycle(pos.clone());
                    return Some(Err(()));
                }
                return Some(Ok(member.clone()));
            }
            crossed |= scope.fn_boundary;
        }
        None
    }

    pub(super) fn check_group_value(
        &mut self,
        name: &str,
        fx: &FnCtx,
        pos: &Pos,
    ) -> Option<hir::Expr> {
        let member = match self.group_read(name, fx, pos)? {
            Ok(member) => member,
            Err(()) => return Some(self.err_expr(pos.clone())),
        };
        let signature = member.signature?;
        Some(hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::LambdaMember(member.id),
            ty: Type::Func(Box::new(signature)),
            pos: pos.clone(),
        })
    }

    pub(super) fn check_group_call(
        &mut self,
        name: &ast::Ident,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<hir::Expr> {
        let member = match self.group_read(name.sym.as_ref(), fx, pos)? {
            Ok(member) => member,
            Err(()) => return Some(self.err_expr(pos.clone())),
        };
        let signature = member.signature?;
        self.mark_user_call(call, false);
        let args = self.check_args_with_arguments(
            RejectionSite::SourceFunctionArgumentCount,
            &member.params,
            &call.args,
            fx,
            pos,
            name.sym.as_ref(),
            None,
            false,
        );
        let defaults = (call.args.len() < member.params.len()).then(|| hir::LambdaDefaults {
            id: member.id,
            params: Vec::new(),
        });
        let value = hir::Expr {
            pending_work: None,
            ty: signature.ret.clone(),
            kind: hir::ExprKind::Call {
                callee: hir::Callee::LambdaSelf {
                    name: std::sync::Arc::new(member.name.clone()),
                    id: member.id,
                    signature,
                    is_async: member.is_async,
                    defaults,
                },
                args,
            },
            pos: pos.clone(),
        };
        Some(self.track_async_call_result(value, fx))
    }
}

pub(super) fn members(module: &hir::Module) -> HashMap<hir::LambdaId, &hir::Expr> {
    fn visit<'a>(child: hir::HirChild<'a>, out: &mut HashMap<hir::LambdaId, &'a hir::Expr>) {
        let children = match child {
            hir::HirChild::Expr(e) => {
                if let hir::ExprKind::Lambda {
                    id, group: Some(_), ..
                } = e.kind
                {
                    out.insert(id, e);
                }
                e.children()
            }
            hir::HirChild::Stmt(s) => s.children(),
        };
        for child in children {
            visit(child, out);
        }
    }
    let mut out = HashMap::new();
    for owner in module.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(e) => visit(hir::HirChild::Expr(e), &mut out),
            hir::ExpressionOwner::Body { statements, .. } => {
                for s in statements {
                    visit(hir::HirChild::Stmt(s), &mut out);
                }
            }
        }
    }
    out
}

/// All members carry the capture union and ownership of their group (§293 rules 4 and 5).
fn normalizer(
    members: HashMap<hir::LambdaId, &hir::Expr>,
) -> impl Fn(hir::HirChildMut<'_>) + use<> {
    let identities = linked_groups(&members);
    let mut groups: HashMap<hir::LambdaId, (Vec<hir::Capture>, bool)> = HashMap::new();
    let mut defaults = HashMap::new();
    let mut ordered: Vec<_> = members.into_iter().collect();
    ordered.sort_by_key(|(id, _)| id.0);
    for (id, e) in ordered {
        if let hir::ExprKind::Lambda {
            captures,
            owns_environment,
            params,
            ..
        } = &e.kind
        {
            defaults.insert(id, params.clone());
            let Some(group) = identities.get(&id).copied().flatten() else {
                continue;
            };
            let (union, owned) = groups.entry(group).or_default();
            *owned |= owns_environment;
            for capture in captures {
                if !union.iter().any(|c| c.name == capture.name) {
                    union.push(capture.clone());
                }
            }
        }
    }
    fn patch(
        child: hir::HirChildMut<'_>,
        groups: &HashMap<hir::LambdaId, (Vec<hir::Capture>, bool)>,
        defaults: &HashMap<hir::LambdaId, Vec<hir::Param>>,
        identities: &HashMap<hir::LambdaId, Option<hir::LambdaId>>,
    ) {
        let children = match child {
            hir::HirChildMut::Expr(e) => {
                if let hir::ExprKind::Lambda {
                    id,
                    group,
                    captures,
                    owns_environment,
                    ..
                } = &mut e.kind
                {
                    if let Some(identity) = identities.get(id) {
                        *group = *identity;
                    }
                    if let Some((union, owned)) = group.and_then(|group| groups.get(&group)) {
                        *captures = union.clone();
                        *owns_environment = *owned;
                    }
                }
                if let hir::ExprKind::Call {
                    callee:
                        hir::Callee::LambdaSelf {
                            defaults: Some(d), ..
                        },
                    ..
                } = &mut e.kind
                {
                    if let Some(params) = defaults.get(&d.id) {
                        d.params = params.clone();
                    }
                }
                e.children_mut()
            }
            hir::HirChildMut::Stmt(s) => s.children_mut(),
        };
        for child in children {
            patch(child, groups, defaults, identities);
        }
    }
    move |child| patch(child, &groups, &defaults, &identities)
}

/// Resolve candidate runs from checked name references before capture or ownership propagation (§293 rule 1).
fn linked_groups(
    members: &HashMap<hir::LambdaId, &hir::Expr>,
) -> HashMap<hir::LambdaId, Option<hir::LambdaId>> {
    fn reads(child: hir::HirChild<'_>, visit: &mut impl FnMut(hir::LambdaId, bool)) {
        let children = match child {
            hir::HirChild::Expr(e) => {
                match &e.kind {
                    hir::ExprKind::Lambda { .. } => return,
                    hir::ExprKind::LambdaMember(id) => visit(*id, true),
                    hir::ExprKind::Call {
                        callee: hir::Callee::LambdaSelf { id, .. },
                        ..
                    } => visit(*id, false),
                    _ => {}
                }
                e.children()
            }
            hir::HirChild::Stmt(s) => s.children(),
        };
        for child in children {
            reads(child, visit);
        }
    }
    let mut links: HashMap<hir::LambdaId, Vec<hir::LambdaId>> = HashMap::new();
    for (&id, expr) in members {
        let hir::ExprKind::Lambda { group, .. } = expr.kind else {
            continue;
        };
        for child in expr.children() {
            reads(child, &mut |target, value| {
                if target.0 < id.0 || (target == id && !value) {
                    return;
                }
                if members.get(&target).is_some_and(|peer| {
                    matches!(peer.kind, hir::ExprKind::Lambda { group: peer_group, .. } if peer_group == group)
                }) {
                    links.entry(id).or_default().push(target);
                    links.entry(target).or_default().push(id);
                }
            });
        }
    }
    let mut identities: HashMap<_, _> = members.keys().map(|&id| (id, None)).collect();
    let mut seen = HashSet::new();
    for &id in links.keys() {
        let mut component = Vec::new();
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            if seen.insert(id) {
                component.push(id);
                pending.extend(&links[&id]);
            }
        }
        if let Some(&group) = component.iter().min_by_key(|id| id.0) {
            for member in component {
                identities.insert(member, Some(group));
            }
        }
    }
    identities
}

/// Forward links decide whether an earlier const read is a group operation (§293 rules 1–3).
pub(super) fn resolve_earlier_reads(
    module: &mut hir::Module,
    reads: &HashMap<hir::LambdaId, Vec<EarlierRead>>,
) -> Vec<Diagnostic> {
    let identities = linked_groups(&members(module));
    let mut diagnostics = Vec::new();
    for owner in module.expression_owners_mut() {
        let roots = match owner {
            hir::ExpressionOwnerMut::Expr(e) => vec![hir::HirChildMut::Expr(e)],
            hir::ExpressionOwnerMut::Body(body) => {
                body.iter_mut().map(hir::HirChildMut::Stmt).collect()
            }
        };
        diagnostics.extend(resolve_reads(&identities, reads, roots));
    }
    diagnostics
}

fn resolve_reads<'a>(
    identities: &HashMap<hir::LambdaId, Option<hir::LambdaId>>,
    reads: &HashMap<hir::LambdaId, Vec<EarlierRead>>,
    roots: impl IntoIterator<Item = hir::HirChildMut<'a>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut grouped = HashMap::new();
    for (owner, references) in reads {
        let Some(Some(group)) = identities.get(owner) else {
            continue;
        };
        let references: Vec<_> = references
            .iter()
            .filter(|read| identities.get(&read.member.id) == Some(&Some(*group)))
            .collect();
        grouped.insert(*owner, references);
    }
    fn patch(
        child: hir::HirChildMut<'_>,
        owner: Option<hir::LambdaId>,
        grouped: &HashMap<hir::LambdaId, Vec<&EarlierRead>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (children, owner) = match child {
            hir::HirChildMut::Expr(e) => {
                let mut owner = owner;
                if let hir::ExprKind::Lambda { id, captures, .. } = &mut e.kind {
                    owner = Some(*id);
                    if let Some(references) = grouped.get(id) {
                        for read in references {
                            if read.nested {
                                diagnostics.push(diagnostic(
                                    read.rejection_site,
                                    format!(
                                        "`{}` is read before its declaration in this block",
                                        read.member.name
                                    ),
                                    read.pos.clone(),
                                ));
                            } else if !read.decided_result {
                                // §293 rule 6 also applies to a member declared before its caller.
                                diagnostics.push(diagnostic(
                                    RejectionSite::InitializerTypeCycle,
                                    "initializer type depends on its own undecided type",
                                    read.pos.clone(),
                                ));
                            }
                        }
                        captures.retain(|capture| {
                            !references
                                .iter()
                                .any(|read| read.member.name == capture.name)
                        });
                    }
                }
                let references = owner.and_then(|id| grouped.get(&id));
                let reference = |pos: &Pos| {
                    references.and_then(|reads| reads.iter().find(|read| read.pos == *pos))
                };
                match &mut e.kind {
                    hir::ExprKind::Local(..) => {
                        if let Some(read) = reference(&e.pos) {
                            e.kind = hir::ExprKind::LambdaMember(read.member.id);
                        }
                    }
                    hir::ExprKind::Call { callee, .. } => {
                        let value = match callee {
                            hir::Callee::Value(value) | hir::Callee::NamedLambda { value, .. } => {
                                Some(value)
                            }
                            _ => None,
                        };
                        if let Some(read) = value.and_then(|value| reference(&value.pos)) {
                            if let Some(signature) = &read.member.signature {
                                *callee = hir::Callee::LambdaSelf {
                                    name: std::sync::Arc::new(read.member.name.clone()),
                                    id: read.member.id,
                                    signature: signature.clone(),
                                    is_async: read.member.is_async,
                                    defaults: callee.lambda_defaults().cloned(),
                                };
                            }
                        }
                    }
                    _ => {}
                }
                (e.children_mut(), owner)
            }
            hir::HirChildMut::Stmt(s) => (s.children_mut(), owner),
        };
        for child in children {
            patch(child, owner, grouped, diagnostics);
        }
    }
    for child in roots {
        patch(child, None, &grouped, &mut diagnostics);
    }
    diagnostics
}

pub(super) fn normalize(module: &mut hir::Module) {
    let patch = normalizer(members(module));
    for owner in module.expression_owners_mut() {
        match owner {
            hir::ExpressionOwnerMut::Expr(e) => patch(hir::HirChildMut::Expr(e)),
            hir::ExpressionOwnerMut::Body(body) => {
                for s in body {
                    patch(hir::HirChildMut::Stmt(s));
                }
            }
        }
    }
}

impl Checker<'_> {
    /// Complete forward-member defaults before initialization-effect analysis (§293, §291).
    pub(super) fn finish_group_defaults(&mut self) {
        fn gather(
            child: hir::HirChild<'_>,
            defaults: &mut HashMap<hir::LambdaId, Vec<hir::Param>>,
        ) {
            let children = match child {
                hir::HirChild::Expr(e) => {
                    if let hir::ExprKind::Lambda {
                        id,
                        group: Some(_),
                        params,
                        ..
                    } = &e.kind
                    {
                        defaults.insert(*id, params.clone());
                    }
                    e.children()
                }
                hir::HirChild::Stmt(s) => s.children(),
            };
            for child in children {
                gather(child, defaults);
            }
        }
        fn patch(child: hir::HirChildMut<'_>, defaults: &HashMap<hir::LambdaId, Vec<hir::Param>>) {
            let children = match child {
                hir::HirChildMut::Expr(e) => {
                    if let hir::ExprKind::Call {
                        callee:
                            hir::Callee::LambdaSelf {
                                defaults: Some(d), ..
                            },
                        ..
                    } = &mut e.kind
                    {
                        if let Some(params) = defaults.get(&d.id) {
                            d.params = params.clone();
                        }
                    }
                    e.children_mut()
                }
                hir::HirChildMut::Stmt(s) => s.children_mut(),
            };
            for child in children {
                patch(child, defaults);
            }
        }
        let mut defaults = HashMap::new();
        for f in self.functions.iter().chain(
            self.classes
                .iter()
                .flat_map(|c| c.ctor.iter().chain(&c.methods)),
        ) {
            for p in &f.params {
                if let Some(e) = &p.default {
                    gather(hir::HirChild::Expr(e), &mut defaults);
                }
            }
            for s in &f.body {
                gather(hir::HirChild::Stmt(s), &mut defaults);
            }
        }
        for e in self.globals.iter().map(|g| &g.init).chain(
            self.classes
                .iter()
                .flat_map(|c| c.fields.iter().filter_map(|f| f.init.as_ref())),
        ) {
            gather(hir::HirChild::Expr(e), &mut defaults);
        }
        for s in &self.top_level {
            gather(hir::HirChild::Stmt(s), &mut defaults);
        }
        for f in self.functions.iter_mut().chain(
            self.classes
                .iter_mut()
                .flat_map(|c| c.ctor.iter_mut().chain(&mut c.methods)),
        ) {
            for p in &mut f.params {
                if let Some(e) = &mut p.default {
                    patch(hir::HirChildMut::Expr(e), &defaults);
                }
            }
            for s in &mut f.body {
                patch(hir::HirChildMut::Stmt(s), &defaults);
            }
        }
        for e in self.globals.iter_mut().map(|g| &mut g.init).chain(
            self.classes
                .iter_mut()
                .flat_map(|c| c.fields.iter_mut().filter_map(|f| f.init.as_mut())),
        ) {
            patch(hir::HirChildMut::Expr(e), &defaults);
        }
        for s in &mut self.top_level {
            patch(hir::HirChildMut::Stmt(s), &defaults);
        }
    }
}

pub(super) fn normalize_body(
    body: &mut [hir::Stmt],
    reads: &HashMap<hir::LambdaId, Vec<EarlierRead>>,
) -> Vec<Diagnostic> {
    fn visit<'a>(child: hir::HirChild<'a>, members: &mut HashMap<hir::LambdaId, &'a hir::Expr>) {
        let children = match child {
            hir::HirChild::Expr(e) => {
                if let hir::ExprKind::Lambda {
                    id, group: Some(_), ..
                } = e.kind
                {
                    members.insert(id, e);
                }
                e.children()
            }
            hir::HirChild::Stmt(s) => s.children(),
        };
        for child in children {
            visit(child, members);
        }
    }
    let mut members = HashMap::new();
    for s in body.iter() {
        visit(hir::HirChild::Stmt(s), &mut members);
    }
    let identities = linked_groups(&members);
    drop(members);
    let diagnostics = resolve_reads(
        &identities,
        reads,
        body.iter_mut().map(hir::HirChildMut::Stmt),
    );
    let mut members = HashMap::new();
    for s in body.iter() {
        visit(hir::HirChild::Stmt(s), &mut members);
    }
    let patch = normalizer(members);
    for s in body {
        patch(hir::HirChildMut::Stmt(s));
    }
    diagnostics
}
