//! The crossing plan of a foreign call (`specs/blocks/compiler.md` §189).
//!
//! [`call_plan`] is the one function that gives, for a foreign callee,
//! the plan of each parameter and of the result. Each node carries the
//! pass that [`subscript_boundary`] decides (§187 `pass.rs`), the
//! members that make a struct a scratch struct, and the members that the
//! call writes back. The result carries the read facts of §187 rule 12
//! (`read.rs`). The C emitter and the dev JIT build their marshaling from
//! this plan, and `subscript boundary` prints it ([`report`]). The
//! marshaling of both tiers decides no pass outside this plan.
//!
//! Two member walks of the dev JIT remain outside the plan. The C-layout
//! walk (`boundary_c_layout_in`) calls `struct_pass` for its check of a
//! fixed array of structs, and stops with the text of
//! [`NotLowered::FixedArrayOfStructs`]. The stabilize walk of a struct
//! result (`stabilize_boundary_return_value`) visits its embedded structs
//! and its pair elements.
//!
//! The plan reads the lowered LIR: the class view that both tiers read.

mod render;

use std::collections::HashMap;
use std::sync::Arc;

use crate::boundary_pass::{field_shape, Classes};
use crate::diag::Pos;
use crate::lir::{self as l, ForeignFunctionId, ForeignTypeProvenance};
use crate::types::{CallbackLifetime, ClassId, Type};
use subscript_boundary::{
    copy_back, element_pass, embedded_pass, first_unreadable, member_pass, parameter_pass,
    scratch_members, struct_pass, value_parameter_pass, CopyBack, FieldShape, PointerPass, Reach,
    ReadMember, ReadRoot, ReadView, StructPass, StructView, Unreadable,
};

pub use render::{render_site, report, Word};

/// The plan of one foreign callee: how a call crosses the C boundary.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CallPlan {
    /// One plan for each declared parameter, in declaration order.
    pub parameters: Vec<ParameterPlan>,
    /// The plan of the result.
    pub result: ResultPlan,
    /// True when the build of a call can allocate a scratch target or a
    /// scratch array, so the call opens a scratch scope (§187 rule 7).
    pub scratch_scope: bool,
}

/// How a call passes one parameter.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ParameterPlan {
    /// A scalar, an enum, a wire alias, or a handle: the value itself.
    Value,
    /// A string: a view of the script bytes (§28).
    StringView,
    /// The trailing completion endpoint of a host completion (§184).
    CompletionEndpoint,
    /// A collapsed pair (§30.2, §31.1).
    Pair(PairPlan),
    /// A by-value struct (§187 rule 10).
    ByValue(StructPlan),
    /// A struct pointer (§187 rule 7).
    Pointer(PointerPlan),
}

/// How a call passes the bytes of one struct.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StructPlan {
    /// The struct.
    pub class: ClassId,
    /// The pass of the struct.
    pub pass: StructPass,
    /// For a scratch struct: the fields that make it one
    /// ([`subscript_boundary::scratch_members`]).
    pub causes: Vec<usize>,
    /// For a scratch struct: the build of each member, in field order. The
    /// userdata fields that a callback absorbs have no entry.
    pub members: Vec<MemberPlan>,
}

/// The build of one member of a scratch struct.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MemberPlan {
    /// The field index in the class.
    pub field: usize,
    /// How the build gives the member.
    pub crossing: MemberCrossing,
}

/// How the build of a scratch struct gives one member.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MemberCrossing {
    /// The C bytes are the script bytes: a scalar, a handle, or a userdata
    /// slot.
    Bytes,
    /// A fixed array of scalars, copied as bytes (§187 rule 13).
    FixedBytes,
    /// A string view of the script bytes (§28).
    StringView,
    /// A callback field with its userdata fields.
    Callback(CallbackPlan),
    /// A collapsed pair.
    Pair(PairPlan),
    /// An embedded struct.
    Embedded(StructPlan),
    /// A struct-pointer member.
    Pointer(PointerPlan),
    /// A member that no call lowering builds.
    NotLowered(NotLowered),
}

