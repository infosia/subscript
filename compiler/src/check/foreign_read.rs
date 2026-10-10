//! The checker side of the boundary read direction
//! (`specs/blocks/compiler.md` §187 rule 3).
//!
//! The checker is the total scan: it reads every result and every
//! parameter of every foreign function, and every struct argument of every
//! callback field, through the mirror classes, wherever their headers
//! define them. The mirror type spells `T *` and `const T *` alike, so the
//! `const` comes from the `@subscript-c-parameter` and `@subscript-c-member`
//! records, which the checker attaches to each parameter and member as
//! [`hir::ForeignTypeProvenance::ConstPointer`]; code generation reads the
//! same fact (§187 rule 7). A pointer with no record counts as
//! non-`const`: the check fails closed. A parameter whose struct the call
//! must build and that reaches a struct cycle is rejected with the text of
//! [`subscript_boundary::StructCycle::message`] (§187 rule 9). The binder rejects the positions it sees first, with the
//! same text. The read root of each struct is the code-generation fact:
//! the checker gives the mirror classes to the pass functions of the
//! boundary crate through [`crate::boundary_pass`], as both tiers give
//! their LIR classes. It walks each position with the one walk
//! [`subscript_boundary::first_unreadable`], which the binder also calls.

use super::*;
use crate::boundary_pass::{field_shape, with_constant, BoundaryClasses, ClassFacts, Classes};
use crate::check::rejection::RejectionSite;
use subscript_boundary::{
    element_pass, first_cycle, first_unreadable, parameter_pass, target_pass, value_parameter_pass,
    written_back_pair_message, FieldShape, PointerPass, Reach, ReadMember, ReadPosition, ReadRoot,
    ReadView, StructPass, StructView, Unreadable, WrittenBack,
};

/// The mirror classes as the pass decision reads them, with the embedded
/// headers (§33 rule 9) collected once.
struct MirrorClasses<'c, 'p> {
    checker: &'c Checker<'p>,
    headers: HashSet<ClassId>,
}

impl BoundaryClasses for MirrorClasses<'_, '_> {
    fn class(&self, id: ClassId) -> Option<ClassFacts> {
        self.checker.classes.get(id.0).map(|class| {
            ClassFacts::new(
                class.is_value,
                self.checker.boundary_classes.contains(&id),
                self.headers.contains(&id),
                class
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.ty.clone(),
                            field.foreign_provenance
                                == Some(hir::ForeignTypeProvenance::ConstPointer),
                        )
                    })
                    .collect(),
            )
        })
    }

    fn class_count(&self) -> usize {
        self.checker.classes.len()
    }

    fn resolve(&self, ty: &Type) -> Type {
        self.checker.apparent_type(ty)
    }
}

/// The scan of one checker over its mirror classes. It is the
/// [`ReadView`] of the one walk: the pass decision reads the classes as
/// both tiers read the LIR classes, and each member carries the `const`
/// fact of the mirror records.
struct Scan<'c, 'p> {
    classes: MirrorClasses<'c, 'p>,
    /// The view of each class, read once: the pass decision and the walk
    /// read each class many times.
    facts: Vec<ScanFacts>,
    structs: Vec<ClassId>,
    /// The classes that copy their bytes, decided once for the scan.
    copying: Option<Vec<ClassId>>,
    /// The classes whose pass is a cycle, decided once for the scan.
    cycles: Option<Vec<ClassId>>,
}

/// The facts of one class that the scan reads.
struct ScanFacts {
    fields: Option<Vec<FieldShape<ClassId>>>,
    members: Option<Vec<ReadMember<ClassId>>>,
}

impl<'c, 'p> Scan<'c, 'p> {
    fn new(classes: MirrorClasses<'c, 'p>) -> Self {
        let mut scan = Scan {
            classes,
            facts: Vec::new(),
            structs: Vec::new(),
            copying: None,
            cycles: None,
        };
        let view = Classes(&scan.classes);
        let count = scan.classes.class_count();
        scan.structs = view.structs();
        scan.facts = (0..count)
            .map(|index| ScanFacts {
                fields: view.fields(ClassId(index)),
                members: scan.read_members(ClassId(index)),
            })
            .collect();
        scan.copying = Some(subscript_boundary::copying_structs(&scan));
        scan.cycles = Some(subscript_boundary::cycle_structs(&scan));
        scan
    }
}

