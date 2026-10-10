//! The read direction of a boundary struct (`specs/blocks/compiler.md` §187).
//!
//! A read position is a place where C writes memory that the script reads
//! after the call. [`member_read`] is the one predicate that decides
//! whether a struct member has a C→script lowering at a read position.
//! [`first_unreadable`] is the one walk: it visits every member that a
//! root reaches (embedded structs, pair elements, and pointer targets) and
//! applies [`member_read`] to each. The binder walks its parse and the
//! checker walks the mirror classes, each through its own [`ReadView`],
//! and both report a member with no read lowering with the text of
//! [`no_read_lowering_message`]. The member class carries the passes that
//! [`crate::pass`] decides, so the read root of each member is the
//! code-generation fact.

use crate::pass::{
    element_pass, embedded_pass, member_pass, struct_pass, target_pass, FieldShape, PointerPass,
    StructPass, StructView,
};

/// The read class of one struct member: its lowering, with the pass that
/// the call gives the struct that it holds or points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemberKind {
    /// A member whose C bytes are its script bytes: a scalar, an enum, a
    /// handle, a scalar alias, an external type, or a fixed array of these.
    Bytes,
    /// A `void *` userdata slot.
    Userdata,
    /// A collapsed count-first pair field (§30.2, §31.1).
    Pair {
        /// True when the element pointer is not `const`, so C can write
        /// the elements.
        mutable: bool,
        /// The pass of the elements, when they are boundary structs.
        elements: Option<PointerPass>,
    },
    /// A collapsed pair of elements that a read validates (§52). `mutable`
    /// is true when the element pointer is not `const`.
    ValidatedPair {
        /// True when C can write the elements.
        mutable: bool,
    },
    /// A string-view field (§28).
    StringView,
    /// A callback field.
    Callback,
    /// An embedded descriptor aggregate.
    DescriptorAggregate,
    /// An embedded boundary struct, or a fixed array of boundary structs
    /// (§32), with the pass that the call gives it.
    Embedded(StructPass),
    /// A boundary-struct pointer member (§33).
    StructPointer {
        /// The pass that the call gives the target.
        pass: PointerPass,
        /// True when the target is not `const`, so C can write it.
        mutable: bool,
    },
    /// A member type that no call lowering converts.
    Unlowered,
}

impl MemberKind {
    /// Classifies a member of shape `field` of a struct that the call
    /// passes as `parent`. `mutable` is true when the member is a pair or
    /// a struct pointer whose pointer is not `const`. The passes come from
    /// [`crate::pass`], so the read class is the code-generation fact.
    #[must_use]
    pub fn of<V: StructView>(
        view: &V,
        parent: StructPass,
        field: &FieldShape<V::Struct>,
        mutable: bool,
    ) -> MemberKind {
        match *field {
            FieldShape::Bytes => Self::Bytes,
            FieldShape::Unlowered => Self::Unlowered,
            FieldShape::Userdata => Self::Userdata,
            FieldShape::StringView => Self::StringView,
            FieldShape::Callback => Self::Callback,
            FieldShape::DescriptorAggregate => Self::DescriptorAggregate,
            FieldShape::ValidatedPair => Self::ValidatedPair { mutable },
            FieldShape::Pair { elements } => Self::Pair {
                mutable,
                elements: elements.map(|element| element_pass(view, element, mutable)),
            },
            FieldShape::Embedded(nested) => Self::Embedded(embedded_pass(view, parent, nested)),
            FieldShape::Pointer { target, .. } => Self::StructPointer {
                pass: member_pass(view, parent, target, mutable),
                mutable,
            },
        }
    }
}

/// How C reaches the struct that a walk reads. The set is closed: a
/// consumer matches it with no wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRoot {
    /// C produces the bytes and the script takes them as a new value: a
    /// result, a completion result, or a callback parameter.
    Value,
    /// C writes the members of a struct that the script holds, which the
    /// call passes as the pass says: the script memory, or a scratch copy
    /// that the copy-back writes back (§187 rule 7).
    Fill(StructPass),
    /// C reads the struct, which the call passes as the pass says. C can
    /// write only through its non-`const` pointers.
    Input(StructPass),
}

impl ReadRoot {
    /// The pass of the struct at this root. A value is C bytes.
    #[must_use]
    pub fn pass(self) -> StructPass {
        match self {
            Self::Value => StructPass::Bytes,
            Self::Fill(pass) | Self::Input(pass) => pass,
        }
    }

    /// The root of a target that a pointer of pass `pointer` reaches: C
    /// writes it when `mutable` is true, else C only reads it.
    #[must_use]
    pub fn of_target(pointer: PointerPass, mutable: bool) -> ReadRoot {
        let pass = target_pass(pointer);
        if mutable {
            Self::Fill(pass)
        } else {
            Self::Input(pass)
        }
    }
}

/// What the read of one member needs. The set is closed: a consumer
/// matches it with no wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRead {
    /// The member has a read lowering, or the read does not touch it.
    Readable,
    /// Read the members of the nested struct at `root` and `reach`. If no
    /// nested member is unreadable and `otherwise` is a kind, the member
    /// itself has no read lowering.
    Nested {
        /// The root of the nested struct.
        root: ReadRoot,
        /// The reach of the nested struct.
        reach: Reach,
        /// The kind of the member itself when the nested struct is
        /// readable.
        otherwise: Option<UnreadableKind>,
    },
    /// The member has no read lowering.
    Unreadable(UnreadableKind),
}

/// A struct member kind that has no read lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnreadableKind {
    /// A collapsed count-first pair field.
    Pair,
    /// A descriptor aggregate.
    DescriptorAggregate,
    /// A string-view field outside the copy-back of a scratch fill
    /// (§28.2, §30.1).
    StringView,
    /// A callback field.
    Callback,
    /// A userdata field that C can write (§187 rule 8).
    Userdata,
    /// A pair of validated elements that C can write (§187 rule 11).
    ValidatedPair,
    /// A pair whose elements the call copies in and back (§187 rule 11,
    /// core principle 16).
    WrittenBackPair,
    /// A member type that no call lowering converts.
    Unlowered,
}

impl UnreadableKind {
    /// The member noun that the diagnostic uses.
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            Self::Pair => "count-first pair field",
            Self::DescriptorAggregate => "descriptor aggregate",
            Self::StringView => "string-view field",
            Self::Callback => "callback field",
            Self::Userdata => "userdata field that C can write",
            Self::ValidatedPair => "pair of validated elements that C can write",
            Self::WrittenBackPair => "pair whose elements the call copies in and back",
            Self::Unlowered => "member with no call lowering",
        }
    }
}