/// A callback field and the userdata fields that it absorbs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallbackPlan {
    /// The field index of the userdata slot.
    pub userdata: usize,
    /// The field index of a second userdata slot, when the struct has one.
    pub second: Option<usize>,
    /// True for an explicit-lifetime registration (§111 rule 2).
    pub explicit: bool,
}

/// How a call passes the elements of a pair.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PairPlan {
    /// `None` when the call passes the script array: scalar elements, or
    /// structs that copy their bytes.
    pub elements: Option<ElementsPlan>,
}

/// The elements of a pair that the call does not pass as the script array.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ElementsPlan {
    /// The element struct.
    pub class: ClassId,
    /// The pass of the elements ([`subscript_boundary::element_pass`]).
    pub pass: PointerPass,
    /// The build of one element, for [`PointerPass::ScratchReadOnly`].
    pub element: Option<StructPlan>,
}

/// How a call passes the target of one struct pointer.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PointerPlan {
    /// The target struct.
    pub class: ClassId,
    /// The pass of the target.
    pub pass: PointerPass,
    /// The build of a scratch copy of the target.
    pub target: Option<StructPlan>,
    /// The write-back of the scratch copy, for
    /// [`PointerPass::ScratchWrittenBack`].
    pub write_back: Option<WriteBackPlan>,
}

/// The members that the copy-back of one scratch struct writes (§187
/// rules 6 and 7).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteBackPlan {
    /// The struct.
    pub class: ClassId,
    /// Each member that the copy-back writes, in field order.
    pub members: Vec<WriteBackMember>,
}

/// One member that a copy-back writes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteBackMember {
    /// The field index in the class.
    pub field: usize,
    /// How the copy-back writes the member.
    pub write: WriteBack,
}

/// How a copy-back writes one member, when C changed it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteBack {
    /// Copy the C bytes into the script member.
    Bytes,
    /// Copy a fixed array of scalars as bytes.
    FixedBytes,
    /// Build a new script string from the C view (§28.2).
    StringView,
    /// Copy an embedded struct that copies its bytes.
    EmbeddedBytes(ClassId),
    /// Write back an embedded scratch struct member by member.
    Embedded(WriteBackPlan),
    /// The embedded struct is a struct cycle (§187 rule 9).
    Cycle(ClassId),
}

/// Why no call lowering builds a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotLowered {
    /// A fixed array of structs in a struct that the call rebuilds (187.3
    /// item 6).
    FixedArrayOfStructs,
    /// A callback field with no userdata field after it.
    CallbackWithoutUserdata,
    /// A struct that reaches itself through scratch copies (§187 rule 9).
    StructCycle,
    /// A pair whose elements the call copies in and back (§187 rule 11).
    WrittenBackElements,
}

impl NotLowered {
    /// The reason, as the command prints it.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::FixedArrayOfStructs => {
                "a fixed array of structs in a struct that the call rebuilds"
            }
            Self::CallbackWithoutUserdata => "a callback field with no userdata field",
            Self::StructCycle => "a struct cycle",
            Self::WrittenBackElements => "a pair whose elements the call copies in and back",
        }
    }
}

/// How a call gives its result.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ResultPlan {
    /// No result.
    Void,
    /// A scalar, an enum, a wire alias, or a handle.
    Value,
    /// A struct that C produces (§187 rule 12).
    Struct(ReadPlan),
}

/// The read of a struct result, from the read facts of §187 rule 12.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ReadPlan {
    /// The struct.
    pub class: ClassId,
    /// The fields whose read goes through a pointer or the elements of a
    /// pair, at the root or through embedded structs. Empty when the
    /// result reads as its bytes.
    pub by_members: Vec<usize>,
    /// The innermost member that has no read lowering. The checker rejects
    /// such a result, so code generation never receives one.
    pub unreadable: Option<Unreadable>,
}

/// One foreign call site and the plan of its callee.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CallSite {
    /// The position of the call.
    pub pos: Pos,
    /// The callee.
    pub callee: ForeignFunctionId,
    /// The plan of the callee.
    pub plan: Arc<CallPlan>,
}

