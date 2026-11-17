//! Checks of pointer graphs that share script storage (compiler.md §325 rule 5).
use super::*;

/// The reachable pointer checks, with finite nodes for recursive C types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WatchPlan {
    /// True when live pointer routes can return to an active node.
    pub recursive: bool,
    /// The root node.
    pub root: usize,
    /// The nodes in this graph.
    pub nodes: Vec<WatchNode>,
}
/// One struct at one C write permission.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WatchNode {
    /// The script-memory class.
    pub class: ClassId,
    /// True when C can write this struct's slots.
    pub writable: bool,
    /// The checks and reachable targets.
    pub members: Vec<WatchMember>,
}
/// One pointer check or route to another node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WatchMember {
    /// Visit the script elements of a nested pair.
    Array {
        /// The field index of the script array handle.
        field: usize,
        /// The element node index.
        target: usize,
    },
    /// Visit one element of an embedded fixed array.
    Element {
        /// The field index.
        field: usize,
        /// The element index.
        index: u32,
        /// The element node index.
        target: usize,
    },
    /// Compare this pointer slot after the call.
    Compare {
        /// The field index.
        field: usize,
        /// True when null is a script value.
        nullable: bool,
    },
    /// Follow this pointer before the call.
    Pointer {
        /// The field index.
        field: usize,
        /// The target node index.
        target: usize,
    },
    /// Visit an embedded struct.
    Embedded {
        /// The field index.
        field: usize,
        /// The embedded node index.
        target: usize,
    },
}
impl Planner<'_> {
    pub(super) fn watch(
        &self,
        class: ClassId,
        writable: bool,
    ) -> Result<Option<WatchPlan>, String> {
        let mut nodes = Vec::new();
        let root = self.watch_node(class, writable, &mut nodes)?;
        // Remove routes that cannot reach a compare, including read-only cycles.
        let mut live = vec![false; nodes.len()];
        loop {
            let before = live.clone();
            for (index, node) in nodes.iter().enumerate() {
                live[index] |= node.members.iter().any(|member| match member {
                    WatchMember::Compare { .. } => true,
                    WatchMember::Pointer { target, .. }
                    | WatchMember::Embedded { target, .. }
                    | WatchMember::Element { target, .. }
                    | WatchMember::Array { target, .. } => before[*target],
                });
            }
            if live == before {
                break;
            }
        }
        for node in &mut nodes {
            node.members.retain(|member| match member {
                WatchMember::Compare { .. } => true,
                WatchMember::Pointer { target, .. }
                | WatchMember::Embedded { target, .. }
                | WatchMember::Element { target, .. }
                | WatchMember::Array { target, .. } => live[*target],
            });
        }
        fn cycle(
            index: usize,
            nodes: &[WatchNode],
            active: &mut [bool],
            done: &mut [bool],
        ) -> bool {
            if active[index] {
                return true;
            }
            if done[index] {
                return false;
            }
            active[index] = true;
            let found = nodes[index].members.iter().any(|member| match member {
                WatchMember::Compare { .. } => false,
                WatchMember::Pointer { target, .. }
                | WatchMember::Embedded { target, .. }
                | WatchMember::Element { target, .. }
                | WatchMember::Array { target, .. } => cycle(*target, nodes, active, done),
            });
            active[index] = false;
            done[index] = true;
            found
        }
        let recursive = cycle(
            root,
            &nodes,
            &mut vec![false; nodes.len()],
            &mut vec![false; nodes.len()],
        );
        Ok(live[root].then_some(WatchPlan {
            root,
            nodes,
            recursive,
        }))
    }

    fn watch_node(
        &self,
        class: ClassId,
        writable: bool,
        nodes: &mut Vec<WatchNode>,
    ) -> Result<usize, String> {
        if let Some(index) = nodes
            .iter()
            .position(|node| node.class == class && node.writable == writable)
        {
            return Ok(index);
        }
        let index = nodes.len();
        nodes.push(WatchNode {
            class,
            writable,
            members: Vec::new(),
        });
        let mut members = Vec::new();
        for (field, value) in self.class(class)?.fields.iter().enumerate() {
            let shape = field_shape(self.module, &value.ty);
            if writable
                && matches!(
                    shape,
                    FieldShape::Pointer { .. } | FieldShape::Userdata | FieldShape::Callback
                )
            {
                members.push(WatchMember::Compare {
                    field,
                    nullable: matches!(value.ty, Type::Nullable(..)),
                });
            }
            match shape {
                FieldShape::Pair {
                    elements: Some(element),
                } => {
                    let constant = value
                        .foreign_provenance
                        .as_ref()
                        .is_some_and(|value| value.cannot_write());
                    let target = self.watch_node(element, !constant, nodes)?;
                    members.push(WatchMember::Array { field, target });
                }
                FieldShape::Pointer { target, .. } => {
                    let constant =
                        value.foreign_provenance == Some(ForeignTypeProvenance::ConstPointer);
                    let target = self.watch_node(target, !constant, nodes)?;
                    members.push(WatchMember::Pointer { field, target });
                }
                FieldShape::Embedded(nested) if matches!(value.ty, Type::FixedArray(..)) => {
                    let Type::FixedArray(_, length) = value.ty else {
                        return Err(internal("fixed array watch has no extent"));
                    };
                    let target = self.watch_node(nested, writable, nodes)?;
                    for index in 0..length {
                        members.push(WatchMember::Element {
                            field,
                            index,
                            target,
                        });
                    }
                }
                FieldShape::Embedded(nested) => {
                    let target = self.watch_node(nested, writable, nodes)?;
                    members.push(WatchMember::Embedded { field, target });
                }
                _ => {}
            }
        }
        nodes[index].members = members;
        Ok(index)
    }
}
