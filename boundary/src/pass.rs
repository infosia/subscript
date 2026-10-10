//! How a foreign call passes a boundary struct (`specs/blocks/compiler.md`
//! §187 rule 3).
//!
//! [`struct_pass`] decides, for each struct, whether a call copies its
//! bytes, builds a C-layout scratch struct, or cannot pass it (a struct
//! cycle, §187 rule 9). [`parameter_pass`], [`member_pass`], and
//! [`element_pass`] decide, for each pointer, whether the call passes the
//! script memory or a scratch copy, and whether it writes the scratch copy
//! back: it writes back the target of a non-`const` pointer, and only the
//! members that C changed (§187 rule 7). A link to an embedded header
//! passes as its declared type (rule 14). [`copy_back`] decides which
//! members the copy-back of a scratch struct writes. The code
//! generation of both tiers, the binder, and the checker call these
//! functions. Each consumer gives its own view of the structs through
//! [`StructView`]; this module is the only place that decides.

/// The lowering of one script-visible member of a boundary struct. The
/// count half of a collapsed pair is not a member: the pair carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldShape<S> {
    /// The C bytes are the script bytes: a scalar, an enum, a handle, a
    /// scalar alias, an opaque external, or a fixed array of these.
    Bytes,
    /// A `void *` userdata slot (`object | null`).
    Userdata,
    /// A string view (§28).
    StringView,
    /// A collapsed count-first pair (§30.2, §31.1).
    Pair {
        /// The element struct, when the elements are boundary structs.
        elements: Option<S>,
    },
    /// A collapsed count-first pair of elements that a read validates: a
    /// `CEnum` wire alias (§52). No call writes the elements back through
    /// the validation (§187 rule 11).
    ValidatedPair,
    /// A callback field.
    Callback,
    /// An embedded descriptor aggregate.
    DescriptorAggregate,
    /// An embedded struct, or a fixed array of structs (§32).
    Embedded(S),
    /// A struct-pointer member (§33).
    Pointer {
        /// The target struct.
        target: S,
        /// True when the target is an embedded header (§33 rule 9): the
        /// link passes as its declared type, so it copies as it is when the
        /// header copies its bytes, `const` or not (§187 rule 14).
        header: bool,
        /// True when the pointer is `const`: C cannot write the target, so
        /// a target that copies its bytes passes as it is (§187 rule 9).
        constant: bool,
    },
    /// A member type that no call lowering converts.
    Unlowered,
}

impl<S: Copy> FieldShape<S> {
    /// The struct that the member holds, points to, or holds elements of.
    #[must_use]
    pub fn nested(&self) -> Option<S> {
        match *self {
            Self::Pair { elements } => elements,
            Self::Embedded(nested) | Self::Pointer { target: nested, .. } => Some(nested),
            Self::Bytes
            | Self::Userdata
            | Self::ValidatedPair
            | Self::StringView
            | Self::Callback
            | Self::DescriptorAggregate
            | Self::Unlowered => None,
        }
    }
}

/// One consumer's view of the boundary structs.
pub trait StructView {
    /// The identity of a struct in this view.
    type Struct: Copy + PartialEq;

    /// Every struct that this view defines.
    fn structs(&self) -> Vec<Self::Struct>;

    /// Every struct of the view that copies its bytes, when the view keeps
    /// the result of [`copying_structs`]. The pass decision then reads it
    /// in place of deciding again. `None` makes the decision compute it.
    fn copying(&self) -> Option<&[Self::Struct]> {
        None
    }

    /// Every struct of the view whose pass is [`StructPass::Cycle`], when
    /// the view keeps the result of [`cycle_structs`] beside
    /// [`StructView::copying`].
    fn cycles(&self) -> Option<&[Self::Struct]> {
        None
    }

    /// The members of `s`, in declaration order, or `None` when `s` is not
    /// a boundary struct that this view defines.
    fn fields(&self, s: Self::Struct) -> Option<Vec<FieldShape<Self::Struct>>>;
}

/// How a call passes the bytes of one struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructPass {
    /// The C bytes are the script bytes: the call copies or shares them.
    Bytes,
    /// The call builds a C-layout scratch struct.
    Scratch,
    /// The scratch struct reaches itself through scratch copies, so no
    /// call can build it (§187 rule 9). The binder and the checker reject
    /// each position that passes it; code generation never receives it.
    Cycle,
}

