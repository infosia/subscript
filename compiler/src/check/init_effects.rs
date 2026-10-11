//! Checks initializer read routes under compiler.md §137 rules 5, 5a, and 5b.

use super::*;
use crate::check::rejection::{diagnostic, RejectionFailure, RejectionSite};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ModuleFunction {
    Free(hir::Symbol),
    Constructor(ClassId),
    Method(ClassId, hir::Symbol),
    Lambda(hir::LambdaId),
    Default(Box<ModuleFunction>, usize),
}

#[derive(Clone, Default)]
struct ModuleEffects {
    accesses: std::collections::BTreeSet<String>,
    calls: Vec<(ModuleFunction, String)>,
    made: Vec<ModuleFunction>,
    indirect: bool,
    lambdas: HashMap<ModuleFunction, ModuleEffects>,
}

struct ModuleEffectScanner<'a> {
    classes: &'a [hir::ClassDef],
    generators: Option<&'a HashSet<ModuleFunction>>,
    defaults: Option<&'a HashMap<ModuleFunction, Vec<(usize, String)>>>,
    ambiguous_class_names: Option<&'a HashSet<&'a str>>,
    effects: ModuleEffects,
}

impl<'a> ModuleEffectScanner<'a> {
    fn new(classes: &'a [hir::ClassDef]) -> Self {
        Self {
            classes,
            generators: None,
            defaults: None,
            ambiguous_class_names: None,
            effects: ModuleEffects::default(),
        }
    }

    fn with_generators(mut self, generators: &'a HashSet<ModuleFunction>) -> Self {
        self.generators = Some(generators);
        self
    }

    fn function(mut self, function: &hir::Function) -> ModuleEffects {
        self.stmts(&function.body);
        self.effects
    }

    fn constructor(mut self, class: &hir::ClassDef) -> ModuleEffects {
        for field in &class.fields {
            if let Some(initializer) = &field.init {
                self.expr(initializer);
            }
        }
        if let Some(constructor) = &class.ctor {
            self.stmts(&constructor.body);
        }
        self.effects
    }

    fn with_defaults(
        mut self,
        defaults: &'a HashMap<ModuleFunction, Vec<(usize, String)>>,
    ) -> Self {
        self.defaults = Some(defaults);
        self
    }

    fn with_class_labels(mut self, names: &'a HashSet<&'a str>) -> Self {
        self.ambiguous_class_names = Some(names);
        self
    }

    fn class_member_label(&self, class: &hir::ClassDef, member: &str) -> String {
        let Some(names) = self.ambiguous_class_names else {
            return identity::class_member_label(self.classes, class, member);
        };
        hir::declaration_label(
            &format!("{}.{}", class.name, source_name(member)),
            &class.pos,
            names.contains(class.name.as_str()),
        )
    }

    fn call_defaults(&mut self, unit: ModuleFunction, supplied: usize) {
        let defaults = self.defaults.and_then(|defaults| defaults.get(&unit));
        for (index, label) in defaults.into_iter().flatten() {
            if *index >= supplied {
                self.record_call(
                    ModuleFunction::Default(Box::new(unit.clone()), *index),
                    label.clone(),
                );
            }
        }
    }

    fn record_access(&mut self, name: &str) {
        self.effects.accesses.insert(name.to_string());
    }

    fn record_indirect_call(&mut self) {
        self.effects.indirect = true;
    }

    fn record_call(&mut self, function: ModuleFunction, label: String) {
        if self
            .generators
            .is_some_and(|units| units.contains(&function))
        {
            self.effects.made.push(function);
        } else {
            self.effects.calls.push((function, label));
        }
    }

    fn callback(&mut self, args: &[hir::Expr]) {
        match args.get(1).map(|arg| &arg.kind) {
            Some(hir::ExprKind::Lambda { id, .. }) => {
                self.record_call(ModuleFunction::Lambda(*id), "[lambda]".to_string())
            }
            Some(hir::ExprKind::FuncRef(name)) => self.record_call(
                ModuleFunction::Free(name.clone()),
                name.full_text().to_string(),
            ),
            Some(_) => self.record_indirect_call(),
            None => self.record_indirect_call(),
        }
    }

    fn class_of(ty: &Type) -> Option<ClassId> {
        match ty {
            Type::Class(id) => Some(*id),
            Type::Nullable(inner) => Self::class_of(inner),
            _ => None,
        }
    }

    fn method_call(&mut self, receiver: &hir::Expr, name: &hir::Symbol) {
        let Some(class_id) = Self::class_of(&receiver.ty) else {
            self.record_indirect_call();
            return;
        };
        let Some(class) = self.classes.get(class_id.0) else {
            self.record_indirect_call();
            return;
        };
        if class.methods.iter().any(|method| method.symbol == *name) {
            self.record_call(
                ModuleFunction::Method(class_id, name.clone()),
                self.class_member_label(class, name.full_text()),
            );
        } else {
            self.record_indirect_call();
        }
    }

    fn async_callee(&mut self, callee: &hir::AsyncCallee) {
        match callee {
            hir::AsyncCallee::Function(name) => {
                self.record_call(
                    ModuleFunction::Free(name.clone()),
                    name.full_text().to_owned(),
                );
            }
            hir::AsyncCallee::Method {
                class,
                receiver: _,
                name,
            } => {
                let label = self.classes.get(class.0).map_or_else(
                    || name.full_text().to_owned(),
                    |definition| self.class_member_label(definition, name.full_text()),
                );
                self.record_call(ModuleFunction::Method(*class, name.clone()), label);
            }
        }
    }

