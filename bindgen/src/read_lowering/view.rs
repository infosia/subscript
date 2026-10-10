//! The binder's view of boundary structs for the pass decision
//! (`specs/blocks/compiler.md` §187 rule 3).
//!
//! [`HeaderStructs`] maps each C field to the member shape that its mirror type
//! gives, so [`subscript_boundary::struct_pass`] and the pointer-pass
//! functions decide from the binder's parse what both tiers decide from
//! the mirror classes.

use std::collections::{HashMap, HashSet};

use subscript_boundary::{FieldShape, ReadMember, ReadView, StructView};

use crate::clangfe::Parsed;
use crate::cparse::{CField, Decl};
use crate::emit::{embedded_array_pairs, lang_scalar, EmbeddedPairs, Kind};

/// The structs of one parse, with the registry that classifies their field
/// types and the embedded headers (§33 rule 9) of the parse.
pub(crate) struct HeaderStructs<'a> {
    parsed: &'a Parsed,
    registry: &'a HashMap<String, Kind>,
    headers: HashSet<&'a str>,
    /// The facts of each struct of the parse, read once: the pass decision
    /// and the walk read each struct many times (core principle 15).
    cache: HashMap<&'a str, StructFacts<'a>>,
    /// The structs that copy their bytes, decided once for the view.
    copying: Option<Vec<&'a str>>,
    /// The structs whose pass is a cycle, decided once for the view.
    cycles: Option<Vec<&'a str>>,
}

/// The facts of one struct that the view gives.
struct StructFacts<'a> {
    fields: Option<Vec<FieldShape<&'a str>>>,
    members: Option<Vec<ReadMember<&'a str>>>,
}

impl<'a> HeaderStructs<'a> {
    /// Builds the view. A struct is an embedded header when it is the
    /// first by-value field of another boundary struct and a struct field
    /// or a function parameter points to it, as the mirror classes give it.
    pub(crate) fn new(parsed: &'a Parsed, registry: &'a HashMap<String, Kind>) -> Self {
        let boundary = |base: &str| matches!(registry.get(base), Some(Kind::Boundary));
        let mut first = HashSet::new();
        let mut linked = HashSet::new();
        for decl in &parsed.decls {
            match decl {
                Decl::Struct { name, fields } if boundary(name) => {
                    if let Some(field) = fields.first() {
                        if !field.pointer && field.array_len.is_none() && boundary(&field.base) {
                            first.insert(field.base.as_str());
                        }
                    }
                    linked.extend(
                        fields
                            .iter()
                            .filter(|field| field.pointer && field.array_len.is_none())
                            .map(|field| field.base.as_str()),
                    );
                }
                Decl::Func { params, .. } => linked.extend(
                    params
                        .iter()
                        .filter(|param| param.pointer && param.array_len.is_none())
                        .map(|param| param.base.as_str()),
                ),
                Decl::Struct { .. }
                | Decl::Enum { .. }
                | Decl::Handle { .. }
                | Decl::FnPtr { .. } => {}
            }
        }
        let headers = first.intersection(&linked).copied().collect();
        let mut view = Self {
            parsed,
            registry,
            headers,
            cache: HashMap::new(),
            copying: None,
            cycles: None,
        };
        let names: Vec<&'a str> = parsed
            .decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Struct { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let cache = names
            .into_iter()
            .map(|name| {
                let facts = StructFacts {
                    fields: view.read_fields(name),
                    members: view.read_members(name),
                };
                (name, facts)
            })
            .collect();
        view.cache = cache;
        view.copying = Some(subscript_boundary::copying_structs(&view));
        view.cycles = Some(subscript_boundary::cycle_structs(&view));
        view
    }

    /// The name of struct `name` as the parse spells it, when the parse
    /// defines it.
    pub(crate) fn struct_name(&self, name: &str) -> Option<&'a str> {
        self.parsed.decls.iter().find_map(|decl| match decl {
            Decl::Struct { name: owner, .. } if owner == name => Some(owner.as_str()),
            _ => None,
        })
    }

    /// The fields of struct `name`, when the parse defines it.
    pub(crate) fn struct_fields(&self, name: &str) -> Option<&'a [CField]> {
        self.parsed.decls.iter().find_map(|decl| match decl {
            Decl::Struct {
                name: owner,
                fields,
            } if owner == name => Some(fields.as_slice()),
            _ => None,
        })
    }

    /// The shape of field `index` of a struct whose pairs are `pairs`. The
    /// count half of a collapsed pair has no shape: the pointer half
    /// carries the pair.
    pub(crate) fn field_shape(
        &self,
        pairs: &EmbeddedPairs,
        index: usize,
        field: &'a CField,
    ) -> Option<FieldShape<&'a str>> {
        if pairs.count_idx.contains(&index) {
            return None;
        }
        let kind = self.registry.get(&field.base);
        let base = field.base.as_str();
        if pairs.ptr_elem.contains_key(&index) {
            // §187 rule 11: a read validates `CEnum` elements (§52).
            if matches!(kind, Some(Kind::CEnum(_))) {
                return Some(FieldShape::ValidatedPair);
            }
            return Some(FieldShape::Pair {
                elements: matches!(kind, Some(Kind::Boundary)).then_some(base),
            });
        }
        if field.array_len.is_some() || !field.pointer {
            // `FixedArray<T, N>` and a by-value use both read as `T`.
            if lang_scalar(base).is_some() {
                return Some(FieldShape::Bytes);
            }
            return Some(match kind {
                Some(Kind::Boundary) => FieldShape::Embedded(base),
                Some(Kind::StringView) => FieldShape::StringView,
                Some(Kind::ArrayPair(_)) => FieldShape::DescriptorAggregate,
                Some(Kind::FnPtr) => FieldShape::Callback,
                // An external type with no definition in this parse: the
                // checker reads it from the mirror that declares it.
                Some(Kind::Enum | Kind::Handle | Kind::Alias | Kind::External | Kind::CEnum(_))
                | None => FieldShape::Bytes,
            });
        }
        if base == "void" {
            return Some(FieldShape::Userdata);
        }
        Some(match kind {
            Some(Kind::Boundary) => FieldShape::Pointer {
                target: base,
                header: self.headers.contains(base),
                constant: field.is_const,
            },
            Some(Kind::FnPtr) => FieldShape::Callback,
            Some(Kind::Handle | Kind::External) => FieldShape::Bytes,
            Some(
                Kind::Enum | Kind::Alias | Kind::CEnum(_) | Kind::StringView | Kind::ArrayPair(_),
            )
            | None => FieldShape::Unlowered,
        })
    }
}