/// How a call passes the target of one pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPass {
    /// The call passes the script memory. C writes reach the script.
    ScriptMemory,
    /// The call passes a scratch copy and writes it back after the call,
    /// by [`copy_back`]: only the members that C changed (§187 rule 7).
    ScratchWrittenBack,
    /// The pointer is `const`: the call passes a scratch copy and does not
    /// write it back (§187 rule 7).
    ScratchReadOnly,
    /// The target is a [`StructPass::Cycle`] struct (§187 rule 9).
    Cycle,
}

/// Decides how a call passes the bytes of `s`. A struct copies its bytes
/// when every member is bytes, a userdata slot, an embedded struct that
/// copies its bytes, or a link that passes as it is: a link to an embedded
/// header that copies its bytes (§187 rule 14), or a `const` pointer to a
/// struct that copies its bytes (rule 9). The decision is the largest set of structs that meets
/// this, so a cycle of such links copies its bytes. A struct on its own
/// embedding path, and a struct that the view does not define, builds a
/// scratch struct. A scratch struct whose build reaches a struct that is
/// already on the build path is a [`StructPass::Cycle`] (rule 9).
#[must_use]
pub fn struct_pass<V: StructView>(view: &V, s: V::Struct) -> StructPass {
    let owned;
    let bytes: &[V::Struct] = match view.copying() {
        Some(copying) => copying,
        None => {
            owned = bytes_structs(view, s);
            &owned
        }
    };
    if bytes.contains(&s) {
        StructPass::Bytes
    } else if let Some(cycles) = view.cycles() {
        if cycles.contains(&s) {
            StructPass::Cycle
        } else {
            StructPass::Scratch
        }
    } else if builds_cycle(view, s, bytes, &mut Vec::new()) {
        StructPass::Cycle
    } else {
        StructPass::Scratch
    }
}

/// The structs that the decision for `s` reads: `s` and every struct that
/// a member reaches.
fn reachable<V: StructView>(view: &V, s: V::Struct) -> Vec<V::Struct> {
    let mut seen = Vec::new();
    let mut stack = vec![s];
    while let Some(next) = stack.pop() {
        if seen.contains(&next) {
            continue;
        }
        seen.push(next);
        for field in view.fields(next).unwrap_or_default() {
            if let Some(nested) = field.nested() {
                stack.push(nested);
            }
        }
    }
    seen
}

/// Every struct of the view whose pass is [`StructPass::Bytes`], decided at
/// once: the largest set whose members meet the rule of [`struct_pass`].
/// The decision for a struct reads only the structs that it reaches, so
/// each member of this set is a struct that [`struct_pass`] gives
/// [`StructPass::Bytes`].
#[must_use]
pub fn copying_structs<V: StructView>(view: &V) -> Vec<V::Struct> {
    largest_copying_set(view, view.structs())
}

/// Every struct of the view whose pass is [`StructPass::Cycle`], for a view
/// that keeps it (§187 rule 9).
#[must_use]
pub fn cycle_structs<V: StructView>(view: &V) -> Vec<V::Struct> {
    view.structs()
        .into_iter()
        .filter(|s| struct_pass(view, *s) == StructPass::Cycle)
        .collect()
}

/// The structs reachable from `s` that copy their bytes: the largest set
/// whose members meet the rule of [`struct_pass`].
fn bytes_structs<V: StructView>(view: &V, s: V::Struct) -> Vec<V::Struct> {
    largest_copying_set(view, reachable(view, s))
}

/// One struct with its members.
type Members<S> = (S, Vec<FieldShape<S>>);