impl CallSite {
    /// Builds a call site.
    #[must_use]
    pub fn new(pos: Pos, callee: ForeignFunctionId, plan: Arc<CallPlan>) -> Self {
        Self { pos, callee, plan }
    }
}

/// The plans of one module, built once per callee.
#[derive(Debug, Default)]
pub struct Plans {
    plans: HashMap<ForeignFunctionId, Arc<CallPlan>>,
}

impl Plans {
    /// Returns the plan of `callee`, and builds it on the first request.
    ///
    /// # Errors
    ///
    /// An internal error when the module does not define the callee or a
    /// class that its plan reaches.
    pub fn get(
        &mut self,
        module: &l::Module,
        callee: ForeignFunctionId,
    ) -> Result<Arc<CallPlan>, String> {
        if let Some(plan) = self.plans.get(&callee) {
            return Ok(Arc::clone(plan));
        }
        let plan = Arc::new(call_plan(module, callee)?);
        self.plans.insert(callee, Arc::clone(&plan));
        Ok(plan)
    }
}

fn internal(message: impl Into<String>) -> String {
    format!("internal error: {}", message.into())
}

/// Gives the plan of foreign callee `callee` of `module`.
///
/// # Errors
///
/// An internal error when the module does not define the callee or a class
/// that its plan reaches.
pub fn call_plan(module: &l::Module, callee: ForeignFunctionId) -> Result<CallPlan, String> {
    let declaration = module
        .foreign_functions
        .get(callee.0 as usize)
        .filter(|declaration| declaration.id == callee)
        .ok_or_else(|| internal(format!("foreign function {} is missing", callee.0)))?;
    let planner = Planner {
        module,
        view: Classes(module),
    };
    let parameters = declaration
        .parameters
        .iter()
        .map(|parameter| planner.parameter(parameter))
        .collect::<Result<Vec<_>, _>>()?;
    let scratch_scope = declaration
        .parameters
        .iter()
        .any(|parameter| planner.builds_scratch(&parameter.ty));
    Ok(CallPlan {
        parameters,
        result: planner.result(&declaration.return_type)?,
        scratch_scope,
    })
}

/// Every foreign call site of `module`, in function, block, and
/// instruction order, with the plan of its callee. A generic function
/// gives one site for each instantiation (§189 rule 3).
///
/// # Errors
///
/// An internal error from [`call_plan`].
pub fn call_sites(module: &l::Module) -> Result<Vec<CallSite>, String> {
    let mut plans = Plans::default();
    let mut sites = Vec::new();
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let callee = match &instruction.kind {
                    l::InstructionKind::Call(target)
                    | l::InstructionKind::AsyncHandleCreate(target) => match target.kind {
                        l::CallTargetKind::Foreign(id) => Some(id),
                        _ => None,
                    },
                    l::InstructionKind::HostCompletion {
                        target: l::HostCompletionTarget::Foreign(id),
                        ..
                    } => Some(*id),
                    _ => None,
                };
                if let Some(callee) = callee {
                    let plan = plans.get(module, callee)?;
                    sites.push(CallSite::new(instruction.pos.clone(), callee, plan));
                }
            }
            if let l::Terminator::Suspend {
                kind:
                    l::SuspendKind::AsyncCall {
                        target:
                            l::CallTarget {
                                kind: l::CallTargetKind::Foreign(callee),
                                ..
                            },
                        ..
                    },
                pos,
                ..
            } = &block.terminator
            {
                let plan = plans.get(module, *callee)?;
                sites.push(CallSite::new(pos.clone(), *callee, plan));
            }
        }
    }
    Ok(sites)
}

struct Planner<'m> {
    module: &'m l::Module,
    view: Classes<'m, l::Module>,
}