impl StructView for Scan<'_, '_> {
    type Struct = ClassId;

    fn structs(&self) -> Vec<ClassId> {
        self.structs.clone()
    }

    fn fields(&self, s: ClassId) -> Option<Vec<FieldShape<ClassId>>> {
        self.facts.get(s.0)?.fields.clone()
    }

    fn copying(&self) -> Option<&[ClassId]> {
        self.copying.as_deref()
    }

    fn cycles(&self) -> Option<&[ClassId]> {
        self.cycles.as_deref()
    }
}

impl ReadView for Scan<'_, '_> {
    fn name(&self, s: ClassId) -> String {
        self.checker().classes[s.0].name.clone()
    }

    fn members(&self, s: ClassId) -> Option<Vec<ReadMember<ClassId>>> {
        self.facts.get(s.0)?.members.clone()
    }
}

impl Scan<'_, '_> {
    /// The members of a boundary class. The shape comes from
    /// [`field_shape`], as both tiers derive it. A member that holds or
    /// points to a value class that is not a boundary struct reads as
    /// bytes: the walk reads nothing below it.
    fn read_members(&self, s: ClassId) -> Option<Vec<ReadMember<ClassId>>> {
        let checker = self.checker();
        if !checker.boundary_classes.contains(&s) {
            return None;
        }
        let definition = checker.classes.get(s.0)?;
        Some(
            definition
                .fields
                .iter()
                .map(|field| {
                    let constant =
                        field.foreign_provenance == Some(hir::ForeignTypeProvenance::ConstPointer);
                    let shape = with_constant(field_shape(&self.classes, &field.ty), constant);
                    let nested = shape
                        .nested()
                        .filter(|nested| checker.boundary_classes.contains(nested));
                    let shape = match (shape, nested) {
                        (FieldShape::Embedded(_) | FieldShape::Pointer { .. }, None) => {
                            FieldShape::Bytes
                        }
                        (FieldShape::Pair { .. }, None) => FieldShape::Pair { elements: None },
                        (shape, _) => shape,
                    };
                    // The `const` fact that code generation reads (§187 rule
                    // 7); a member with no record is mutable (rule 3).
                    let mutable =
                        field.foreign_provenance != Some(hir::ForeignTypeProvenance::ConstPointer);
                    ReadMember::new(field.name.clone(), shape, mutable)
                })
                .collect(),
        )
    }
}

impl<'p> Checker<'p> {
    /// Rejects each read position whose boundary struct holds, at any
    /// depth, a member with no read lowering (§187 rule 3): every foreign
    /// result and completion result, every parameter of every foreign
    /// function, every struct argument of every callback field, and every
    /// struct argument of a callback parameter of a foreign function. It
    /// runs after every mirror resolves its classes, so a position of a
    /// class that another mirror declares sees that class's fields.
    ///
    /// A completion result (§178 rule 6) is read the same way. Its class
    /// admits only scalar, enum, and nested struct fields, so the read
    /// finds nothing there today.
    pub(super) fn check_foreign_result_reads(&mut self) {
        let headers = self
            .boundary_classes
            .iter()
            .copied()
            .filter(|class| {
                crate::boundary_pass::is_embedded_header(&self.classes, &self.foreign_defs, *class)
            })
            .collect();
        let scan = Scan::new(MirrorClasses {
            checker: self,
            headers,
        });
        let read = |(message, pos)| (RejectionSite::ForeignResultRead, message, pos);
        let mut rejected: Vec<_> = scan.result_reads().into_iter().map(read).collect();
        rejected.extend(scan.parameter_reads());
        rejected.extend(scan.callback_parameter_reads().into_iter().map(read));
        rejected.extend(
            scan.foreign_callback_parameter_reads()
                .into_iter()
                .map(read),
        );
        for (site, message, pos) in rejected {
            self.reject_subset(site, message, pos);
        }
    }
}