fn largest_copying_set<V: StructView>(view: &V, candidates: Vec<V::Struct>) -> Vec<V::Struct> {
    // Each struct's members, read from the view once.
    let table: Vec<Members<V::Struct>> = candidates
        .into_iter()
        .filter_map(|t| view.fields(t).map(|fields| (t, fields)))
        .collect();
    let members = |t: V::Struct| {
        table
            .iter()
            .find(|(owner, _)| *owner == t)
            .map(|(_, fields)| fields)
    };
    // A struct on its own embedding path builds a scratch struct.
    let embeds_itself = |s: V::Struct| {
        let mut seen = Vec::new();
        let mut stack = vec![s];
        while let Some(next) = stack.pop() {
            for field in members(next).into_iter().flatten() {
                if let FieldShape::Embedded(nested) = *field {
                    if nested == s {
                        return true;
                    }
                    if !seen.contains(&nested) {
                        seen.push(nested);
                        stack.push(nested);
                    }
                }
            }
        }
        false
    };
    let mut bytes: Vec<V::Struct> = table
        .iter()
        .map(|(t, _)| *t)
        .filter(|t| !embeds_itself(*t))
        .collect();
    loop {
        let before = bytes.clone();
        bytes.retain(|t| {
            members(*t)
                .is_some_and(|fields| fields.iter().all(|field| member_copies(field, &before)))
        });
        if bytes.len() == before.len() {
            return bytes;
        }
    }
}

fn member_copies<S: Copy + PartialEq>(field: &FieldShape<S>, bytes: &[S]) -> bool {
    match *field {
        FieldShape::Bytes | FieldShape::Userdata => true,
        FieldShape::Embedded(nested) => bytes.contains(&nested),
        // §187 rule 14: a link to an embedded header passes as its
        // declared type, so it copies as it is when the header copies its
        // bytes, `const` or not.
        FieldShape::Pointer {
            target,
            header: true,
            ..
        } => bytes.contains(&target),
        FieldShape::Pointer {
            target, constant, ..
        } => constant && bytes.contains(&target),
        FieldShape::StringView
        | FieldShape::Pair { .. }
        | FieldShape::ValidatedPair
        | FieldShape::Callback
        | FieldShape::DescriptorAggregate
        | FieldShape::Unlowered => false,
    }
}

/// True when the scratch build of `s` reaches a struct that is already on
/// the build path. The build of a scratch struct builds each embedded
/// struct, each pointer target, and each pair element struct that does not
/// copy its bytes. A link to an embedded header builds its declared type
/// (§187 rule 14).
fn builds_cycle<V: StructView>(
    view: &V,
    s: V::Struct,
    bytes: &[V::Struct],
    active: &mut Vec<V::Struct>,
) -> bool {
    if active.contains(&s) {
        return true;
    }
    let Some(fields) = view.fields(s) else {
        return false;
    };
    active.push(s);
    let cycle = fields.iter().any(|field| {
        field.nested().is_some_and(|nested| {
            !bytes.contains(&nested) && builds_cycle(view, nested, bytes, active)
        })
    });
    active.pop();
    cycle
}

/// The pass of a pointer to `target`. `writable` is true when the pointer
/// is not `const`, so C can write the target. A link to an embedded header
/// passes as its declared type, whatever the box holds (§187 rule 14).
fn pointer_to<V: StructView>(view: &V, target: V::Struct, writable: bool) -> PointerPass {
    class_pass(view, target, writable)
}

/// The pass of a pointer whose target class is `s`: script memory for a
/// struct that copies its bytes, else a scratch copy, written back when
/// `writable` is true.
#[must_use]
pub fn class_pass<V: StructView>(view: &V, s: V::Struct, writable: bool) -> PointerPass {
    match struct_pass(view, s) {
        StructPass::Bytes => PointerPass::ScriptMemory,
        StructPass::Scratch if writable => PointerPass::ScratchWrittenBack,
        StructPass::Scratch => PointerPass::ScratchReadOnly,
        StructPass::Cycle => PointerPass::Cycle,
    }
}

/// Decides how a call passes the target of a struct-pointer parameter.
/// `writable` is true when the pointer is not `const`.
#[must_use]
pub fn parameter_pass<V: StructView>(view: &V, target: V::Struct, writable: bool) -> PointerPass {
    pointer_to(view, target, writable)
}

/// Decides how a call passes a by-value struct argument: as
/// [`struct_pass`] decides (§187 rule 10). The call copies the bytes of a
/// struct that copies its bytes, a fixed array of scalars included, and
/// builds a scratch struct for any other struct.
#[must_use]
pub fn value_parameter_pass<V: StructView>(view: &V, s: V::Struct) -> StructPass {
    struct_pass(view, s)
}