impl Planner<'_> {
    fn class(&self, id: ClassId) -> Result<&l::Class, String> {
        self.module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| internal(format!("boundary class {} is missing", id.0)))
    }

    fn is_value(&self, id: ClassId) -> bool {
        self.class(id).is_ok_and(|class| class.is_value)
    }

    /// The value class of `X | null`.
    fn pointer_class(&self, ty: &Type) -> Option<ClassId> {
        match ty {
            Type::Nullable(inner) => match inner.as_ref() {
                Type::Class(id) if self.is_value(*id) => Some(*id),
                _ => None,
            },
            _ => None,
        }
    }

    fn parameter(&self, parameter: &l::ForeignParameter) -> Result<ParameterPlan, String> {
        if parameter.foreign_provenance == Some(ForeignTypeProvenance::CompletionEndpoint) {
            return Ok(ParameterPlan::CompletionEndpoint);
        }
        Ok(match &parameter.ty {
            Type::Str => ParameterPlan::StringView,
            Type::Array(element) => {
                // §187 rule 7: the call does not write `const` elements back.
                let writable = !matches!(
                    &parameter.foreign_provenance,
                    Some(
                        ForeignTypeProvenance::Descriptor {
                            element_const: true,
                            ..
                        } | ForeignTypeProvenance::ScalarPair {
                            element_const: true,
                            ..
                        }
                    )
                );
                ParameterPlan::Pair(self.pair(element, writable, &mut Vec::new())?)
            }
            Type::Class(class) if self.is_value(*class) => {
                let pass = value_parameter_pass(&self.view, *class);
                ParameterPlan::ByValue(self.structure(*class, pass, &mut Vec::new())?)
            }
            ty => match self.pointer_class(ty) {
                Some(class) => {
                    // §187 rule 7: the call does not write a `const` target
                    // back.
                    let writable =
                        parameter.foreign_provenance != Some(ForeignTypeProvenance::ConstPointer);
                    let pass = parameter_pass(&self.view, class, writable);
                    ParameterPlan::Pointer(self.pointer(class, pass, &mut Vec::new())?)
                }
                None => ParameterPlan::Value,
            },
        })
    }

    fn pair(
        &self,
        element: &Type,
        writable: bool,
        active: &mut Vec<ClassId>,
    ) -> Result<PairPlan, String> {
        let elements = match element {
            Type::Class(class)
                if self.is_value(*class)
                    && struct_pass(&self.view, *class) != StructPass::Bytes =>
            {
                let pass = element_pass(&self.view, *class, writable);
                let element = match pass {
                    PointerPass::ScratchReadOnly => {
                        Some(self.structure(*class, StructPass::Scratch, active)?)
                    }
                    PointerPass::ScriptMemory
                    | PointerPass::ScratchWrittenBack
                    | PointerPass::Cycle => None,
                };
                Some(ElementsPlan {
                    class: *class,
                    pass,
                    element,
                })
            }
            _ => None,
        };
        Ok(PairPlan { elements })
    }

    fn pointer(
        &self,
        class: ClassId,
        pass: PointerPass,
        active: &mut Vec<ClassId>,
    ) -> Result<PointerPlan, String> {
        let (target, write_back) = match pass {
            PointerPass::ScriptMemory | PointerPass::Cycle => (None, None),
            PointerPass::ScratchReadOnly => (
                Some(self.structure(class, StructPass::Scratch, active)?),
                None,
            ),
            PointerPass::ScratchWrittenBack => (
                Some(self.structure(class, StructPass::Scratch, active)?),
                Some(self.write_back(class, &mut Vec::new())?),
            ),
        };
        Ok(PointerPlan {
            class,
            pass,
            target,
            write_back,
        })
    }

    /// The plan of `class`, which the call passes as `pass`. A build that
    /// reaches a struct already on its path is a cycle (§187 rule 9).
    fn structure(
        &self,
        class: ClassId,
        pass: StructPass,
        active: &mut Vec<ClassId>,
    ) -> Result<StructPlan, String> {
        let definition = self.class(class)?;
        let pass = if active.contains(&class) {
            StructPass::Cycle
        } else {
            pass
        };
        if pass != StructPass::Scratch {
            return Ok(StructPlan {
                class,
                pass,
                causes: Vec::new(),
                members: Vec::new(),
            });
        }
        active.push(class);
        let mut members = Vec::new();
        let mut index = 0usize;
        while index < definition.fields.len() {
            let (crossing, width) = self.member(definition, index, active)?;
            members.push(MemberPlan {
                field: index,
                crossing,
            });
            index += width;
        }
        active.pop();
        Ok(StructPlan {
            class,
            pass,
            causes: scratch_members(&self.view, class),
            members,
        })
    }

    /// The build of field `index` of the scratch struct `definition`, and
    /// the number of fields that it takes.
    fn member(
        &self,
        definition: &l::Class,
        index: usize,
        active: &mut Vec<ClassId>,
    ) -> Result<(MemberCrossing, usize), String> {
        let field = &definition.fields[index];
        // §187 rule 7: the call does not write a `const` target back.
        let writable = field.foreign_provenance != Some(ForeignTypeProvenance::ConstPointer);
        Ok(match &field.ty {
            Type::Func(_) => {
                if index + 1 >= definition.fields.len() {
                    return Ok((
                        MemberCrossing::NotLowered(NotLowered::CallbackWithoutUserdata),
                        1,
                    ));
                }
                let second = definition
                    .fields
                    .get(index + 2)
                    .filter(|field| is_userdata_slot(&field.ty))
                    .map(|_| index + 2);
                let width = if second.is_some() { 3 } else { 2 };
                (
                    MemberCrossing::Callback(CallbackPlan {
                        userdata: index + 1,
                        second,
                        explicit: definition.callback_lifetime == CallbackLifetime::Explicit,
                    }),
                    width,
                )
            }
            Type::Array(element) => (
                MemberCrossing::Pair(self.pair(element, writable, active)?),
                1,
            ),
            Type::Str => (MemberCrossing::StringView, 1),
            Type::Class(nested) if self.is_value(*nested) => {
                let pass = embedded_pass(&self.view, StructPass::Scratch, *nested);
                (
                    MemberCrossing::Embedded(self.structure(*nested, pass, active)?),
                    1,
                )
            }
            ty if self.pointer_class(ty).is_some() => {
                let nested = self
                    .pointer_class(ty)
                    .ok_or_else(|| internal("boundary pointer class is missing"))?;
                let pass = member_pass(&self.view, StructPass::Scratch, nested, writable);
                (
                    MemberCrossing::Pointer(self.pointer(nested, pass, active)?),
                    1,
                )
            }
            ty @ Type::FixedArray(..) if field_shape(self.module, ty) == FieldShape::Bytes => {
                (MemberCrossing::FixedBytes, 1)
            }
            Type::FixedArray(..) => (
                MemberCrossing::NotLowered(NotLowered::FixedArrayOfStructs),
                1,
            ),
            _ => (MemberCrossing::Bytes, 1),
        })
    }

    /// The copy-back of the scratch struct `class` (§187 rules 6 and 7).
    fn write_back(
        &self,
        class: ClassId,
        active: &mut Vec<ClassId>,
    ) -> Result<WriteBackPlan, String> {
        let definition = self.class(class)?;
        active.push(class);
        let mut members = Vec::new();
        for (index, field) in definition.fields.iter().enumerate() {
            let write = match copy_back(&field_shape(self.module, &field.ty)) {
                CopyBack::Skip => continue,
                CopyBack::Bytes if matches!(field.ty, Type::FixedArray(..)) => {
                    WriteBack::FixedBytes
                }
                CopyBack::Bytes => WriteBack::Bytes,
                CopyBack::StringView => WriteBack::StringView,
                CopyBack::Embedded(nested) => match struct_pass(&self.view, nested) {
                    StructPass::Bytes => WriteBack::EmbeddedBytes(nested),
                    StructPass::Scratch if !active.contains(&nested) => {
                        WriteBack::Embedded(self.write_back(nested, active)?)
                    }
                    StructPass::Scratch | StructPass::Cycle => WriteBack::Cycle(nested),
                },
            };
            members.push(WriteBackMember {
                field: index,
                write,
            });
        }
        active.pop();
        Ok(WriteBackPlan { class, members })
    }

    /// True when a call that passes an argument of type `ty` can allocate a
    /// scratch target or a scratch array (§187 rule 7). A struct pointer
    /// passes its target in a local of the call, so only the build of that
    /// target can allocate; a pair of elements that do not copy their bytes
    /// always allocates its scratch array.
    fn builds_scratch(&self, ty: &Type) -> bool {
        let builds = |class: ClassId| {
            struct_pass(&self.view, class) == StructPass::Scratch
                && subscript_boundary::builds_scratch(&self.view, class)
        };
        match ty {
            Type::Array(element) => matches!(element.as_ref(), Type::Class(class)
                if self.is_value(*class)
                    && struct_pass(&self.view, *class) != StructPass::Bytes),
            Type::Nullable(inner) => {
                matches!(inner.as_ref(), Type::Class(class)
                    if self.is_value(*class) && builds(*class))
            }
            Type::Class(class) => self.is_value(*class) && builds(*class),
            _ => false,
        }
    }

    /// The result plan, from the read facts of §187 rule 12.
    fn result(&self, ty: &Type) -> Result<ResultPlan, String> {
        Ok(match ty {
            Type::Void => ResultPlan::Void,
            Type::Class(class) if self.is_value(*class) => {
                let view = LirRead(self);
                let mut by_members = Vec::new();
                for (index, member) in view.members(*class).unwrap_or_default().iter().enumerate() {
                    if read_goes_indirect(&view, member, Reach::Root, &mut Vec::new()) {
                        by_members.push(index);
                    }
                }
                ResultPlan::Struct(ReadPlan {
                    class: *class,
                    by_members,
                    unreadable: first_unreadable(&view, *class, ReadRoot::Value, Reach::Root),
                })
            }
            _ => ResultPlan::Value,
        })
    }
}