    fn stmts(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.stmt(statement);
        }
    }

    fn stmt(&mut self, statement: &hir::Stmt) {
        use hir::Stmt as S;
        // compiler.md §137 rule 5b: every statement participates in the scan.
        match statement {
            S::Let { .. }
            | S::Expr(_)
            | S::Return { .. }
            | S::If {
                cond: _,
                then: _,
                els: _,
                pos: _,
            }
            | S::While {
                cond: _,
                body: _,
                pos: _,
            }
            | S::For {
                init: _,
                cond: _,
                step: _,
                body: _,
                pos: _,
            }
            | S::ForOf {
                name: _,
                ty: _,
                subject: _,
                kind: _,
                body: _,
                pos: _,
            }
            | S::GeneratorForOf {
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                body: _,
                pos: _,
            }
            | S::Switch {
                disc: _,
                cases: _,
                pos: _,
            }
            | S::Break(_)
            | S::Continue(_)
            | S::Block(_)
            | S::Throw { .. }
            | S::Try {
                body: _,
                binding: _,
                handler: _,
                pos: _,
            }
            | S::Using {
                bindings: _,
                body: _,
                finalizer: _,
                pos: _,
            } => {
                for child in statement.children() {
                    match child {
                        hir::HirChild::Expr(expression) => self.expr(expression),
                        hir::HirChild::Stmt(statement) => self.stmt(statement),
                    }
                }
            }
        }
        if matches!(
            statement,
            S::GeneratorForOf {
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                body: _,
                pos: _
            }
        ) {
            self.record_indirect_call();
        }
        if let S::Using {
            bindings,
            body: _,
            finalizer: _,
            pos: _,
        } = statement
        {
            for binding in bindings.iter().rev() {
                self.stmt(&binding.hook());
            }
        }
    }

    fn callee(&mut self, callee: &hir::Callee, args: &[hir::Expr]) {
        use hir::Callee as C;
        match callee {
            C::Func(name) => {
                self.record_call(
                    ModuleFunction::Free(name.clone()),
                    name.full_text().to_owned(),
                );
            }
            C::Standard(_) | C::Foreign(_) | C::Value(_) => self.record_indirect_call(),
            C::Method { recv, name } => {
                if let Some((hir::OperationSignatureTarget::BuiltinMethod(method), _)) =
                    hir::operation_signature_target(callee)
                {
                    // compiler.md §137 rule 5b: a generator step is indirect.
                    match method {
                        hir::BuiltinMethod::ArrayPush
                        | hir::BuiltinMethod::ArrayPop
                        | hir::BuiltinMethod::ArrayClear
                        | hir::BuiltinMethod::StringSlice => {}
                        hir::BuiltinMethod::GeneratorNext => self.record_indirect_call(),
                    }
                } else {
                    self.method_call(recv, name);
                }
            }
            C::Ambient(function) => {
                if match function {
                    hir::AmbientFn::Print
                    | hir::AmbientFn::Unreachable
                    | hir::AmbientFn::Collect
                    | hir::AmbientFn::UnsafeDelete => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::ContextBytes { function, .. } => {
                if match function {
                    hir::ContextBytesFn::BytesOf
                    | hir::ContextBytesFn::BytesInto
                    | hir::ContextBytesFn::FromBytes => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Math(function) => {
                if match function {
                    hir::MathFn::Abs
                    | hir::MathFn::Acos
                    | hir::MathFn::Acosh
                    | hir::MathFn::Asin
                    | hir::MathFn::Asinh
                    | hir::MathFn::Atan
                    | hir::MathFn::Atanh
                    | hir::MathFn::Cbrt
                    | hir::MathFn::Ceil
                    | hir::MathFn::Cos
                    | hir::MathFn::Cosh
                    | hir::MathFn::Exp
                    | hir::MathFn::Expm1
                    | hir::MathFn::Floor
                    | hir::MathFn::Log
                    | hir::MathFn::Log1p
                    | hir::MathFn::Log10
                    | hir::MathFn::Log2
                    | hir::MathFn::Round
                    | hir::MathFn::Sign
                    | hir::MathFn::Sin
                    | hir::MathFn::Sinh
                    | hir::MathFn::Sqrt
                    | hir::MathFn::Tan
                    | hir::MathFn::Tanh
                    | hir::MathFn::Trunc
                    | hir::MathFn::Atan2
                    | hir::MathFn::Hypot
                    | hir::MathFn::Pow
                    | hir::MathFn::Max
                    | hir::MathFn::Min
                    | hir::MathFn::Random
                    | hir::MathFn::Clz32
                    | hir::MathFn::Imul
                    | hir::MathFn::Fround
                    | hir::MathFn::F32ToBits
                    | hir::MathFn::F32FromBits => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Num(function) => {
                if match function {
                    hir::NumFn::IsNaN
                    | hir::NumFn::IsFinite
                    | hir::NumFn::IsInteger
                    | hir::NumFn::IsSafeInteger
                    | hir::NumFn::ParseInt
                    | hir::NumFn::ParseFloat
                    | hir::NumFn::ToFixed
                    | hir::NumFn::ToStringF32
                    | hir::NumFn::ToStringF64
                    | hir::NumFn::ToExponential
                    | hir::NumFn::ToPrecision => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Date(function) => {
                if match function {
                    hir::DateFn::New
                    | hir::DateFn::Utc
                    | hir::DateFn::Now
                    | hir::DateFn::GetUtcFullYear
                    | hir::DateFn::GetUtcMonth
                    | hir::DateFn::GetUtcDate
                    | hir::DateFn::GetUtcDay
                    | hir::DateFn::GetUtcHours
                    | hir::DateFn::GetUtcMinutes
                    | hir::DateFn::GetUtcSeconds
                    | hir::DateFn::GetUtcMilliseconds
                    | hir::DateFn::ToIso
                    | hir::DateFn::ToUtcString => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Json(function) => {
                if match function {
                    hir::JsonFn::Begin
                    | hir::JsonFn::BeginTracked
                    | hir::JsonFn::Finish
                    | hir::JsonFn::Raw
                    | hir::JsonFn::Str
                    | hir::JsonFn::I32
                    | hir::JsonFn::U32
                    | hir::JsonFn::I64
                    | hir::JsonFn::U64
                    | hir::JsonFn::F32
                    | hir::JsonFn::F64
                    | hir::JsonFn::Bool
                    | hir::JsonFn::Date
                    | hir::JsonFn::Null
                    | hir::JsonFn::Visit
                    | hir::JsonFn::Leave
                    | hir::JsonFn::ParseBegin
                    | hir::JsonFn::ParseEnd
                    | hir::JsonFn::ParseRoot
                    | hir::JsonFn::ParseIsKind
                    | hir::JsonFn::ParseNumberFits
                    | hir::JsonFn::ParseNumber
                    | hir::JsonFn::ParseInteger
                    | hir::JsonFn::ParseBool
                    | hir::JsonFn::ParseString
                    | hir::JsonFn::ParseArrayLen
                    | hir::JsonFn::ParseArrayGet
                    | hir::JsonFn::ParseObjectGet
                    | hir::JsonFn::ParseFailure => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Text(function) => {
                if match function {
                    hir::TextFn::ErrorToString
                    | hir::TextFn::EncodeUri
                    | hir::TextFn::EncodeComponent
                    | hir::TextFn::DecodeUri
                    | hir::TextFn::DecodeComponent
                    | hir::TextFn::UriFailure
                    | hir::TextFn::ComponentFailure => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Str(function) => {
                if match function {
                    hir::StrFn::Slice
                    | hir::StrFn::IndexOf
                    | hir::StrFn::LastIndexOf
                    | hir::StrFn::Includes
                    | hir::StrFn::StartsWith
                    | hir::StrFn::EndsWith
                    | hir::StrFn::CharCodeAt
                    | hir::StrFn::Split
                    | hir::StrFn::Trim
                    | hir::StrFn::TrimStart
                    | hir::StrFn::TrimEnd
                    | hir::StrFn::Repeat
                    | hir::StrFn::PadStart
                    | hir::StrFn::PadEnd
                    | hir::StrFn::ToUpperCase
                    | hir::StrFn::ToLowerCase
                    | hir::StrFn::Replace
                    | hir::StrFn::ReplaceAll
                    | hir::StrFn::Substring
                    | hir::StrFn::Substr
                    | hir::StrFn::CharAt
                    | hir::StrFn::CodePointAt
                    | hir::StrFn::Concat
                    | hir::StrFn::At
                    | hir::StrFn::Normalize
                    | hir::StrFn::GraphemeLength
                    | hir::StrFn::SliceGraphemes => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Regex(function) => {
                if match function {
                    hir::RegexFn::New
                    | hir::RegexFn::Test
                    | hir::RegexFn::Source
                    | hir::RegexFn::Flags
                    | hir::RegexFn::Search
                    | hir::RegexFn::Replace
                    | hir::RegexFn::ReplaceAll
                    | hir::RegexFn::Split
                    | hir::RegexFn::MatchStart
                    | hir::RegexFn::MatchEnd
                    | hir::RegexFn::Global
                    | hir::RegexFn::IgnoreCase
                    | hir::RegexFn::Multiline
                    | hir::RegexFn::DotAll
                    | hir::RegexFn::Unicode
                    | hir::RegexFn::HasIndices
                    | hir::RegexFn::Sticky
                    | hir::RegexFn::ToString => false,
                } {
                    self.record_indirect_call();
                }
            }
            C::Arr(function) => {
                if match function {
                    hir::ArrFn::ForEach
                    | hir::ArrFn::Map
                    | hir::ArrFn::Filter
                    | hir::ArrFn::Reduce
                    | hir::ArrFn::Some
                    | hir::ArrFn::Every
                    | hir::ArrFn::FindIndex
                    | hir::ArrFn::Sort
                    | hir::ArrFn::ReduceRight
                    | hir::ArrFn::Find
                    | hir::ArrFn::FindLast
                    | hir::ArrFn::FindLastIndex
                    | hir::ArrFn::FlatMap => true,
                    hir::ArrFn::IndexOf
                    | hir::ArrFn::LastIndexOf
                    | hir::ArrFn::Includes
                    | hir::ArrFn::Join
                    | hir::ArrFn::Slice
                    | hir::ArrFn::Fill
                    | hir::ArrFn::Reverse
                    | hir::ArrFn::Concat
                    | hir::ArrFn::Splice
                    | hir::ArrFn::Shift
                    | hir::ArrFn::Unshift
                    | hir::ArrFn::CopyWithin
                    | hir::ArrFn::At => false,
                } {
                    self.callback(args);
                }
            }
            C::Map(function) => {
                if match function {
                    hir::MapFn::ForEach | hir::MapFn::GroupBy => true,
                    hir::MapFn::New
                    | hir::MapFn::Size
                    | hir::MapFn::Get
                    | hir::MapFn::GetOr
                    | hir::MapFn::Set
                    | hir::MapFn::Has
                    | hir::MapFn::Delete
                    | hir::MapFn::Clear => false,
                } {
                    self.callback(args);
                }
            }
            C::Set(function) => {
                if match function {
                    hir::SetFn::ForEach => true,
                    hir::SetFn::New
                    | hir::SetFn::Size
                    | hir::SetFn::Add
                    | hir::SetFn::Has
                    | hir::SetFn::Delete
                    | hir::SetFn::Clear
                    | hir::SetFn::Union
                    | hir::SetFn::Intersection
                    | hir::SetFn::Difference
                    | hir::SetFn::SymmetricDifference
                    | hir::SetFn::IsSubsetOf
                    | hir::SetFn::IsSupersetOf
                    | hir::SetFn::IsDisjointFrom => false,
                } {
                    self.callback(args);
                }
            }
            C::Worker(function) => {
                if match function {
                    hir::WorkerFn::Spawn(..)
                    | hir::WorkerFn::Post
                    | hir::WorkerFn::Poll
                    | hir::WorkerFn::Close
                    | hir::WorkerFn::Join
                    | hir::WorkerFn::InboxWait
                    | hir::WorkerFn::InboxPoll
                    | hir::WorkerFn::OutboxPost => false,
                } {
                    self.record_indirect_call();
                }
            }
        }
    }

    fn expr(&mut self, expression: &hir::Expr) {
        use hir::ExprKind as K;

        match &expression.kind {
            K::Lambda { id, body, .. } => {
                let unit = ModuleFunction::Lambda(*id);
                let mut scanner = Self::new(self.classes);
                scanner.generators = self.generators;
                scanner.defaults = self.defaults;
                scanner.ambiguous_class_names = self.ambiguous_class_names;
                scanner.stmts(body);
                self.effects
                    .lambdas
                    .extend(std::mem::take(&mut scanner.effects.lambdas));
                self.effects.lambdas.insert(unit.clone(), scanner.effects);
                self.effects.made.push(unit);
                return;
            }
            K::DescriptorLit { class, fields } => {
                if let Some(definition) = self.classes.get(class.0) {
                    for (index, field) in fields.iter().enumerate() {
                        if let Some(value) = field.as_ref().or_else(|| {
                            definition
                                .fields
                                .get(index)
                                .and_then(|field| field.init.as_ref())
                        }) {
                            self.expr(value);
                        }
                    }
                } else {
                    for field in fields.iter().flatten() {
                        self.expr(field);
                    }
                    self.record_indirect_call();
                }
                return;
            }
            K::Global(name) => self.record_access(name.full_text()),
            K::FuncRef(name) => self.effects.made.push(ModuleFunction::Free(name.clone())),
            K::Call { callee, args } => {
                let unit = match callee {
                    hir::Callee::Func(name) => Some(ModuleFunction::Free(name.clone())),
                    hir::Callee::Method { recv, name } => {
                        Self::class_of(&recv.ty).map(|id| ModuleFunction::Method(id, name.clone()))
                    }
                    _ => None,
                };
                if let Some(unit) = unit {
                    self.call_defaults(unit, args.len());
                }
                self.callee(callee, args);
            }
            K::New { class, args } => {
                self.call_defaults(ModuleFunction::Constructor(*class), args.len());
                let label = self.classes.get(class.0).map_or_else(
                    || "constructor".to_string(),
                    |definition| self.class_member_label(definition, "constructor"),
                );
                self.record_call(ModuleFunction::Constructor(*class), label);
            }
            K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
                let unit = match callee {
                    hir::AsyncCallee::Function(name) => ModuleFunction::Free(name.clone()),
                    hir::AsyncCallee::Method { class, name, .. } => {
                        ModuleFunction::Method(*class, name.clone())
                    }
                };
                self.call_defaults(unit, args.len());
                self.async_callee(callee)
            }
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(..)
            | K::EnumMember { .. }
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Assign { .. }
            | K::Cast(_)
            | K::Zero
            | K::Unassigned
            | K::RawNew { .. }
            | K::Field { .. }
            | K::Length(_)
            | K::Index { .. }
            | K::ArrayLit(_)
            | K::ArraySpreadLit(_)
            | K::Template(_)
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncHandleAwait(_)
            | K::TaskGroup { .. }
            | K::AsyncAll { .. }
            | K::AsyncHandleTransfer { .. }
            | K::Cond { .. } => {}
        }
        for child in expression.children() {
            match child {
                hir::HirChild::Expr(expression) => self.expr(expression),
                hir::HirChild::Stmt(statement) => self.stmt(statement),
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct ModuleRoute {
    parent: Option<usize>,
    label: Option<String>,
}

#[derive(Debug, PartialEq)]
struct ModuleReads {
    accesses: BTreeMap<String, usize>,
    routes: Vec<ModuleRoute>,
    #[cfg(test)]
    queue_pops: usize,
}

impl ModuleReads {
    fn route(&self, mut index: usize) -> Vec<&str> {
        let mut route = Vec::new();
        loop {
            let link = &self.routes[index];
            if let Some(label) = &link.label {
                route.push(label.as_str());
            }
            let Some(parent) = link.parent else { break };
            index = parent;
        }
        route.reverse();
        route
    }
}

struct ModuleItemScan<'a> {
    summaries: &'a HashMap<ModuleFunction, ModuleEffects>,
    memo: HashSet<ModuleFunction>,
    queue: std::collections::VecDeque<(&'a ModuleEffects, usize)>,
    reads: ModuleReads,
}

impl<'a> ModuleItemScan<'a> {
    fn enqueue(
        &mut self,
        unit: &ModuleFunction,
        parent: usize,
        label: Option<&str>,
    ) -> Result<(), RejectionFailure> {
        let summary = self.summaries.get(unit).ok_or_else(|| {
            RejectionFailure::new(
                RejectionSite::InitializerMissingSummary,
                "internal error: made function value has no summary".to_string(),
            )
        })?;
        if self.memo.insert(unit.clone()) {
            let index = self.reads.routes.len();
            self.reads.routes.push(ModuleRoute {
                parent: Some(parent),
                label: label.map(str::to_string),
            });
            self.queue.push_back((summary, index));
        }
        Ok(())
    }
}

#[derive(Default)]
struct ModuleRouteScan {
    made: HashSet<ModuleFunction>,
    made_order: Vec<ModuleFunction>,
}

impl ModuleRouteScan {
    fn resolve<'a>(
        &mut self,
        effects: &'a ModuleEffects,
        summaries: &'a HashMap<ModuleFunction, ModuleEffects>,
    ) -> Result<ModuleReads, RejectionFailure> {
        let mut item = ModuleItemScan {
            summaries,
            memo: HashSet::new(),
            queue: std::collections::VecDeque::from([(effects, 0)]),
            reads: ModuleReads {
                accesses: BTreeMap::new(),
                routes: vec![ModuleRoute {
                    parent: None,
                    label: None,
                }],
                #[cfg(test)]
                queue_pops: 0,
            },
        };
        let mut indirect_route = None;
        while let Some((summary, route)) = item.queue.pop_front() {
            #[cfg(test)]
            {
                item.reads.queue_pops += 1;
            }
            for name in &summary.accesses {
                item.reads.accesses.entry(name.clone()).or_insert(route);
            }
            for unit in &summary.made {
                if self.made.insert(unit.clone()) {
                    self.made_order.push(unit.clone());
                    if let Some(parent) = indirect_route {
                        item.enqueue(unit, parent, None)?;
                    }
                }
            }
            let mut indirect = summary.indirect;
            for (unit, label) in &summary.calls {
                if summaries.contains_key(unit) {
                    item.enqueue(unit, route, Some(label))?;
                } else {
                    indirect = true;
                }
            }
            if indirect && indirect_route.is_none() {
                let parent = item.reads.routes.len();
                item.reads.routes.push(ModuleRoute {
                    parent: Some(route),
                    label: Some("[indirect call]".to_string()),
                });
                for unit in &self.made_order {
                    item.enqueue(unit, parent, None)?;
                }
                indirect_route = Some(parent);
            }
        }
        Ok(item.reads)
    }
}

fn insert_module_summary(
    unit: ModuleFunction,
    mut effects: ModuleEffects,
    summaries: &mut HashMap<ModuleFunction, ModuleEffects>,
) {
    summaries.extend(std::mem::take(&mut effects.lambdas));
    summaries.insert(unit, effects);
}

fn module_data_bindings(
    checker: &Checker<'_>,
    file_order: &[usize],
    file_segments: &[Option<hir::InitializerSegment>],
) -> Vec<String> {
    let mut bindings = Vec::new();
    for &file in file_order {
        if let Some(segment) = &file_segments[file] {
            let mut indices = segment.globals.clone();
            indices.sort_by_key(|&index| (checker.globals[index].initializer_index, index));
            bindings.extend(
                indices
                    .into_iter()
                    .map(|index| checker.globals[index].symbol.full_text().to_string()),
            );
        }
    }
    bindings
}

fn validate_module_segment(
    segment: &hir::InitializerSegment,
    globals: &[hir::Global],
    statements: usize,
) -> Result<(), RejectionFailure> {
    if segment.top_level.start > segment.top_level.end || segment.top_level.end > statements {
        return Err(RejectionFailure::new(
            RejectionSite::InitializerSegmentRange,
            "internal error: initializer segment is outside the module body".to_string(),
        ));
    }
    for &index in &segment.globals {
        let global = globals.get(index).ok_or_else(|| {
            RejectionFailure::new(
                RejectionSite::InitializerMissingGlobal,
                "internal error: initializer owner names a missing global".to_string(),
            )
        })?;
        if !(segment.top_level.start..=segment.top_level.end).contains(&global.initializer_index) {
            return Err(RejectionFailure::new(
                RejectionSite::GlobalInitializerOwnerRange,
                "internal error: global initializer is outside its owner range".to_string(),
            ));
        }
    }
    Ok(())
}

pub(super) fn module_initializer_diagnostics(
    checker: &Checker<'_>,
    file_order: &[usize],
    file_segments: &[Option<hir::InitializerSegment>],
) -> Vec<Diagnostic> {
    for &file in file_order {
        if let Some(segment) = &file_segments[file] {
            if let Err(message) =
                validate_module_segment(segment, &checker.globals, checker.top_level.len())
            {
                return vec![diagnostic(
                    message.site,
                    message.message,
                    Pos::new(&checker.prog.files[file].name, 1, 1),
                )];
            }
        }
    }
    let label = |symbol: &str| {
        identity::declaration_label(
            symbol,
            checker
                .globals
                .iter()
                .map(|g| (g.symbol.full_text(), g.name.as_str(), &g.pos))
                .chain(
                    checker
                        .functions
                        .iter()
                        .map(|f| (f.symbol.full_text(), f.name.as_str(), &f.pos)),
                ),
        )
    };
    let mut bindings = HashMap::new();
    for (index, name) in module_data_bindings(checker, file_order, file_segments)
        .into_iter()
        .enumerate()
    {
        bindings.entry(name).or_insert(index);
    }
    let mut class_names = HashSet::new();
    let mut ambiguous_class_names = HashSet::new();
    for class in &checker.classes {
        if !class_names.insert(class.name.as_str()) {
            ambiguous_class_names.insert(class.name.as_str());
        }
    }
    let generators: HashSet<_> = checker
        .functions
        .iter()
        .filter(|function| function.is_generator)
        .map(|function| ModuleFunction::Free(function.symbol.clone()))
        .chain(
            checker
                .classes
                .iter()
                .enumerate()
                .flat_map(|(index, class)| {
                    class
                        .methods
                        .iter()
                        .filter(|method| method.is_generator)
                        .map(move |method| {
                            ModuleFunction::Method(ClassId(index), method.symbol.clone())
                        })
                }),
        )
        .collect();
    let mut summaries = HashMap::new();
    let mut defaults = HashMap::new();
    let mut declaration_names = HashSet::new();
    let mut ambiguous_declaration_names = HashSet::new();
    for name in checker
        .globals
        .iter()
        .map(|g| g.name.as_str())
        .chain(checker.functions.iter().map(|f| f.name.as_str()))
    {
        if !declaration_names.insert(name) {
            ambiguous_declaration_names.insert(name);
        }
    }
    let default_function_labels: HashMap<_, _> = checker
        .functions
        .iter()
        .filter(|function| function.params.iter().any(|p| p.default.is_some()))
        .map(|function| {
            (
                &function.symbol,
                hir::declaration_label(
                    &function.name,
                    &function.pos,
                    ambiguous_declaration_names.contains(function.name.as_str()),
                ),
            )
        })
        .collect();
    let mut sources = Vec::new();
    for function in &checker.functions {
        sources.push((
            ModuleFunction::Free(function.symbol.clone()),
            &function.params,
        ));
    }
    for (index, class) in checker.classes.iter().enumerate() {
        if let Some(ctor) = &class.ctor {
            sources.push((ModuleFunction::Constructor(ClassId(index)), &ctor.params));
        }
        for method in &class.methods {
            sources.push((
                ModuleFunction::Method(ClassId(index), method.symbol.clone()),
                &method.params,
            ));
        }
    }
    for (unit, params) in &sources {
        defaults.insert(
            unit.clone(),
            params
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    p.default.as_ref().map(|_| {
                        let owner = match unit {
                            ModuleFunction::Free(name) => default_function_labels
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| source_name(name.full_text())),
                            ModuleFunction::Method(id, name) => {
                                let class = &checker.classes[id.0];
                                hir::declaration_label(
                                    &format!("{}.{}", class.name, source_name(name.full_text())),
                                    &class.pos,
                                    ambiguous_class_names.contains(class.name.as_str()),
                                )
                            }
                            ModuleFunction::Constructor(id) => {
                                let class = &checker.classes[id.0];
                                hir::declaration_label(
                                    &format!("{}.constructor", class.name),
                                    &class.pos,
                                    ambiguous_class_names.contains(class.name.as_str()),
                                )
                            }
                            ModuleFunction::Lambda(_) => "[lambda]".to_string(),
                            ModuleFunction::Default(_, _) => "[default]".to_string(),
                        };
                        (i, format!("{owner} (default of {})", p.name))
                    })
                })
                .collect(),
        );
    }
    for (unit, params) in sources {
        for (index, param) in params.iter().enumerate() {
            if let Some(value) = &param.default {
                let mut scanner = ModuleEffectScanner::new(&checker.classes)
                    .with_generators(&generators)
                    .with_defaults(&defaults)
                    .with_class_labels(&ambiguous_class_names);
                scanner.expr(value);
                insert_module_summary(
                    ModuleFunction::Default(Box::new(unit.clone()), index),
                    scanner.effects,
                    &mut summaries,
                );
            }
        }
    }

    for function in &checker.functions {
        let direct = ModuleEffectScanner::new(&checker.classes)
            .with_generators(&generators)
            .with_defaults(&defaults)
            .with_class_labels(&ambiguous_class_names)
            .function(function);
        insert_module_summary(
            ModuleFunction::Free(function.symbol.clone()),
            direct,
            &mut summaries,
        );
    }
    for (index, class) in checker.classes.iter().enumerate() {
        let class_id = ClassId(index);
        let constructor = ModuleEffectScanner::new(&checker.classes)
            .with_generators(&generators)
            .with_defaults(&defaults)
            .with_class_labels(&ambiguous_class_names)
            .constructor(class);
        insert_module_summary(
            ModuleFunction::Constructor(class_id),
            constructor,
            &mut summaries,
        );
        for method in &class.methods {
            let direct = ModuleEffectScanner::new(&checker.classes)
                .with_generators(&generators)
                .with_defaults(&defaults)
                .with_class_labels(&ambiguous_class_names)
                .function(method);
            insert_module_summary(
                ModuleFunction::Method(class_id, method.symbol.clone()),
                direct,
                &mut summaries,
            );
        }
    }

    let mut diagnostics = Vec::new();
    let mut initialized = HashSet::new();
    let mut scan = ModuleRouteScan::default();
    let mut check_effect = |mut scanner: ModuleEffectScanner<'_>,
                            pos: &Pos,
                            initialized: &HashSet<String>,
                            direct: &str| {
        summaries.extend(std::mem::take(&mut scanner.effects.lambdas));
        let reads = match scan.resolve(&scanner.effects, &summaries) {
            Ok(reads) => reads,
            Err(message) => {
                diagnostics.push(diagnostic(message.site, message.message, pos.clone()));
                return;
            }
        };
        let violation = reads
            .accesses
            .iter()
            .filter(|(binding, _)| !initialized.contains(*binding))
            .filter_map(|(binding, &route)| {
                bindings.get(binding).map(|&index| (index, binding, route))
            })
            .min_by_key(|&(index, _, _)| index);
        let Some((_, binding, route_index)) = violation else {
            return;
        };
        let path = reads.route(route_index);
        let route = if path.is_empty() {
            direct.to_string()
        } else {
            let path = path
                .iter()
                .map(|step| {
                    if *step == "[indirect call]" {
                        "an indirect call".to_string()
                    } else if *step == "[lambda]" {
                        "a lambda".to_string()
                    } else {
                        format!("`{}`", label(step))
                    }
                })
                .collect::<Vec<_>>()
                .join(" -> ");
            format!("through {path}")
        };
        let diagnostic = diagnostic(
            if path.is_empty() {
                RejectionSite::InitializerDirectRead
            } else {
                RejectionSite::InitializerRouteRead
            },
            format!(
                "`{}` is accessed before its declaration, {route}",
                label(binding)
            ),
            pos.clone(),
        );
        diagnostics.push(diagnostic);
    };
    // compiler.md §137 rules 5 and 5a: each file checks its source order.
    for &file in file_order {
        let Some(segment) = &file_segments[file] else {
            continue;
        };
        for statement_index in segment.top_level.start..=segment.top_level.end {
            for &global_index in &segment.globals {
                let global = &checker.globals[global_index];
                if global.initializer_index != statement_index {
                    continue;
                }
                {
                    let init = &global.init;
                    let mut scanner = ModuleEffectScanner::new(&checker.classes)
                        .with_generators(&generators)
                        .with_defaults(&defaults)
                        .with_class_labels(&ambiguous_class_names);
                    scanner.expr(init);
                    check_effect(
                        scanner,
                        &init.pos,
                        &initialized,
                        "directly from this initializer",
                    );
                }
                initialized.insert(global.symbol.full_text().to_string());
            }
            if statement_index < segment.top_level.end {
                let statement = &checker.top_level[statement_index];
                let mut scanner = ModuleEffectScanner::new(&checker.classes)
                    .with_generators(&generators)
                    .with_defaults(&defaults)
                    .with_class_labels(&ambiguous_class_names);
                scanner.stmt(statement);
                let pos = init_order::statement_pos(statement)
                    .cloned()
                    .unwrap_or_else(|| Pos::new(&checker.prog.files[file].name, 1, 1));
                check_effect(
                    scanner,
                    &pos,
                    &initialized,
                    "directly from this top-level statement",
                );
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_bindings_keep_the_first_declaration_order() {
        let diagnostics = crate::check_program(&[crate::SourceFile::entry(
            "main.ts",
            "const first: i32 = second + third;\n\
             const third: i32 = 1;\n\
             const second: i32 = 2;\n\
             const third: i32 = 3;\n\
             export function main(): void {}",
        )])
        .expect_err("duplicate declarations must fail");
        assert_eq!(
            diagnostics[0].message,
            "`third (main.ts)` is accessed before its declaration, directly from this initializer"
        );
    }

    #[test]
    fn unresolved_method_routes_follow_only_made_values() {
        let classes = [crate::check_program(&[crate::SourceFile::entry(
            "main.ts",
            "class A {} export function main(): void {}",
        )])
        .unwrap()
        .classes
        .into_iter()
        .find(|class| class.name == "A")
        .unwrap()];
        for (ty, definitions) in [
            (Type::I32, &[][..]),
            (Type::Class(ClassId(99)), &classes[..]),
            (Type::Class(ClassId(0)), &classes[..]),
        ] {
            let receiver = hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::Local("receiver".to_string(), ty.clone(), false),
                ty,
                pos: Pos::new("main.ts", 1, 1),
            };
            let mut scanner = ModuleEffectScanner::new(definitions);
            scanner.method_call(&receiver, &hir::Symbol::from_full_text("missing"));
            assert!(scanner.effects.indirect);
            assert!(resolved_with_value(&scanner.effects, false).is_empty());
            assert_eq!(
                resolved_with_value(&scanner.effects, true).get("later"),
                Some(&vec!["[indirect call]".to_string()])
            );
        }
    }

    #[test]
    fn builtin_methods_only_step_generators_indirectly() {
        for (ty, name) in [
            (Type::Array(Box::new(Type::I32)), "push"),
            (Type::Array(Box::new(Type::I32)), "pop"),
            (Type::Str, "slice"),
            (Type::Generator(Box::new(Type::I32)), "next"),
        ] {
            let receiver = hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::Local("receiver".to_string(), ty.clone(), false),
                ty,
                pos: Pos::new("main.ts", 1, 1),
            };
            let mut scanner = ModuleEffectScanner::new(&[]);
            scanner.callee(
                &hir::Callee::Method {
                    recv: Box::new(receiver),
                    name: hir::Symbol::from_full_text(name),
                },
                &[],
            );
            assert_eq!(scanner.effects.indirect, name == "next", "{name}");
        }
    }

    fn resolved_with_value(effects: &ModuleEffects, made: bool) -> BTreeMap<String, Vec<String>> {
        let unit = ModuleFunction::Lambda(hir::LambdaId(0));
        let summaries = HashMap::from([(
            unit.clone(),
            ModuleEffects {
                accesses: std::collections::BTreeSet::from(["later".to_string()]),
                ..ModuleEffects::default()
            },
        )]);
        let mut scan = ModuleRouteScan::default();
        if made {
            scan.made.insert(unit.clone());
            scan.made_order.push(unit);
        }
        let reads = scan.resolve(effects, &summaries).expect("valid summaries");
        reads
            .accesses
            .iter()
            .map(|(name, &index)| {
                (
                    name.clone(),
                    reads.route(index).into_iter().map(str::to_string).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn recorded_call_without_summary_follows_only_made_values() {
        let effects = ModuleEffects {
            calls: vec![(
                ModuleFunction::Free(hir::Symbol::from_full_text("missing")),
                "missing".to_string(),
            )],
            ..ModuleEffects::default()
        };
        assert!(resolved_with_value(&effects, false).is_empty());
        let resolved = resolved_with_value(&effects, true);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get("later"),
            Some(&vec!["[indirect call]".to_string()])
        );
    }
    #[test]
    fn a_missing_descriptor_class_is_an_indirect_call() {
        let mut scanner = ModuleEffectScanner::new(&[]);
        scanner.expr(&hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::DescriptorLit {
                class: ClassId(99),
                fields: Vec::new(),
            },
            ty: Type::Class(ClassId(99)),
            pos: Pos::new("main.ts", 1, 1),
        });
        assert!(scanner.effects.indirect);
        assert!(resolved_with_value(&scanner.effects, false).is_empty());
        assert_eq!(
            resolved_with_value(&scanner.effects, true).get("later"),
            Some(&vec!["[indirect call]".to_string()])
        );
    }
    #[test]
    fn twelve_callbacks_visit_each_unit_once_per_item() {
        let mut source = "const values: i32[] = [1]; function leaf(): i32 { return 1; } const stored: () => i32 = leaf;".to_string();
        for index in 0..12 {
            source.push_str(&format!("const cb{index}: () => i32 = (): i32 => {{ values.filter((v: i32): boolean => v > 0); return stored(); }};"));
        }
        source.push_str("const first: i32 = cb0(); export function main(): void {}");
        let module = crate::check_program(&[crate::SourceFile::entry("main.ts", source)])
            .expect("callbacks read only initialized globals");
        let mut summaries = HashMap::new();
        for function in &module.functions {
            insert_module_summary(
                ModuleFunction::Free(function.symbol.clone()),
                ModuleEffectScanner::new(&module.classes).function(function),
                &mut summaries,
            );
        }
        let mut scan = ModuleRouteScan::default();
        let mut pops = 0;
        for global in &module.globals {
            let mut scanner = ModuleEffectScanner::new(&module.classes);
            scanner.expr(&global.init);
            summaries.extend(std::mem::take(&mut scanner.effects.lambdas));
            let reads = scan
                .resolve(&scanner.effects, &summaries)
                .expect("valid summaries");
            assert!(
                reads.routes.len() <= reads.queue_pops + 1,
                "one link per pop and at most one indirect link"
            );
            for (index, route) in reads.routes.iter().enumerate() {
                assert!(
                    route.parent.is_none_or(|parent| parent < index),
                    "parents precede children"
                );
            }
            pops += reads.queue_pops;
        }
        assert_eq!(
            pops, 40,
            "fifteen item roots, twelve outer units, twelve filter units, and leaf"
        );
    }

    #[test]
    fn a_made_unit_without_a_summary_returns_an_error() {
        let unit = ModuleFunction::Lambda(hir::LambdaId(99));
        let mut scan = ModuleRouteScan::default();
        scan.made.insert(unit.clone());
        scan.made_order.push(unit);
        let effects = ModuleEffects {
            indirect: true,
            ..ModuleEffects::default()
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scan.resolve(&effects, &HashMap::new())
        }));
        assert_eq!(
            result.expect("the scan must not panic"),
            Err(RejectionFailure::new(
                RejectionSite::InitializerMissingSummary,
                "internal error: made function value has no summary".to_string()
            ))
        );
    }
}