/// Decides how a call passes a struct `nested` that a struct passed as
/// `parent` embeds by value. The bytes of a parent that copies its bytes
/// hold the embedded struct as it is.
#[must_use]
pub fn embedded_pass<V: StructView>(view: &V, parent: StructPass, nested: V::Struct) -> StructPass {
    match parent {
        StructPass::Bytes => StructPass::Bytes,
        StructPass::Scratch => struct_pass(view, nested),
        StructPass::Cycle => StructPass::Cycle,
    }
}

/// How a call passes the struct that a pointer of pass `pointer` targets:
/// the script memory carries the script bytes, and a scratch copy is a
/// scratch struct. A cycle gives [`StructPass::Cycle`].
#[must_use]
pub fn target_pass(pointer: PointerPass) -> StructPass {
    match pointer {
        PointerPass::ScriptMemory => StructPass::Bytes,
        PointerPass::ScratchWrittenBack | PointerPass::ScratchReadOnly => StructPass::Scratch,
        PointerPass::Cycle => StructPass::Cycle,
    }
}

/// Decides how a call passes the target of a struct-pointer member of a
/// struct that the call passes as `parent`. `writable` is true when the
/// member pointer is not `const`. A struct that copies its bytes carries
/// the link as it is; a scratch struct links a target that copies its
/// bytes, and a scratch copy of any other target. The copy-back keeps the
/// script link and writes back the scratch copy of a non-`const` pointer
/// (§187 rule 7).
#[must_use]
pub fn member_pass<V: StructView>(
    view: &V,
    parent: StructPass,
    target: V::Struct,
    writable: bool,
) -> PointerPass {
    match parent {
        StructPass::Bytes => PointerPass::ScriptMemory,
        StructPass::Scratch => pointer_to(view, target, writable),
        StructPass::Cycle => PointerPass::Cycle,
    }
}

/// Decides how a call passes the elements of a pair of `element` structs:
/// the script array when the element copies its bytes, else a scratch
/// array. The scratch array needs a write-back when the element pointer is
/// not `const` (`writable`), or when the build of an element writes back a
/// target ([`writes_back`]); the binder and the checker reject such a pair
/// (§187 rule 11), so code generation receives only
/// [`PointerPass::ScratchReadOnly`] elements.
#[must_use]
pub fn element_pass<V: StructView>(view: &V, element: V::Struct, writable: bool) -> PointerPass {
    match struct_pass(view, element) {
        StructPass::Bytes => PointerPass::ScriptMemory,
        StructPass::Scratch if writable || writes_back(view, element) => {
            PointerPass::ScratchWrittenBack
        }
        StructPass::Scratch => PointerPass::ScratchReadOnly,
        StructPass::Cycle => PointerPass::Cycle,
    }
}

/// True when the scratch build of `s` writes back a target: a non-`const`
/// pointer member, at any depth of embedding and pointer targets, whose
/// target the call passes as a scratch copy (§187 rule 7).
#[must_use]
pub fn writes_back<V: StructView>(view: &V, s: V::Struct) -> bool {
    writes_back_from(view, s, &mut Vec::new())
}

fn writes_back_from<V: StructView>(view: &V, s: V::Struct, active: &mut Vec<V::Struct>) -> bool {
    if active.contains(&s) {
        return false;
    }
    let Some(fields) = view.fields(s) else {
        return false;
    };
    active.push(s);
    let found = fields.iter().any(|field| match *field {
        FieldShape::Embedded(nested) => {
            struct_pass(view, nested) == StructPass::Scratch
                && writes_back_from(view, nested, active)
        }
        FieldShape::Pointer {
            target, constant, ..
        } => match member_pass(view, StructPass::Scratch, target, !constant) {
            PointerPass::ScratchWrittenBack => true,
            PointerPass::ScratchReadOnly => writes_back_from(view, target, active),
            PointerPass::ScriptMemory | PointerPass::Cycle => false,
        },
        FieldShape::Pair {
            elements: Some(element),
        } => {
            struct_pass(view, element) == StructPass::Scratch
                && writes_back_from(view, element, active)
        }
        _ => false,
    });
    active.pop();
    found
}