/// True when the read of `member` of a result goes through a pointer or the
/// elements of a pair, at `reach` or through the structs that it embeds.
fn read_goes_indirect(
    view: &LirRead<'_, '_>,
    member: &ReadMember<ClassId>,
    reach: Reach,
    active: &mut Vec<ClassId>,
) -> bool {
    let kind = subscript_boundary::MemberKind::of(
        view,
        ReadRoot::Value.pass(),
        &member.shape,
        member.mutable,
    );
    match subscript_boundary::member_read(kind, ReadRoot::Value, reach) {
        subscript_boundary::MemberRead::Nested {
            reach: Reach::Indirect,
            ..
        } => true,
        subscript_boundary::MemberRead::Nested { reach, .. } => {
            let Some(nested) = member.shape.nested() else {
                return false;
            };
            if active.contains(&nested) {
                return false;
            }
            active.push(nested);
            let found = view
                .members(nested)
                .unwrap_or_default()
                .iter()
                .any(|inner| read_goes_indirect(view, inner, reach, active));
            active.pop();
            found
        }
        subscript_boundary::MemberRead::Readable
        | subscript_boundary::MemberRead::Unreadable(_) => false,
    }
}

/// The LIR classes as a read view.
struct LirRead<'p, 'm>(&'p Planner<'m>);

impl StructView for LirRead<'_, '_> {
    type Struct = ClassId;

    fn structs(&self) -> Vec<ClassId> {
        self.0.view.structs()
    }

    fn fields(&self, s: ClassId) -> Option<Vec<FieldShape<ClassId>>> {
        self.0.view.fields(s)
    }
}

impl ReadView for LirRead<'_, '_> {
    fn name(&self, s: ClassId) -> String {
        self.0.class(s).map_or_else(
            |_| format!("class {}", s.0),
            |class| class.source_name.clone(),
        )
    }

    fn members(&self, s: ClassId) -> Option<Vec<ReadMember<ClassId>>> {
        let class = self.0.class(s).ok()?;
        let shapes = self.0.view.fields(s)?;
        Some(
            class
                .fields
                .iter()
                .zip(shapes)
                .map(|(field, shape)| {
                    ReadMember::new(
                        field.source_name.clone(),
                        shape,
                        field.foreign_provenance != Some(ForeignTypeProvenance::ConstPointer),
                    )
                })
                .collect(),
        )
    }
}

/// True for a `void *` userdata slot (`object` or `object | null`).
fn is_userdata_slot(ty: &Type) -> bool {
    matches!(ty, Type::Object) || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
}

#[cfg(test)]
mod tests;