/// How a member is reached from its read root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A direct member of the root.
    Root,
    /// A member reached from the root through by-value embedding only.
    Embedded,
    /// A member reached through a struct-pointer member or the elements
    /// of a pair, or a root that starts there.
    Indirect,
}

impl Reach {
    /// The reach of a struct that a member at this reach embeds.
    fn embedded(self) -> Reach {
        match self {
            Self::Root | Self::Embedded => Self::Embedded,
            Self::Indirect => Self::Indirect,
        }
    }
}

/// Decides the read of one member of kind `kind` of a struct that C
/// reaches at `root`, reached as `reach` says.
///
/// - A value (a result, a completion result, a callback parameter) has a
///   read lowering only for bytes and the structs that it embeds or points
///   to. A userdata slot that C writes has none (rule 8).
/// - A fill writes back every scratch copy (§187 rule 7). The copy-back
///   keeps the script array of a direct pair (rule 4) and materializes a
///   string view at the root and at any depth of by-value embedding
///   (rule 2). A string view behind a pointer or in pair elements has no
///   read lowering. A pair keeps its script array at every reach (rule
///   11): a pair of scalar elements is readable, and a pair of struct
///   elements is read where C writes the elements. The copy-back of a
///   scratch struct skips a userdata slot and a callback field, so the
///   script keeps its values (rule 8); in a struct whose bytes the call
///   passes, a userdata slot has no read lowering. A pointer member keeps
///   the script link; its target is read at the root that the pointer
///   gives it.
/// - An input is read only through its non-`const` pointers and pairs.
/// - A pair of validated elements that C can write has no read lowering
///   at any root (rule 11).
///
/// A member type that no call lowering converts has no read lowering
/// where C writes it.
#[must_use]
pub fn member_read(kind: MemberKind, root: ReadRoot, reach: Reach) -> MemberRead {
    match root {
        ReadRoot::Value => value_read(kind, reach),
        ReadRoot::Fill(pass) => fill_read(kind, pass, reach),
        ReadRoot::Input(_) => input_read(kind, reach),
    }
}

fn nested(root: ReadRoot, reach: Reach, otherwise: Option<UnreadableKind>) -> MemberRead {
    MemberRead::Nested {
        root,
        reach,
        otherwise,
    }
}

fn value_read(kind: MemberKind, reach: Reach) -> MemberRead {
    match kind {
        MemberKind::Bytes => MemberRead::Readable,
        // §187 rule 8: no registration validates a C-written address.
        MemberKind::Userdata => MemberRead::Unreadable(UnreadableKind::Userdata),
        MemberKind::ValidatedPair { .. } => MemberRead::Unreadable(UnreadableKind::Pair),
        MemberKind::Pair {
            elements: Some(_), ..
        } => nested(ReadRoot::Value, Reach::Indirect, Some(UnreadableKind::Pair)),
        MemberKind::Pair { elements: None, .. } => MemberRead::Unreadable(UnreadableKind::Pair),
        MemberKind::StringView => MemberRead::Unreadable(UnreadableKind::StringView),
        MemberKind::Callback => MemberRead::Unreadable(UnreadableKind::Callback),
        MemberKind::DescriptorAggregate => {
            MemberRead::Unreadable(UnreadableKind::DescriptorAggregate)
        }
        MemberKind::Unlowered => MemberRead::Unreadable(UnreadableKind::Unlowered),
        MemberKind::Embedded(_) => nested(ReadRoot::Value, reach.embedded(), None),
        MemberKind::StructPointer { .. } => nested(ReadRoot::Value, Reach::Indirect, None),
    }
}

fn fill_read(kind: MemberKind, pass: StructPass, reach: Reach) -> MemberRead {
    let scratch = pass == StructPass::Scratch;
    match kind {
        MemberKind::Bytes => MemberRead::Readable,
        // §187 rule 8: the copy-back of a scratch struct skips a userdata
        // slot and a callback field, so the script keeps its own values. In
        // a struct whose bytes the call passes, the script reads C's bytes.
        MemberKind::Userdata | MemberKind::Callback if scratch => MemberRead::Readable,
        MemberKind::Userdata => MemberRead::Unreadable(UnreadableKind::Userdata),
        // §187 rule 11 and core principle 16: a pair whose elements the call
        // copies in and back has a cost that grows with the count.
        MemberKind::Pair {
            elements: Some(PointerPass::ScratchWrittenBack),
            ..
        } => MemberRead::Unreadable(UnreadableKind::WrittenBackPair),
        // §187 rules 4 and 11: the copy-back keeps the script array of a
        // pair at every reach. C writes the elements in place, so the
        // elements are read where C writes them: through the element
        // pointer, or through the pointers of `const` elements.
        MemberKind::Pair {
            mutable,
            elements: Some(pointer),
        } => nested(ReadRoot::of_target(pointer, mutable), Reach::Indirect, None),
        MemberKind::Pair { elements: None, .. } => MemberRead::Readable,
        // §187 rule 11: C writes validated elements in place, with no
        // validation; a `const` pair of them stays readable.
        MemberKind::ValidatedPair { mutable: true } => {
            MemberRead::Unreadable(UnreadableKind::ValidatedPair)
        }
        MemberKind::ValidatedPair { mutable: false } => MemberRead::Readable,
        MemberKind::StringView if scratch && reach != Reach::Indirect => MemberRead::Readable,
        MemberKind::StringView => MemberRead::Unreadable(UnreadableKind::StringView),
        MemberKind::Callback => MemberRead::Unreadable(UnreadableKind::Callback),
        MemberKind::DescriptorAggregate => {
            MemberRead::Unreadable(UnreadableKind::DescriptorAggregate)
        }
        MemberKind::Unlowered => MemberRead::Unreadable(UnreadableKind::Unlowered),
        MemberKind::Embedded(nested_pass) => {
            nested(ReadRoot::Fill(nested_pass), reach.embedded(), None)
        }
        // C can replace the pointer; the copy-back keeps the script link
        // (§187 rule 7, 187.3 item 4).
        MemberKind::StructPointer { pass, mutable } => {
            nested(ReadRoot::of_target(pass, mutable), Reach::Indirect, None)
        }
    }
}