/// True when the scratch build of `s` allocates a scratch target or a
/// scratch array: a pointer member whose target the call passes as a
/// scratch copy, or a pair whose elements do not copy their bytes, at any
/// depth of embedding. Both tiers open the scratch scope of a call only
/// when its build can allocate (§187 rule 7).
#[must_use]
pub fn builds_scratch<V: StructView>(view: &V, s: V::Struct) -> bool {
    builds_scratch_from(view, s, &mut Vec::new())
}

fn builds_scratch_from<V: StructView>(view: &V, s: V::Struct, active: &mut Vec<V::Struct>) -> bool {
    if active.contains(&s) {
        return false;
    }
    let Some(fields) = view.fields(s) else {
        return false;
    };
    active.push(s);
    let found = fields.iter().any(|field| match *field {
        FieldShape::Embedded(nested) => {
            struct_pass(view, nested) == StructPass::Scratch
                && builds_scratch_from(view, nested, active)
        }
        FieldShape::Pointer {
            target, constant, ..
        } => matches!(
            member_pass(view, StructPass::Scratch, target, !constant),
            PointerPass::ScratchWrittenBack | PointerPass::ScratchReadOnly
        ),
        FieldShape::Pair {
            elements: Some(element),
        } => struct_pass(view, element) == StructPass::Scratch,
        _ => false,
    });
    active.pop();
    found
}

/// What the copy-back of a scratch struct does with one member (§187
/// rule 6). Both tiers write back the members that this function names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyBack<S> {
    /// Copy the C bytes into the script member.
    Bytes,
    /// Build a new script string from the C view (§28.2).
    StringView,
    /// Copy back the embedded struct: its bytes when it copies its bytes,
    /// else member by member.
    Embedded(S),
    /// The copy-back does not write the member.
    Skip,
}