impl HeaderStructs<'_> {
    /// True when `aggregate` holds a string view or a pair, directly, by
    /// embedding, or behind a pointer member. This is the scope of the
    /// §28/§30 write-direction validation, not the pass decision: a
    /// callback alone also makes a scratch struct, and the binder lowers
    /// it with no validation.
    pub(crate) fn holds_view_or_pair(&self, aggregate: &str) -> bool {
        self.holds(aggregate, &mut Vec::new())
    }

    fn holds<'s>(&'s self, aggregate: &'s str, visiting: &mut Vec<&'s str>) -> bool {
        if visiting.contains(&aggregate)
            || !matches!(self.registry.get(aggregate), Some(Kind::Boundary))
        {
            return false;
        }
        let Some(fields) = self.struct_fields(aggregate) else {
            return false;
        };
        let Ok(pairs) = embedded_array_pairs(aggregate, fields, self.registry) else {
            return false;
        };
        visiting.push(aggregate);
        // A fixed array of structs is outside the scope.
        let holds = fields.iter().enumerate().any(|(index, field)| {
            match self.field_shape(&pairs, index, field) {
                Some(
                    FieldShape::StringView | FieldShape::Pair { .. } | FieldShape::ValidatedPair,
                ) => true,
                Some(FieldShape::Embedded(nested) | FieldShape::Pointer { target: nested, .. })
                    if field.array_len.is_none() =>
                {
                    self.holds(nested, visiting)
                }
                _ => false,
            }
        });
        visiting.pop();
        holds
    }
}

impl<'a> StructView for HeaderStructs<'a> {
    type Struct = &'a str;

    fn copying(&self) -> Option<&[&'a str]> {
        self.copying.as_deref()
    }

    fn cycles(&self) -> Option<&[&'a str]> {
        self.cycles.as_deref()
    }

    fn structs(&self) -> Vec<&'a str> {
        self.parsed
            .decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Struct { name, .. }
                    if matches!(self.registry.get(name), Some(Kind::Boundary)) =>
                {
                    Some(name.as_str())
                }
                _ => None,
            })
            .collect()
    }

    fn fields(&self, s: &'a str) -> Option<Vec<FieldShape<&'a str>>> {
        match self.cache.get(s) {
            Some(facts) => facts.fields.clone(),
            None => self.read_fields(s),
        }
    }
}

impl<'a> HeaderStructs<'a> {
    fn read_fields(&self, s: &'a str) -> Option<Vec<FieldShape<&'a str>>> {
        if !matches!(self.registry.get(s), Some(Kind::Boundary)) {
            return None;
        }
        let fields = self.struct_fields(s)?;
        // A malformed pair fails the binder elsewhere; here it has no
        // byte image.
        let pairs = embedded_array_pairs(s, fields, self.registry).ok()?;
        Some(
            fields
                .iter()
                .enumerate()
                .filter_map(|(index, field)| self.field_shape(&pairs, index, field))
                .collect(),
        )
    }
}

impl<'a> ReadView for HeaderStructs<'a> {
    fn name(&self, s: &'a str) -> String {
        s.to_string()
    }

    fn members(&self, s: &'a str) -> Option<Vec<ReadMember<&'a str>>> {
        match self.cache.get(s) {
            Some(facts) => facts.members.clone(),
            None => self.read_members(s),
        }
    }
}

impl<'a> HeaderStructs<'a> {
    /// The members of a boundary struct, and of a descriptor aggregate as
    /// a struct whose pointer field is its pair. A member is mutable when
    /// its pointer is not `const`. A member whose type this parse does not
    /// define reads as bytes: the checker reads it from the mirror that
    /// declares it (§187 rule 3).
    fn read_members(&self, s: &'a str) -> Option<Vec<ReadMember<&'a str>>> {
        let fields = self.struct_fields(s)?;
        let pairs = match self.registry.get(s) {
            Some(Kind::Boundary) => embedded_array_pairs(s, fields, self.registry).ok()?,
            Some(Kind::ArrayPair(element)) => super::descriptor_pair(fields, element),
            _ => return None,
        };
        Some(
            fields
                .iter()
                .enumerate()
                .filter_map(|(index, field)| {
                    let shape = match self.field_shape(&pairs, index, field)? {
                        FieldShape::Unlowered if !self.registry.contains_key(&field.base) => {
                            FieldShape::Bytes
                        }
                        shape => shape,
                    };
                    Some(ReadMember::new(field.name.clone(), shape, !field.is_const))
                })
                .collect(),
        )
    }
}