fn input_read(kind: MemberKind, reach: Reach) -> MemberRead {
    match kind {
        MemberKind::ValidatedPair { mutable: true } => {
            MemberRead::Unreadable(UnreadableKind::ValidatedPair)
        }
        MemberKind::Pair {
            elements: Some(PointerPass::ScratchWrittenBack),
            ..
        } => MemberRead::Unreadable(UnreadableKind::WrittenBackPair),
        MemberKind::Pair {
            mutable,
            elements: Some(pointer),
        } => nested(ReadRoot::of_target(pointer, mutable), Reach::Indirect, None),
        MemberKind::Embedded(pass) => nested(ReadRoot::Input(pass), reach.embedded(), None),
        MemberKind::StructPointer { pass, mutable } => {
            nested(ReadRoot::of_target(pass, mutable), Reach::Indirect, None)
        }
        MemberKind::Bytes
        | MemberKind::Userdata
        | MemberKind::ValidatedPair { mutable: false }
        | MemberKind::Pair { elements: None, .. }
        | MemberKind::StringView
        | MemberKind::Callback
        | MemberKind::DescriptorAggregate
        | MemberKind::Unlowered => MemberRead::Readable,
    }
}

/// One member of a struct, as a [`ReadView`] gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadMember<S> {
    /// The member name.
    pub name: String,
    /// The member shape.
    pub shape: FieldShape<S>,
    /// True when the member is a pair or a struct pointer whose pointer is
    /// not `const`. A member with no `const` fact is mutable: the walk
    /// fails closed.
    pub mutable: bool,
}

impl<S> ReadMember<S> {
    /// Builds a member.
    #[must_use]
    pub fn new(name: String, shape: FieldShape<S>, mutable: bool) -> Self {
        Self {
            name,
            shape,
            mutable,
        }
    }
}

/// One consumer's view of the structs that the read walk visits.
pub trait ReadView: StructView {
    /// The name of `s` in diagnostics.
    fn name(&self, s: Self::Struct) -> String;

    /// The members of `s`, in declaration order, or `None` when the walk
    /// reads nothing in `s`.
    fn members(&self, s: Self::Struct) -> Option<Vec<ReadMember<Self::Struct>>>;
}

/// The innermost member that has no read lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unreadable {
    /// The struct that declares the member.
    pub owner: String,
    /// The member name.
    pub member: String,
    /// The member kind.
    pub kind: UnreadableKind,
    /// For a walk that starts at an input: the outermost non-`const`
    /// member (owner, name) through which C writes the struct that holds
    /// the member.
    pub through: Option<(String, String)>,
    /// For [`UnreadableKind::WrittenBackPair`]: why the call writes the
    /// elements back.
    pub written_back: Option<WrittenBack>,
}

/// Why the call copies the elements of a pair in and back (§187 rule 11).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WrittenBack {
    /// True when the element pointer is not `const`.
    pub mutable: bool,
    /// The first pointer member (owner, name), at any depth of an element,
    /// whose target C can write and that the call passes as a scratch copy.
    pub member: Option<(String, String)>,
}

impl WrittenBack {
    /// The cause for a pair of `element` structs whose element pointer is
    /// not `const` when `mutable` is true.
    #[must_use]
    pub fn of<V: ReadView>(view: &V, element: V::Struct, mutable: bool) -> Self {
        Self {
            mutable,
            member: written_back_member(view, element, &mut Vec::new()),
        }
    }

    /// The sentences of the diagnostic after the position: the cause, the
    /// cost, and the forms that make the pair readable.
    fn text(&self) -> String {
        let cure = match (&self.member, self.mutable) {
            (None, _) => "Declare the element pointer `const`, or use elements that copy \
                          their bytes"
                .to_string(),
            (Some((owner, member)), false) => format!(
                "C can write the scratch target of `{owner}.{member}`; a `const` on that \
                 pointer, or on a pointer between, makes the pair readable"
            ),
            (Some((owner, member)), true) => format!(
                "Declare the element pointer `const`, and put a `const` on \
                 `{owner}.{member}` or on a pointer between; or use elements that copy \
                 their bytes"
            ),
        };
        format!(
            "a pair whose elements the call copies in and back one by one. The cost grows \
             with the count, and the call site does not show it. {cure} (compiler.md §187 \
             rule 11)"
        )
    }
}

/// The first pointer member, at any depth of `s`, whose target C can write
/// and that the scratch build of `s` passes as a scratch copy: the member
/// that makes [`crate::pass::writes_back`] true.
fn written_back_member<V: ReadView>(
    view: &V,
    s: V::Struct,
    active: &mut Vec<V::Struct>,
) -> Option<(String, String)> {
    if active.contains(&s) {
        return None;
    }
    let members = view.members(s)?;
    active.push(s);
    let found = members.iter().find_map(|member| match member.shape {
        FieldShape::Embedded(nested) if struct_pass(view, nested) == StructPass::Scratch => {
            written_back_member(view, nested, active)
        }
        FieldShape::Pointer { target, .. } => {
            match member_pass(view, StructPass::Scratch, target, member.mutable) {
                PointerPass::ScratchWrittenBack => Some((view.name(s), member.name.clone())),
                PointerPass::ScratchReadOnly => written_back_member(view, target, active),
                PointerPass::ScriptMemory | PointerPass::Cycle => None,
            }
        }
        FieldShape::Pair {
            elements: Some(element),
        } if struct_pass(view, element) == StructPass::Scratch => {
            written_back_member(view, element, active)
        }
        _ => None,
    });
    active.pop();
    found
}

impl Unreadable {
    /// The member `owner.member` of kind `kind`, with no `through` member.
    #[must_use]
    pub fn new(owner: String, member: String, kind: UnreadableKind) -> Self {
        Self {
            owner,
            member,
            kind,
            through: None,
            written_back: None,
        }
    }

    /// The diagnostic for this member at `position`. A walk that reached
    /// the member through a non-`const` member names that member.
    #[must_use]
    pub fn message(&self, position: ReadPosition<'_>) -> String {
        let position = match (&self.through, position) {
            (
                Some((owner, member)),
                ReadPosition::Parameter {
                    function,
                    parameter,
                },
            ) => ReadPosition::Reached {
                function,
                parameter,
                owner,
                member,
            },
            (_, position) => position,
        };
        match &self.written_back {
            Some(cause) if self.kind == UnreadableKind::WrittenBackPair => format!(
                "{} reads `{}.{}`, {}",
                position.describe(),
                self.owner,
                self.member,
                cause.text()
            ),
            _ => no_read_lowering_message(position, &self.owner, &self.member, self.kind),
        }
    }
}

/// Returns the innermost member of `s` that has no read lowering when C
/// reaches `s` at `root`, reached as `reach` says.
///
/// This is the one walk of §187 rule 3. It visits every member that the
/// root reaches, whatever the route: embedded structs, pair elements, and
/// pointer targets, and decides each with [`member_read`]. A struct that
/// is already on the path at the same root and reach is read where it
/// first appears.
#[must_use]
pub fn first_unreadable<V: ReadView>(
    view: &V,
    s: V::Struct,
    root: ReadRoot,
    reach: Reach,
) -> Option<Unreadable> {
    Walk {
        view,
        active: Vec::new(),
        from_input: matches!(root, ReadRoot::Input(_)),
    }
    .structure(s, root, reach, None)
}