/// Decides what the copy-back of a scratch struct does with a member of
/// shape `field`. A pair keeps the script array (§187 rule 4); a callback
/// keeps the script closure; a pointer member keeps the script link, and
/// the call writes back the scratch copy of its target separately (§187
/// rule 7). A userdata slot keeps the script value: C can write any
/// address there, and no registration exists to validate it against.
#[must_use]
pub fn copy_back<S: Copy>(field: &FieldShape<S>) -> CopyBack<S> {
    match *field {
        FieldShape::Bytes => CopyBack::Bytes,
        FieldShape::StringView => CopyBack::StringView,
        FieldShape::Embedded(nested) => CopyBack::Embedded(nested),
        FieldShape::Userdata
        | FieldShape::Pair { .. }
        | FieldShape::ValidatedPair
        | FieldShape::Callback
        | FieldShape::DescriptorAggregate
        | FieldShape::Pointer { .. }
        | FieldShape::Unlowered => CopyBack::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view of named structs, for tests.
    struct Named(Vec<(&'static str, Vec<FieldShape<&'static str>>)>);

    impl StructView for Named {
        type Struct = &'static str;

        fn structs(&self) -> Vec<&'static str> {
            self.0.iter().map(|(name, _)| *name).collect()
        }

        fn fields(&self, s: &'static str) -> Option<Vec<FieldShape<&'static str>>> {
            self.0
                .iter()
                .find(|(name, _)| *name == s)
                .map(|(_, fields)| fields.clone())
        }
    }

    fn view() -> Named {
        let pointer = |target, header| FieldShape::Pointer {
            target,
            header,
            constant: false,
        };
        Named(vec![
            ("P", vec![FieldShape::Bytes, FieldShape::Userdata]),
            ("Text", vec![FieldShape::StringView]),
            ("Holder", vec![FieldShape::Bytes, pointer("P", false)]),
            ("Linked", vec![FieldShape::Bytes, pointer("Header", true)]),
            ("Header", vec![FieldShape::Bytes, pointer("Header", true)]),
            ("TextHeader", vec![FieldShape::StringView]),
            (
                "TextLinked",
                vec![FieldShape::Bytes, pointer("TextHeader", true)],
            ),
            ("Outer", vec![FieldShape::Embedded("P")]),
            ("Wrap", vec![FieldShape::Embedded("Text")]),
            ("Cycle", vec![FieldShape::Embedded("Cycle")]),
            (
                "List",
                vec![FieldShape::Pair {
                    elements: Some("P"),
                }],
            ),
            ("Fn", vec![FieldShape::Callback]),
            ("Desc", vec![FieldShape::DescriptorAggregate]),
            ("Odd", vec![FieldShape::Unlowered]),
            // §187 rule 9: a self link, a mutual pair, a struct that reaches
            // a cycle, and a cycle through pair elements.
            ("Node", vec![FieldShape::Bytes, pointer("Node", false)]),
            ("A", vec![FieldShape::Bytes, pointer("B", false)]),
            ("B", vec![FieldShape::Bytes, pointer("A", false)]),
            ("ToNode", vec![FieldShape::Bytes, pointer("Node", false)]),
            (
                "Tree",
                vec![FieldShape::Pair {
                    elements: Some("Tree"),
                }],
            ),
            // §187 rule 10: a fixed array of scalars is bytes.
            ("Ft", vec![FieldShape::Bytes, FieldShape::Bytes]),
        ])
    }

    #[test]
    fn a_struct_copies_its_bytes_only_when_every_member_does() {
        let view = view();
        for (name, pass) in [
            ("P", StructPass::Bytes),
            ("Header", StructPass::Bytes),
            ("Linked", StructPass::Bytes),
            ("Outer", StructPass::Bytes),
            ("Text", StructPass::Scratch),
            ("TextLinked", StructPass::Scratch),
            ("Holder", StructPass::Scratch),
            ("Wrap", StructPass::Scratch),
            ("Cycle", StructPass::Cycle),
            ("List", StructPass::Scratch),
            ("Fn", StructPass::Scratch),
            ("Desc", StructPass::Scratch),
            ("Odd", StructPass::Scratch),
            ("Undefined", StructPass::Scratch),
            ("Node", StructPass::Cycle),
            ("A", StructPass::Cycle),
            ("B", StructPass::Cycle),
            ("ToNode", StructPass::Cycle),
            ("Tree", StructPass::Cycle),
        ] {
            assert_eq!(struct_pass(&view, name), pass, "{name}");
        }
    }

    #[test]
    fn each_pointer_takes_the_pass_of_its_site() {
        let view = view();
        let (memory, back, read_only) = (
            PointerPass::ScriptMemory,
            PointerPass::ScratchWrittenBack,
            PointerPass::ScratchReadOnly,
        );
        for writable in [true, false] {
            assert_eq!(parameter_pass(&view, "P", writable), memory);
            assert_eq!(parameter_pass(&view, "Node", writable), PointerPass::Cycle);
            for parent in [StructPass::Bytes, StructPass::Scratch] {
                assert_eq!(member_pass(&view, parent, "P", writable), memory);
            }
            assert_eq!(
                member_pass(&view, StructPass::Bytes, "Holder", writable),
                memory
            );
            assert_eq!(
                member_pass(&view, StructPass::Cycle, "P", writable),
                PointerPass::Cycle
            );
            assert_eq!(element_pass(&view, "P", writable), memory);
            assert_eq!(element_pass(&view, "Tree", writable), PointerPass::Cycle);
        }
        // §187 rule 7: the scratch copy of a `const` target is not written
        // back.
        assert_eq!(parameter_pass(&view, "Holder", true), back);
        assert_eq!(parameter_pass(&view, "Holder", false), read_only);
        assert_eq!(
            member_pass(&view, StructPass::Scratch, "Holder", true),
            back
        );
        assert_eq!(
            member_pass(&view, StructPass::Scratch, "Holder", false),
            read_only
        );
        assert_eq!(element_pass(&view, "Text", true), back);
        assert_eq!(element_pass(&view, "Text", false), read_only);
        // §187 rule 10: a by-value argument takes the struct pass.
        for name in ["P", "Ft", "Holder", "Node"] {
            assert_eq!(
                value_parameter_pass(&view, name),
                struct_pass(&view, name),
                "{name}"
            );
        }
        assert_eq!(value_parameter_pass(&view, "Ft"), StructPass::Bytes);
        assert_eq!(
            embedded_pass(&view, StructPass::Bytes, "Text"),
            StructPass::Bytes
        );
        assert_eq!(
            embedded_pass(&view, StructPass::Scratch, "Text"),
            StructPass::Scratch
        );
        assert_eq!(
            embedded_pass(&view, StructPass::Scratch, "P"),
            StructPass::Bytes
        );
        assert_eq!(
            embedded_pass(&view, StructPass::Scratch, "Node"),
            StructPass::Cycle
        );
        for (pointer, pass) in [
            (memory, StructPass::Bytes),
            (back, StructPass::Scratch),
            (read_only, StructPass::Scratch),
            (PointerPass::Cycle, StructPass::Cycle),
        ] {
            assert_eq!(target_pass(pointer), pass, "{pointer:?}");
        }
    }

    /// §187 rules 9 and 14: a `const` cycle of structs that copy their
    /// bytes passes as script memory; a link to an embedded header passes as
    /// its declared type, whatever its extensions hold.
    #[test]
    fn a_const_cycle_and_a_header_family_take_the_pass_of_every_member() {
        let link = |target, header, constant| FieldShape::Pointer {
            target,
            header,
            constant,
        };
        let view = Named(vec![
            ("CNode", vec![FieldShape::Bytes, link("CNode", false, true)]),
            (
                "MNode",
                vec![FieldShape::Bytes, link("MNode", false, false)],
            ),
            ("Leaf", vec![FieldShape::Bytes]),
            ("CRef", vec![FieldShape::Bytes, link("Leaf", false, true)]),
            ("MRef", vec![FieldShape::Bytes, link("Leaf", false, false)]),
            ("QBase", vec![FieldShape::Bytes]),
            (
                "QExt",
                vec![FieldShape::Embedded("QBase"), FieldShape::Bytes],
            ),
            (
                "QExt2",
                vec![FieldShape::Embedded("QExt"), FieldShape::Bytes],
            ),
            (
                "QHolder",
                vec![FieldShape::Bytes, link("QBase", true, true)],
            ),
            ("PBase", vec![FieldShape::Bytes]),
            (
                "PExt",
                vec![FieldShape::Embedded("PBase"), FieldShape::StringView],
            ),
            (
                "PHolder",
                vec![FieldShape::Bytes, link("PBase", true, true)],
            ),
            ("VBase", vec![FieldShape::Bytes, FieldShape::StringView]),
            (
                "VExt",
                vec![FieldShape::Embedded("VBase"), FieldShape::Bytes],
            ),
            (
                "CNodeH",
                vec![FieldShape::Bytes, link("CNodeH", true, true)],
            ),
            (
                "CTagged",
                vec![FieldShape::Embedded("CNodeH"), FieldShape::Bytes],
            ),
        ]);
        for (name, pass) in [
            ("CNode", StructPass::Bytes),
            ("MNode", StructPass::Cycle),
            ("CRef", StructPass::Bytes),
            ("MRef", StructPass::Scratch),
            ("QHolder", StructPass::Bytes),
            ("PHolder", StructPass::Bytes),
            ("CNodeH", StructPass::Bytes),
            ("CTagged", StructPass::Bytes),
        ] {
            assert_eq!(struct_pass(&view, name), pass, "{name}");
        }
        assert_eq!(
            parameter_pass(&view, "CNode", false),
            PointerPass::ScriptMemory
        );
        // The decision at once gives each struct its own decision.
        let copying = copying_structs(&view);
        for name in view.structs() {
            assert_eq!(
                copying.contains(&name),
                struct_pass(&view, name) == StructPass::Bytes,
                "{name}"
            );
        }
        // A link passes as its declared type, whatever extension the box
        // holds (§187 rule 14).
        for writable in [true, false] {
            for header in ["QBase", "PBase"] {
                assert_eq!(
                    parameter_pass(&view, header, writable),
                    PointerPass::ScriptMemory
                );
                assert_eq!(
                    member_pass(&view, StructPass::Scratch, header, writable),
                    PointerPass::ScriptMemory
                );
            }
            assert_eq!(
                parameter_pass(&view, "VBase", writable),
                class_pass(&view, "VBase", writable)
            );
        }
    }

    /// §187 rule 14: a family member that is a cycle does not make the
    /// header a cycle: the link passes as the header.
    #[test]
    fn a_cycle_class_in_a_family_does_not_reach_the_header() {
        let link = |target, header, constant| FieldShape::Pointer {
            target,
            header,
            constant,
        };
        let view = Named(vec![
            ("Vec3", vec![FieldShape::Bytes]),
            (
                "NodeV",
                vec![FieldShape::Embedded("Vec3"), link("NodeV", false, false)],
            ),
            ("Holder", vec![FieldShape::Bytes, link("Vec3", true, false)]),
        ]);
        assert_eq!(struct_pass(&view, "NodeV"), StructPass::Cycle);
        assert_eq!(struct_pass(&view, "Holder"), StructPass::Bytes);
        for writable in [true, false] {
            assert_eq!(
                parameter_pass(&view, "Vec3", writable),
                PointerPass::ScriptMemory
            );
            assert_eq!(class_pass(&view, "NodeV", writable), PointerPass::Cycle);
        }
    }

    /// §187 rules 7 and 11: the build of a struct writes back a target
    /// through a non-`const` pointer member to a scratch target, at any
    /// depth; a pair of such elements, or of elements that C can write,
    /// needs a write-back of each element.
    #[test]
    fn a_build_writes_back_through_each_mutable_scratch_link() {
        let link = |target, constant| FieldShape::Pointer {
            target,
            header: false,
            constant,
        };
        let view = Named(vec![
            ("Vec", vec![FieldShape::Bytes]),
            ("Text", vec![FieldShape::StringView]),
            ("Plain", vec![FieldShape::StringView, link("Vec", false)]),
            ("Touch", vec![FieldShape::Bytes, link("Text", false)]),
            ("Look", vec![FieldShape::Bytes, link("Text", true)]),
            ("LookDeep", vec![FieldShape::Bytes, link("Touch", true)]),
            ("Embeds", vec![FieldShape::Embedded("Touch")]),
            (
                "Items",
                vec![FieldShape::Pair {
                    elements: Some("Touch"),
                }],
            ),
            (
                "Texts",
                vec![FieldShape::Pair {
                    elements: Some("Text"),
                }],
            ),
        ]);
        for (name, back) in [
            ("Vec", false),
            ("Text", false),
            ("Plain", false),
            ("Touch", true),
            ("Look", false),
            ("LookDeep", true),
            ("Embeds", true),
            ("Items", true),
            ("Texts", false),
        ] {
            assert_eq!(writes_back(&view, name), back, "{name}");
        }
        assert_eq!(element_pass(&view, "Vec", true), PointerPass::ScriptMemory);
        assert_eq!(
            element_pass(&view, "Text", true),
            PointerPass::ScratchWrittenBack
        );
        assert_eq!(
            element_pass(&view, "Text", false),
            PointerPass::ScratchReadOnly
        );
        assert_eq!(
            element_pass(&view, "Look", false),
            PointerPass::ScratchReadOnly
        );
        for name in ["Touch", "LookDeep", "Embeds"] {
            assert_eq!(
                element_pass(&view, name, false),
                PointerPass::ScratchWrittenBack,
                "{name}"
            );
        }
        for (name, builds) in [
            ("Vec", false),
            ("Text", false),
            ("Plain", false),
            ("Touch", true),
            ("Look", true),
            ("Embeds", true),
            ("Items", true),
            ("Texts", true),
        ] {
            assert_eq!(builds_scratch(&view, name), builds, "{name}");
        }
    }

    #[test]
    fn the_copy_back_writes_bytes_views_and_embedded_structs_only() {
        let cases: [(FieldShape<&str>, CopyBack<&str>); 10] = [
            (FieldShape::Bytes, CopyBack::Bytes),
            (FieldShape::StringView, CopyBack::StringView),
            (FieldShape::Embedded("P"), CopyBack::Embedded("P")),
            (FieldShape::Userdata, CopyBack::Skip),
            (FieldShape::Pair { elements: None }, CopyBack::Skip),
            (FieldShape::ValidatedPair, CopyBack::Skip),
            (FieldShape::Callback, CopyBack::Skip),
            (FieldShape::DescriptorAggregate, CopyBack::Skip),
            (
                FieldShape::Pointer {
                    target: "P",
                    header: false,
                    constant: false,
                },
                CopyBack::Skip,
            ),
            (FieldShape::Unlowered, CopyBack::Skip),
        ];
        for (field, expected) in cases {
            assert_eq!(copy_back(&field), expected, "{field:?}");
        }
    }
}