impl Scan<'_, '_> {
    fn checker(&self) -> &Checker<'_> {
        self.classes.checker
    }

    /// The innermost member of `class` that has no read lowering when C
    /// reaches it at `root`: the one walk, which the binder also calls.
    fn unreadable(&self, class: ClassId, root: ReadRoot, reach: Reach) -> Option<Unreadable> {
        first_unreadable(self, class, root, reach)
    }

    /// Reads every foreign result and completion result as a value.
    fn result_reads(&self) -> Vec<(String, Pos)> {
        let checker = self.checker();
        let mut rejected = Vec::new();
        for function in &checker.foreign_defs {
            let (ret, completion) = match checker.apparent_type(&function.ret) {
                Type::AsyncHandle(value) => (checker.apparent_type(&value), true),
                ret => (ret, false),
            };
            let Some(class) = self.boundary_struct(&ret) else {
                continue;
            };
            let Some(found) = self.unreadable(class, ReadRoot::Value, Reach::Root) else {
                continue;
            };
            let position = if completion {
                ReadPosition::CompletionResult {
                    function: &function.name,
                }
            } else {
                ReadPosition::Result {
                    function: &function.name,
                }
            };
            rejected.push((found.message(position), function.pos.clone()));
        }
        rejected
    }

    /// Reads every parameter of every foreign function (§187 rule 3, the
    /// total scan). A struct-pointer parameter that is not `const` is a
    /// fill: C writes the whole struct, which the call passes as
    /// [`parameter_pass`] says. A `const` pointer, a by-value struct, and
    /// a by-value descriptor are inputs: C writes only through their
    /// non-`const` pointer members and pairs. A pointer parameter with no
    /// `@subscript-c-parameter` record counts as non-`const`.
    fn parameter_reads(&self) -> Vec<(RejectionSite, String, Pos)> {
        let checker = self.checker();
        let mut rejected = Vec::new();
        for definition in &checker.foreign_defs {
            for param in &definition.params {
                let pointer = matches!(checker.apparent_type(&param.ty), Type::Nullable(_));
                let fill = pointer
                    && param.foreign_provenance != Some(hir::ForeignTypeProvenance::ConstPointer);
                let position = ReadPosition::Parameter {
                    function: &definition.name,
                    parameter: &param.name,
                };
                if let Some((site, message)) = self.parameter_read(param, pointer, fill, position) {
                    rejected.push((site, message, definition.pos.clone()));
                }
            }
        }
        rejected
    }

    /// The diagnostic of one parameter at `position`: a struct cycle that
    /// the call must build (§187 rule 9), or the read of a fill, an input,
    /// or a by-value descriptor whose elements C can write.
    fn parameter_read(
        &self,
        param: &hir::Param,
        pointer: bool,
        fill: bool,
        position: ReadPosition<'_>,
    ) -> Option<(RejectionSite, String)> {
        let checker = self.checker();
        if let Type::Array(element) = checker.apparent_type(&param.ty) {
            // A by-value descriptor: its pair is an input pair whose
            // elements C writes unless the descriptor record says the
            // element pointer is `const`.
            let mutable = !matches!(
                &param.foreign_provenance,
                Some(hir::ForeignTypeProvenance::Descriptor {
                    element_const: true,
                    ..
                })
            );
            let class = self.boundary_struct(&element)?;
            let pointer = element_pass(self, class, mutable);
            if pointer == PointerPass::Cycle {
                return first_cycle(self, class)
                    .map(|cycle| (RejectionSite::ForeignStructCycle, cycle.message(position)));
            }
            // §187 rule 11: the call copies each element in and back.
            if pointer == PointerPass::ScratchWrittenBack {
                return Some((
                    RejectionSite::ForeignResultRead,
                    written_back_pair_message(position, &WrittenBack::of(self, class, mutable)),
                ));
            }
            let root = ReadRoot::of_target(pointer, mutable);
            return self
                .unreadable(class, root, Reach::Indirect)
                .map(|found| (RejectionSite::ForeignResultRead, found.message(position)));
        }
        let class = self.boundary_struct(&param.ty)?;
        let link = pointer.then(|| parameter_pass(self, class, fill));
        let pass = match link {
            Some(link) => target_pass(link),
            None => value_parameter_pass(self, class),
        };
        // §187 rule 9.
        if pass == StructPass::Cycle {
            return first_cycle(self, class)
                .map(|cycle| (RejectionSite::ForeignStructCycle, cycle.message(position)));
        }
        let root = if fill {
            ReadRoot::Fill(pass)
        } else {
            ReadRoot::Input(pass)
        };
        self.unreadable(class, root, Reach::Root)
            .map(|found| (RejectionSite::ForeignResultRead, found.message(position)))
    }

    /// Reads each struct parameter of each callback field of a boundary
    /// class, nullable or not: C passes the struct to the script. A
    /// function type carries no parameter names, so the position names the
    /// parameter by index.
    fn callback_parameter_reads(&self) -> Vec<(String, Pos)> {
        let checker = self.checker();
        let mut rejected = Vec::new();
        let mut classes: Vec<_> = checker.boundary_classes.iter().copied().collect();
        classes.sort_by_key(|class| class.0);
        for class in classes {
            for field in &checker.classes[class.0].fields {
                let callback = match &field.foreign_provenance {
                    Some(hir::ForeignTypeProvenance::Callback { typedef_name }) => {
                        typedef_name.clone()
                    }
                    _ => format!("{}.{}", checker.classes[class.0].name, field.name),
                };
                rejected.extend(self.callback_reads(&field.ty, &callback, &field.pos));
            }
        }
        rejected
    }

    /// Reads each struct parameter of each callback parameter of each
    /// foreign function: a hand-written mirror can declare one.
    fn foreign_callback_parameter_reads(&self) -> Vec<(String, Pos)> {
        let checker = self.checker();
        let mut rejected = Vec::new();
        for definition in &checker.foreign_defs {
            for param in &definition.params {
                let callback = match &param.foreign_provenance {
                    Some(hir::ForeignTypeProvenance::Callback { typedef_name }) => {
                        typedef_name.clone()
                    }
                    _ => format!("{}.{}", definition.name, param.name),
                };
                rejected.extend(self.callback_reads(&param.ty, &callback, &definition.pos));
            }
        }
        rejected
    }

    /// Reads each struct argument of `ty` as a value when `ty` is a
    /// function type.
    fn callback_reads(&self, ty: &Type, callback: &str, pos: &Pos) -> Vec<(String, Pos)> {
        let checker = self.checker();
        let ty = checker.apparent_type(ty);
        let Some(function) = ty.function_type() else {
            return Vec::new();
        };
        let mut rejected = Vec::new();
        for (index, parameter) in function.params.iter().enumerate() {
            let Some(argument) = self.boundary_struct(parameter) else {
                continue;
            };
            let Some(found) = self.unreadable(argument, ReadRoot::Value, Reach::Root) else {
                continue;
            };
            let parameter = format!("#{index}");
            rejected.push((
                found.message(ReadPosition::CallbackParameter {
                    callback,
                    parameter: &parameter,
                }),
                pos.clone(),
            ));
        }
        rejected
    }

    /// The boundary struct that a result or member of type `ty` holds by
    /// value (`X`) or through a pointer (`X | null`).
    fn boundary_struct(&self, ty: &Type) -> Option<ClassId> {
        let checker = self.checker();
        let class = match checker.apparent_type(ty) {
            Type::Class(id) => id,
            Type::Nullable(inner) => match checker.apparent_type(&inner) {
                Type::Class(id) => id,
                _ => return None,
            },
            _ => return None,
        };
        checker.boundary_classes.contains(&class).then_some(class)
    }
}