struct Walk<'v, V: ReadView> {
    view: &'v V,
    active: Vec<(V::Struct, ReadRoot, Reach)>,
    /// True when the walk starts at an input, so it records the member
    /// through which C writes.
    from_input: bool,
}

impl<V: ReadView> Walk<'_, V> {
    fn structure(
        &mut self,
        s: V::Struct,
        root: ReadRoot,
        reach: Reach,
        through: Option<(String, String)>,
    ) -> Option<Unreadable> {
        let key = (s, root, reach);
        if self.active.contains(&key) {
            return None;
        }
        let members = self.view.members(s)?;
        let owner = self.view.name(s);
        self.active.push(key);
        let mut found = None;
        for member in &members {
            let kind = MemberKind::of(self.view, root.pass(), &member.shape, member.mutable);
            found = self.member(
                &owner,
                member,
                member_read(kind, root, reach),
                &through,
                root,
            );
            if found.is_some() {
                break;
            }
        }
        self.active.pop();
        found
    }

    fn member(
        &mut self,
        owner: &str,
        member: &ReadMember<V::Struct>,
        read: MemberRead,
        through: &Option<(String, String)>,
        root: ReadRoot,
    ) -> Option<Unreadable> {
        let own = |kind, through: Option<(String, String)>| Unreadable {
            owner: owner.to_string(),
            member: member.name.clone(),
            kind,
            through,
            written_back: match (kind, &member.shape) {
                (
                    UnreadableKind::WrittenBackPair,
                    FieldShape::Pair {
                        elements: Some(element),
                    },
                ) => Some(WrittenBack::of(self.view, *element, member.mutable)),
                _ => None,
            },
        };
        match read {
            MemberRead::Readable => None,
            MemberRead::Unreadable(kind) => Some(own(kind, through.clone())),
            MemberRead::Nested {
                root: nested_root,
                reach,
                otherwise,
            } => {
                let enters_fill = self.from_input
                    && matches!(root, ReadRoot::Input(_))
                    && matches!(nested_root, ReadRoot::Fill(_));
                let through = match through {
                    Some(through) => Some(through.clone()),
                    None if enters_fill => Some((owner.to_string(), member.name.clone())),
                    None => None,
                };
                member
                    .shape
                    .nested()
                    .and_then(|nested| self.structure(nested, nested_root, reach, through.clone()))
                    .or_else(|| otherwise.map(|kind| own(kind, through)))
            }
        }
    }
}

/// The struct-pointer, embedded, or pair member through which the scratch
/// build of a struct reaches a struct that is already on its build path
/// (§187 rule 9).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StructCycle {
    /// The struct that declares the member.
    pub owner: String,
    /// The member name.
    pub member: String,
}

impl StructCycle {
    /// The diagnostic for a position that passes this cycle. The binder
    /// and the checker give the same text.
    #[must_use]
    pub fn message(&self, position: ReadPosition<'_>) -> String {
        format!(
            "{} passes `{}.{}`, a member through which a struct reaches itself; a call \
             cannot build a struct cycle as scratch copies (compiler.md §187)",
            position.describe(),
            self.owner,
            self.member
        )
    }
}

/// Returns the member that closes a cycle in the scratch build of `s`, or
/// `None` when the build ends. The build follows each member whose struct
/// does not copy its bytes, as [`crate::struct_pass`] does, so a position
/// whose pass is [`StructPass::Cycle`] has a cycle member.
#[must_use]
pub fn first_cycle<V: ReadView>(view: &V, s: V::Struct) -> Option<StructCycle> {
    cycle_member(view, s, &mut Vec::new())
}

fn cycle_member<V: ReadView>(
    view: &V,
    s: V::Struct,
    active: &mut Vec<V::Struct>,
) -> Option<StructCycle> {
    let members = view.members(s)?;
    active.push(s);
    let mut found = None;
    for member in &members {
        let Some(nested) = member.shape.nested() else {
            continue;
        };
        if struct_pass(view, nested) == StructPass::Bytes {
            continue;
        }
        if active.contains(&nested) {
            found = Some(StructCycle {
                owner: view.name(s),
                member: member.name.clone(),
            });
        } else {
            found = cycle_member(view, nested, active);
        }
        if found.is_some() {
            break;
        }
    }
    active.pop();
    found
}

/// A position where C writes a value that the script reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadPosition<'a> {
    /// The result of a foreign function, by value or through a pointer.
    Result {
        /// The foreign function.
        function: &'a str,
    },
    /// The result that a completion delivers (§178 rule 6).
    CompletionResult {
        /// The foreign function.
        function: &'a str,
    },
    /// A parameter of a foreign function: a fill, or an input whose
    /// non-`const` pointers C can write through.
    Parameter {
        /// The foreign function.
        function: &'a str,
        /// The parameter.
        parameter: &'a str,
    },
    /// A non-`const` pointer target that a parameter reaches through a
    /// struct member.
    Reached {
        /// The foreign function.
        function: &'a str,
        /// The parameter.
        parameter: &'a str,
        /// The struct that declares the non-`const` pointer member.
        owner: &'a str,
        /// The non-`const` pointer member.
        member: &'a str,
    },
    /// A parameter of a callback that C calls.
    CallbackParameter {
        /// The callback typedef.
        callback: &'a str,
        /// The parameter.
        parameter: &'a str,
    },
}

impl ReadPosition<'_> {
    fn describe(self) -> String {
        match self {
            Self::Result { function } => format!("foreign function `{function}` result"),
            Self::CompletionResult { function } => {
                format!("foreign function `{function}` completion result")
            }
            Self::Parameter {
                function,
                parameter,
            } => format!("foreign function `{function}` parameter `{parameter}`"),
            Self::Reached {
                function,
                parameter,
                owner,
                member,
            } => format!(
                "foreign function `{function}` parameter `{parameter}` through `{owner}.{member}`"
            ),
            Self::CallbackParameter {
                callback,
                parameter,
            } => format!("callback typedef `{callback}` parameter `{parameter}`"),
        }
    }
}

/// Returns the diagnostic for a read position that reaches a member with
/// no read lowering. `owner` and `member` name the innermost member, in
/// the §32 discipline.
#[must_use]
pub fn no_read_lowering_message(
    position: ReadPosition<'_>,
    owner: &str,
    member: &str,
    kind: UnreadableKind,
) -> String {
    // A position reached through the member that it reads names the
    // member once.
    let position = match position {
        ReadPosition::Reached {
            function,
            parameter,
            owner: through_owner,
            member: through_member,
        } if through_owner == owner && through_member == member => ReadPosition::Parameter {
            function,
            parameter,
        },
        position => position,
    };
    if kind == UnreadableKind::WrittenBackPair {
        let cause = WrittenBack {
            mutable: true,
            member: None,
        };
        return format!(
            "{} reads `{owner}.{member}`, {}",
            position.describe(),
            cause.text()
        );
    }
    format!(
        "{} reads `{owner}.{member}`, a {} with no read lowering; a struct that C \
         writes and the script reads must not hold one (compiler.md §187)",
        position.describe(),
        kind.noun()
    )
}

