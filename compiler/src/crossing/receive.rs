//! Reads of received elements and pointer checks (compiler.md §325).
use super::*;

/// The read of one received C element, with input slots retained by compiler.md §325 rule 2.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceivePlan {
    /// A fixed array reads each of its declared elements.
    Fixed {
        /// The declared extent.
        length: u32,
        /// The read of one element.
        element: Box<ReceivePlan>,
    },
    /// A wire enum validates each received value.
    Validated(crate::types::StringAliasId),
    /// An element that copies its bytes.
    Bytes(Type),
    /// A string view copied into a script string.
    StringView,
    /// A pointer slot; no C address enters script storage.
    Pointer {
        /// True when null is a script value.
        nullable: bool,
        /// The script member type, which determines its storage size.
        ty: Type,
    },
    /// A struct read member by member, including structs that pass as bytes.
    Struct {
        /// The script class.
        class: ClassId,
        /// Every member in declaration order.
        members: Vec<ReceiveMember>,
    },
    /// A nested pair received into a new script array.
    Pair {
        /// True when the pointer precedes the count.
        pointer_first: bool,
        /// The read of each element.
        element: Box<ReceivePlan>,
    },
}

/// One member of a received struct.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReceiveMember {
    /// The member index.
    pub field: usize,
    /// The member's read.
    pub read: ReceivePlan,
}

impl Planner<'_> {
    pub(super) fn receive(&self, ty: &Type) -> Result<ReceivePlan, String> {
        if let Type::FixedArray(element, length) = ty {
            return Ok(ReceivePlan::Fixed {
                length: *length,
                element: Box::new(self.receive(element)?),
            });
        }
        if let Type::StringAlias(alias) = ty {
            return Ok(ReceivePlan::Validated(*alias));
        }
        let shape = field_shape(self.module, ty);
        Ok(match shape {
            FieldShape::Pointer { .. } | FieldShape::Userdata | FieldShape::Callback => {
                ReceivePlan::Pointer {
                    nullable: matches!(ty, Type::Nullable(..)),
                    ty: ty.clone(),
                }
            }
            FieldShape::StringView => ReceivePlan::StringView,
            FieldShape::Embedded(class) if !matches!(ty, Type::FixedArray(..)) => {
                let definition = self.class(class)?;
                let members = definition
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(field, value)| {
                        let mut read = self.receive(&value.ty)?;
                        if let ReceivePlan::Pair { pointer_first, .. } = &mut read {
                            *pointer_first = matches!(
                                value.foreign_provenance,
                                Some(ForeignTypeProvenance::PairOrder {
                                    pointer_first: true,
                                    ..
                                })
                            );
                        }
                        Ok(ReceiveMember { field, read })
                    })
                    .collect::<Result<_, String>>()?;
                ReceivePlan::Struct { class, members }
            }
            FieldShape::Pair { .. } | FieldShape::ValidatedPair => {
                let Type::Array(element) = ty else {
                    return Err(internal("received pair has no element"));
                };
                ReceivePlan::Pair {
                    pointer_first: false,
                    element: Box::new(self.receive(element)?),
                }
            }
            _ => ReceivePlan::Bytes(ty.clone()),
        })
    }

    /// A script-memory struct already receives scalar writes. Only pointer slots need checks.
    pub(super) fn pointer_checks(
        &self,
        class: ClassId,
        active: &mut Vec<ClassId>,
    ) -> Result<WriteBackPlan, String> {
        if active.contains(&class) {
            return Ok(WriteBackPlan {
                class,
                members: Vec::new(),
                path: self.class(class)?.source_name.clone(),
            });
        }
        active.push(class);
        let mut members = Vec::new();
        for (field, value) in self.class(class)?.fields.iter().enumerate() {
            if let Type::FixedArray(element, length) = &value.ty {
                if let Type::Class(nested) = element.as_ref() {
                    let plan = self.pointer_checks(*nested, active)?;
                    if !plan.members.is_empty() {
                        members.push(WriteBackMember {
                            field,
                            write: WriteBack::FixedArray(vec![plan; *length as usize]),
                        });
                    }
                }
                continue;
            }
            let write = match field_shape(self.module, &value.ty) {
                FieldShape::Pointer { .. } | FieldShape::Userdata | FieldShape::Callback => {
                    WriteBack::Pointer {
                        nullable: matches!(value.ty, Type::Nullable(..)),
                    }
                }
                FieldShape::Embedded(nested) => {
                    let plan = self.pointer_checks(nested, active)?;
                    if plan.members.is_empty() {
                        continue;
                    }
                    WriteBack::Embedded(plan)
                }
                _ => continue,
            };
            members.push(WriteBackMember { field, write });
        }
        active.pop();
        Ok(WriteBackPlan {
            class,
            members,
            path: self.class(class)?.source_name.clone(),
        })
    }
}