/// The diagnostic for a by-value descriptor parameter at `position` whose
/// elements the call copies in and back for `cause` (§187 rule 11). The
/// binder and the checker give the same text.
#[must_use]
pub fn written_back_pair_message(position: ReadPosition<'_>, cause: &WrittenBack) -> String {
    format!("{} passes {}", position.describe(), cause.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REACHES: [Reach; 3] = [Reach::Root, Reach::Embedded, Reach::Indirect];
    const BYTES: StructPass = StructPass::Bytes;
    const SCRATCH: StructPass = StructPass::Scratch;
    const MEMORY: PointerPass = PointerPass::ScriptMemory;
    const COPY: PointerPass = PointerPass::ScratchWrittenBack;
    const READ: PointerPass = PointerPass::ScratchReadOnly;

    fn nested(root: ReadRoot, reach: Reach, otherwise: Option<UnreadableKind>) -> MemberRead {
        MemberRead::Nested {
            root,
            reach,
            otherwise,
        }
    }

    fn unreadable(kind: UnreadableKind) -> MemberRead {
        MemberRead::Unreadable(kind)
    }

    fn pointer(pass: PointerPass, mutable: bool) -> MemberKind {
        MemberKind::StructPointer { pass, mutable }
    }

    fn pair(mutable: bool, elements: Option<PointerPass>) -> MemberKind {
        MemberKind::Pair { mutable, elements }
    }

    /// A value reads bytes and the structs below; every other kind has no
    /// read lowering.
    #[test]
    fn a_value_reads_only_bytes_and_nested_structs() {
        for reach in REACHES {
            let case = |kind| member_read(kind, ReadRoot::Value, reach);
            assert_eq!(case(MemberKind::Bytes), MemberRead::Readable);
            // Rule 8: a userdata slot that C writes in a result.
            assert_eq!(
                case(MemberKind::Userdata),
                unreadable(UnreadableKind::Userdata)
            );
            for mutable in [true, false] {
                assert_eq!(
                    case(MemberKind::ValidatedPair { mutable }),
                    unreadable(UnreadableKind::Pair)
                );
            }
            assert_eq!(
                case(MemberKind::StringView),
                unreadable(UnreadableKind::StringView)
            );
            assert_eq!(
                case(MemberKind::Callback),
                unreadable(UnreadableKind::Callback)
            );
            assert_eq!(
                case(MemberKind::DescriptorAggregate),
                unreadable(UnreadableKind::DescriptorAggregate)
            );
            assert_eq!(
                case(MemberKind::Unlowered),
                unreadable(UnreadableKind::Unlowered)
            );
            assert_eq!(case(pair(true, None)), unreadable(UnreadableKind::Pair));
            assert_eq!(
                case(pair(false, Some(COPY))),
                nested(ReadRoot::Value, Reach::Indirect, Some(UnreadableKind::Pair))
            );
            assert_eq!(
                case(pointer(MEMORY, true)),
                nested(ReadRoot::Value, Reach::Indirect, None)
            );
        }
        assert_eq!(
            member_read(MemberKind::Embedded(BYTES), ReadRoot::Value, Reach::Root),
            nested(ReadRoot::Value, Reach::Embedded, None)
        );
        assert_eq!(
            member_read(
                MemberKind::Embedded(BYTES),
                ReadRoot::Value,
                Reach::Indirect
            ),
            nested(ReadRoot::Value, Reach::Indirect, None)
        );
    }

    /// §187 rules 2, 4, 7, and 8 at a fill.
    #[test]
    fn a_fill_reads_what_the_copy_back_writes() {
        let fill = ReadRoot::Fill;
        for pass in [BYTES, SCRATCH] {
            for reach in REACHES {
                let case = |kind| member_read(kind, fill(pass), reach);
                assert_eq!(case(MemberKind::Bytes), MemberRead::Readable);
                // Rule 8: the copy-back of a scratch struct skips a userdata
                // slot and a callback field; in a struct whose bytes the
                // call passes, the script reads C's bytes.
                let (userdata, callback) = if pass == SCRATCH {
                    (MemberRead::Readable, MemberRead::Readable)
                } else {
                    (
                        unreadable(UnreadableKind::Userdata),
                        unreadable(UnreadableKind::Callback),
                    )
                };
                assert_eq!(case(MemberKind::Userdata), userdata);
                assert_eq!(case(MemberKind::Callback), callback);
                assert_eq!(
                    case(MemberKind::DescriptorAggregate),
                    unreadable(UnreadableKind::DescriptorAggregate)
                );
                assert_eq!(
                    case(MemberKind::Unlowered),
                    unreadable(UnreadableKind::Unlowered)
                );
                // Rule 7: the copy-back keeps the link, and the target is
                // read where C writes it.
                assert_eq!(
                    case(pointer(COPY, true)),
                    nested(fill(SCRATCH), Reach::Indirect, None)
                );
                assert_eq!(
                    case(pointer(MEMORY, true)),
                    nested(fill(BYTES), Reach::Indirect, None)
                );
                assert_eq!(
                    case(pointer(COPY, false)),
                    nested(ReadRoot::Input(SCRATCH), Reach::Indirect, None)
                );
                assert_eq!(
                    case(MemberKind::Embedded(BYTES)),
                    nested(
                        fill(BYTES),
                        if reach == Reach::Indirect {
                            Reach::Indirect
                        } else {
                            Reach::Embedded
                        },
                        None
                    )
                );
            }
        }
        // Rule 2: a string view in the copy-back of a scratch struct, at the
        // root and through embedding; never behind a pointer.
        for (pass, reach, expected) in [
            (SCRATCH, Reach::Root, MemberRead::Readable),
            (SCRATCH, Reach::Embedded, MemberRead::Readable),
            (
                SCRATCH,
                Reach::Indirect,
                unreadable(UnreadableKind::StringView),
            ),
            (BYTES, Reach::Root, unreadable(UnreadableKind::StringView)),
        ] {
            assert_eq!(
                member_read(MemberKind::StringView, fill(pass), reach),
                expected,
                "{pass:?} {reach:?}"
            );
        }
        // Rule 4: the direct pair of a scratch fill keeps the script array;
        // its `const` elements are read where C writes them. Rule 11: a pair
        // whose elements the call copies in and back has no read lowering.
        let root = |kind| member_read(kind, fill(SCRATCH), Reach::Root);
        assert_eq!(root(pair(true, None)), MemberRead::Readable);
        for mutable in [true, false] {
            assert_eq!(
                root(pair(mutable, Some(COPY))),
                unreadable(UnreadableKind::WrittenBackPair)
            );
        }
        assert_eq!(
            root(pair(false, Some(READ))),
            nested(ReadRoot::Input(SCRATCH), Reach::Indirect, None)
        );
        assert_eq!(
            root(pair(false, Some(MEMORY))),
            nested(ReadRoot::Input(BYTES), Reach::Indirect, None)
        );
        for reach in [Reach::Embedded, Reach::Indirect] {
            // Rule 11: a pair keeps its script array at every reach, `const`
            // or not: scalar elements are readable, and struct elements are
            // read where C writes them, as at the root.
            for mutable in [true, false] {
                assert_eq!(
                    member_read(pair(mutable, None), fill(SCRATCH), reach),
                    MemberRead::Readable
                );
            }
            assert_eq!(
                member_read(pair(true, Some(COPY)), fill(SCRATCH), reach),
                unreadable(UnreadableKind::WrittenBackPair)
            );
            assert_eq!(
                member_read(pair(false, Some(READ)), fill(SCRATCH), reach),
                nested(ReadRoot::Input(SCRATCH), Reach::Indirect, None)
            );
            assert_eq!(
                member_read(pair(false, Some(MEMORY)), fill(SCRATCH), reach),
                nested(ReadRoot::Input(BYTES), Reach::Indirect, None)
            );
        }
    }

    /// §187 rule 11: a pair of validated elements that C can write has no
    /// read lowering at a fill and at an input, at every reach and pass; a
    /// `const` pair of them is readable.
    #[test]
    fn a_writable_pair_of_validated_elements_is_not_read() {
        for pass in [BYTES, SCRATCH] {
            for reach in REACHES {
                for root in [ReadRoot::Fill(pass), ReadRoot::Input(pass)] {
                    assert_eq!(
                        member_read(MemberKind::ValidatedPair { mutable: true }, root, reach),
                        unreadable(UnreadableKind::ValidatedPair),
                        "{root:?} {reach:?}"
                    );
                    assert_eq!(
                        member_read(MemberKind::ValidatedPair { mutable: false }, root, reach),
                        MemberRead::Readable,
                        "{root:?} {reach:?}"
                    );
                }
            }
        }
    }

    /// An input is read only through its non-`const` pointers and pairs,
    /// and through the `const` ones for what they reach.
    #[test]
    fn an_input_is_read_through_its_pointers() {
        for pass in [BYTES, SCRATCH] {
            for reach in REACHES {
                let case = |kind| member_read(kind, ReadRoot::Input(pass), reach);
                for kind in [
                    MemberKind::Bytes,
                    MemberKind::Userdata,
                    MemberKind::StringView,
                    MemberKind::Callback,
                    MemberKind::DescriptorAggregate,
                    MemberKind::Unlowered,
                    pair(true, None),
                ] {
                    assert_eq!(case(kind), MemberRead::Readable, "{kind:?}");
                }
                assert_eq!(
                    case(pointer(COPY, true)),
                    nested(ReadRoot::Fill(SCRATCH), Reach::Indirect, None)
                );
                assert_eq!(
                    case(pointer(MEMORY, false)),
                    nested(ReadRoot::Input(BYTES), Reach::Indirect, None)
                );
                assert_eq!(
                    case(pair(true, Some(MEMORY))),
                    nested(ReadRoot::Fill(BYTES), Reach::Indirect, None)
                );
                assert_eq!(
                    case(pair(false, Some(COPY))),
                    unreadable(UnreadableKind::WrittenBackPair)
                );
                assert_eq!(
                    case(pair(false, Some(READ))),
                    nested(ReadRoot::Input(SCRATCH), Reach::Indirect, None)
                );
            }
        }
    }

    #[test]
    fn a_member_kind_takes_its_pass_from_the_pass_module() {
        struct One;
        impl StructView for One {
            type Struct = u8;
            fn structs(&self) -> Vec<u8> {
                vec![0, 1]
            }
            fn fields(&self, s: u8) -> Option<Vec<FieldShape<u8>>> {
                match s {
                    0 => Some(vec![FieldShape::Bytes]),
                    1 => Some(vec![FieldShape::StringView]),
                    _ => None,
                }
            }
        }
        let shape = |target| FieldShape::Pointer {
            target,
            header: false,
            constant: false,
        };
        for parent in [BYTES, SCRATCH] {
            for target in [0, 1] {
                for mutable in [false, true] {
                    assert_eq!(
                        MemberKind::of(&One, parent, &shape(target), mutable),
                        MemberKind::StructPointer {
                            pass: member_pass(&One, parent, target, mutable),
                            mutable,
                        }
                    );
                    assert_eq!(
                        MemberKind::of(
                            &One,
                            parent,
                            &FieldShape::Pair {
                                elements: Some(target)
                            },
                            mutable
                        ),
                        pair(mutable, Some(element_pass(&One, target, mutable)))
                    );
                }
                assert_eq!(
                    MemberKind::of(&One, parent, &FieldShape::Embedded(target), false),
                    MemberKind::Embedded(embedded_pass(&One, parent, target))
                );
            }
        }
        assert_eq!(
            MemberKind::of(&One, SCRATCH, &FieldShape::Unlowered, false),
            MemberKind::Unlowered
        );
    }

    /// A view of named structs, for the walk tests: each member is
    /// `(name, shape, mutable)`.
    type Members = Vec<(&'static str, FieldShape<&'static str>, bool)>;

    struct Named(Vec<(&'static str, Members)>);

    impl Named {
        fn find(&self, s: &str) -> Option<&Members> {
            self.0
                .iter()
                .find(|(name, _)| *name == s)
                .map(|(_, members)| members)
        }
    }

    impl StructView for Named {
        type Struct = &'static str;

        fn structs(&self) -> Vec<&'static str> {
            self.0.iter().map(|(name, _)| *name).collect()
        }

        fn fields(&self, s: &'static str) -> Option<Vec<FieldShape<&'static str>>> {
            self.find(s)
                .map(|members| members.iter().map(|(_, shape, _)| *shape).collect())
        }
    }

    impl ReadView for Named {
        fn name(&self, s: &'static str) -> String {
            s.to_string()
        }

        fn members(&self, s: &'static str) -> Option<Vec<ReadMember<&'static str>>> {
            self.find(s).map(|members| {
                members
                    .iter()
                    .map(|(name, shape, mutable)| {
                        ReadMember::new((*name).to_string(), *shape, *mutable)
                    })
                    .collect()
            })
        }
    }

    fn view() -> Named {
        let to = |target| FieldShape::Pointer {
            target,
            header: false,
            constant: false,
        };
        let elements = |element| FieldShape::Pair {
            elements: Some(element),
        };
        Named(vec![
            ("Q", vec![("y", FieldShape::Bytes, false)]),
            (
                "P",
                vec![("x", FieldShape::Bytes, false), ("q", to("Q"), true)],
            ),
            ("Ud", vec![("ud", FieldShape::Userdata, false)]),
            (
                "Item",
                vec![("k", FieldShape::Bytes, false), ("p", to("P"), true)],
            ),
            ("List", vec![("items", elements("Item"), false)]),
            ("MutList", vec![("items", elements("Item"), true)]),
            ("Text", vec![("name", FieldShape::StringView, false)]),
            (
                "Named",
                vec![
                    ("name", FieldShape::StringView, false),
                    ("inner", FieldShape::Embedded("Ud"), false),
                ],
            ),
            (
                "UdHolder",
                vec![("k", FieldShape::Bytes, false), ("u", to("UdText"), true)],
            ),
            (
                "UdText",
                vec![
                    ("name", FieldShape::StringView, false),
                    ("ud", FieldShape::Userdata, false),
                ],
            ),
            (
                "ConstHolder",
                vec![("k", FieldShape::Bytes, false), ("u", to("UdText"), false)],
            ),
            (
                "Chain",
                vec![
                    ("name", FieldShape::StringView, false),
                    ("next", to("Chain"), true),
                    ("ud", FieldShape::Userdata, false),
                ],
            ),
            (
                "Viewer",
                vec![("k", FieldShape::Bytes, false), ("t", to("Text"), true)],
            ),
        ])
    }

    fn found(owner: &str, member: &str, kind: UnreadableKind) -> Unreadable {
        Unreadable {
            owner: owner.to_string(),
            member: member.to_string(),
            kind,
            through: None,
            written_back: None,
        }
    }

    /// A pair of `Item` whose element pointer is not `const` when
    /// `mutable` is true; `Item.p` needs the write-back.
    fn written_back(owner: &str, mutable: bool) -> Unreadable {
        let mut unreadable = found(owner, "items", UnreadableKind::WrittenBackPair);
        unreadable.written_back = Some(WrittenBack {
            mutable,
            member: Some(("Item".to_string(), "p".to_string())),
        });
        unreadable
    }

    fn through(mut unreadable: Unreadable, owner: &str, member: &str) -> Unreadable {
        unreadable.through = Some((owner.to_string(), member.to_string()));
        unreadable
    }

    /// The walk visits embedded structs, pair elements, and pointer
    /// targets, at each root.
    #[test]
    fn the_walk_visits_every_route() {
        let view = view();
        let walk = |s, root| first_unreadable(&view, s, root, Reach::Root);
        let fill = ReadRoot::Fill(SCRATCH);
        let input = ReadRoot::Input(SCRATCH);
        // Pointer targets, at every depth: the chain is readable under rule 7.
        assert_eq!(walk("Item", fill), None);
        assert_eq!(walk("P", fill), None);
        // A `const` pair whose elements write back a target, and a mutable
        // pair of scratch elements: the call copies each element in and back
        // (rule 11).
        assert_eq!(walk("List", fill), Some(written_back("List", false)));
        // An embedded userdata slot under a fill (rule 8).
        assert_eq!(
            walk("Named", fill),
            Some(found("Ud", "ud", UnreadableKind::Userdata))
        );
        // A userdata slot of a pointer target that C can write.
        assert_eq!(
            walk("UdHolder", fill),
            Some(found("UdText", "name", UnreadableKind::StringView))
        );
        // The same target behind a `const` pointer: C only reads it.
        assert_eq!(walk("ConstHolder", fill), None);
        // An input: C writes through `u`, and the walk names it.
        assert_eq!(
            walk("UdHolder", input),
            Some(through(
                found("UdText", "name", UnreadableKind::StringView),
                "UdHolder",
                "u"
            ))
        );
        assert_eq!(walk("ConstHolder", input), None);
        assert_eq!(walk("MutList", input), Some(written_back("MutList", true)));
        // A string view behind a pointer (rule 2).
        assert_eq!(
            walk("Viewer", fill),
            Some(found("Text", "name", UnreadableKind::StringView))
        );
        // A cycle ends: the second visit of `Chain` at the same root and
        // reach reads nothing more.
        assert_eq!(
            walk("Chain", fill),
            Some(found("Chain", "name", UnreadableKind::StringView))
        );
        assert_eq!(
            first_unreadable(&view, "Chain", ReadRoot::Value, Reach::Root),
            Some(found("Chain", "name", UnreadableKind::StringView))
        );
        // A struct that the view does not define reads nothing.
        assert_eq!(walk("Undefined", fill), None);
    }

    /// The walk reaches a member through each route in turn, so a check of
    /// one route does not hide another.
    #[test]
    fn the_walk_reports_the_innermost_member_of_each_route() {
        let embed = |nested| FieldShape::Embedded(nested);
        let to = |target| FieldShape::Pointer {
            target,
            header: false,
            constant: false,
        };
        let view = Named(vec![
            ("Ud", vec![("ud", FieldShape::Userdata, false)]),
            ("ByEmbed", vec![("e", embed("Ud"), false)]),
            ("ByPointer", vec![("p", to("Ud"), true)]),
            (
                "ByElements",
                vec![(
                    "items",
                    FieldShape::Pair {
                        elements: Some("Ud"),
                    },
                    true,
                )],
            ),
        ]);
        let userdata = Some(found("Ud", "ud", UnreadableKind::Userdata));
        for s in ["ByEmbed", "ByPointer", "ByElements"] {
            assert_eq!(
                first_unreadable(&view, s, ReadRoot::Fill(SCRATCH), Reach::Root),
                userdata,
                "{s}"
            );
        }
        assert_eq!(
            first_unreadable(&view, "ByEmbed", ReadRoot::Input(SCRATCH), Reach::Root),
            None
        );
        assert_eq!(
            first_unreadable(&view, "ByPointer", ReadRoot::Input(SCRATCH), Reach::Root),
            userdata
                .clone()
                .map(|found| through(found, "ByPointer", "p"))
        );
    }

    /// §187 rule 9: the cycle walk names the member that closes the cycle
    /// of a self link, a mutual pair, a struct that reaches a cycle, and a
    /// cycle through pair elements; a link to a struct that copies its bytes
    /// ends the build.
    #[test]
    fn the_cycle_walk_names_the_member_that_closes_the_cycle() {
        let to = |target| FieldShape::Pointer {
            target,
            header: false,
            constant: false,
        };
        let view = Named(vec![
            (
                "Node",
                vec![("v", FieldShape::Bytes, false), ("next", to("Node"), true)],
            ),
            ("A", vec![("b", to("B"), true)]),
            ("B", vec![("a", to("A"), false)]),
            (
                "Holder",
                vec![("k", FieldShape::Bytes, false), ("n", to("Node"), false)],
            ),
            (
                "Tree",
                vec![(
                    "kids",
                    FieldShape::Pair {
                        elements: Some("Tree"),
                    },
                    true,
                )],
            ),
            ("Q", vec![("y", FieldShape::Bytes, false)]),
            ("P", vec![("q", to("Q"), true)]),
        ]);
        let cycle = |owner: &str, member: &str| {
            Some(StructCycle {
                owner: owner.to_string(),
                member: member.to_string(),
            })
        };
        for (s, expected) in [
            ("Node", cycle("Node", "next")),
            ("A", cycle("B", "a")),
            ("B", cycle("A", "b")),
            ("Holder", cycle("Node", "next")),
            ("Tree", cycle("Tree", "kids")),
            ("P", None),
            ("Q", None),
        ] {
            assert_eq!(first_cycle(&view, s), expected, "{s}");
            assert_eq!(
                struct_pass(&view, s) == StructPass::Cycle,
                expected.is_some(),
                "{s}"
            );
        }
        let position = ReadPosition::Parameter {
            function: "nodeBump",
            parameter: "node",
        };
        assert_eq!(
            first_cycle(&view, "Node").map(|found| found.message(position)),
            Some(
                "foreign function `nodeBump` parameter `node` passes `Node.next`, a member \
                 through which a struct reaches itself; a call cannot build a struct cycle as \
                 scratch copies (compiler.md §187)"
                    .to_string()
            )
        );
    }

    /// §187 rule 11: a pair whose elements the call copies in and back has
    /// its own text, which names the cause and the accepted forms.
    #[test]
    fn a_written_back_pair_names_the_cause_and_the_accepted_forms() {
        let position = ReadPosition::Parameter {
            function: "touch",
            parameter: "span",
        };
        let lead = "a pair whose elements the call copies in and back one by one. The cost \
                    grows with the count, and the call site does not show it.";
        let rule = "(compiler.md §187 rule 11)";
        let mutable = WrittenBack {
            mutable: true,
            member: None,
        };
        let text = format!(
            "{lead} Declare the element pointer `const`, or use elements that copy their \
             bytes {rule}"
        );
        assert_eq!(
            no_read_lowering_message(position, "L", "items", UnreadableKind::WrittenBackPair),
            format!("foreign function `touch` parameter `span` reads `L.items`, {text}")
        );
        assert_eq!(
            written_back_pair_message(position, &mutable),
            format!("foreign function `touch` parameter `span` passes {text}")
        );
        // A `const` pair names the member that needs the write-back, and
        // does not ask for a `const` that the pair already has.
        let view = view();
        assert_eq!(
            first_unreadable(&view, "List", ReadRoot::Fill(SCRATCH), Reach::Root)
                .map(|found| found.message(position)),
            Some(format!(
                "foreign function `touch` parameter `span` reads `List.items`, {lead} C can \
                 write the scratch target of `Item.p`; a `const` on that pointer, or on a \
                 pointer between, makes the pair readable {rule}"
            ))
        );
        assert_eq!(
            first_unreadable(&view, "MutList", ReadRoot::Fill(SCRATCH), Reach::Root)
                .map(|found| found.message(position)),
            Some(format!(
                "foreign function `touch` parameter `span` reads `MutList.items`, {lead} \
                 Declare the element pointer `const`, and put a `const` on `Item.p` or on a \
                 pointer between; or use elements that copy their bytes {rule}"
            ))
        );
    }

    #[test]
    fn message_names_the_position_and_the_innermost_member() {
        let cases = [
            (
                ReadPosition::Result { function: "layGet" },
                UnreadableKind::Pair,
                "foreign function `layGet` result reads `Lay.items`, a count-first pair field",
            ),
            (
                ReadPosition::CompletionResult { function: "layGet" },
                UnreadableKind::StringView,
                "foreign function `layGet` completion result reads `Lay.items`, a string-view field",
            ),
            (
                ReadPosition::Parameter {
                    function: "layFill",
                    parameter: "out",
                },
                UnreadableKind::Callback,
                "foreign function `layFill` parameter `out` reads `Lay.items`, a callback field",
            ),
            (
                ReadPosition::Reached {
                    function: "use",
                    parameter: "list",
                    owner: "List",
                    member: "lays",
                },
                UnreadableKind::DescriptorAggregate,
                "foreign function `use` parameter `list` through `List.lays` reads `Lay.items`, \
                 a descriptor aggregate",
            ),
            (
                ReadPosition::CallbackParameter {
                    callback: "OnLay",
                    parameter: "lay",
                },
                UnreadableKind::Userdata,
                "callback typedef `OnLay` parameter `lay` reads `Lay.items`, a userdata field \
                 that C can write",
            ),
            (
                ReadPosition::Result { function: "layGet" },
                UnreadableKind::Unlowered,
                "foreign function `layGet` result reads `Lay.items`, a member with no call \
                 lowering",
            ),
        ];
        for (position, kind, prefix) in cases {
            let message = no_read_lowering_message(position, "Lay", "items", kind);
            assert_eq!(
                message,
                format!(
                    "{prefix} with no read lowering; a struct that C writes and the script \
                     reads must not hold one (compiler.md §187)"
                )
            );
        }
    }

    #[test]
    fn a_reached_member_that_is_read_is_named_once() {
        let tail = "with no read lowering; a struct that C writes and the script reads must \
                    not hold one (compiler.md §187)";
        let parameter = ReadPosition::Parameter {
            function: "use",
            parameter: "p",
        };
        let own = through(found("P", "w", UnreadableKind::Pair), "P", "w");
        assert_eq!(
            own.message(parameter),
            format!(
                "foreign function `use` parameter `p` reads `P.w`, a count-first pair field \
                 {tail}"
            )
        );
        let other = through(found("P", "w", UnreadableKind::Pair), "P", "v");
        assert_eq!(
            other.message(parameter),
            format!(
                "foreign function `use` parameter `p` through `P.v` reads `P.w`, a \
                 count-first pair field {tail}"
            )
        );
        assert_eq!(
            found("P", "w", UnreadableKind::Pair).message(parameter),
            format!(
                "foreign function `use` parameter `p` reads `P.w`, a count-first pair field \
                 {tail}"
            )
        );
    }
}
