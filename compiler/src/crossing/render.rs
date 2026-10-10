//! The text of `subscript boundary` (`specs/blocks/compiler.md` §189 rule
//! 5): the format and the closed word set, in one place.
//!
//! One line per call site and crossing position:
//! `<file>:<line>:<col> <callee> <path>: <what> [<cause>, ...]`, then
//! `writes-back <member> ...` when the call writes back. A fact that depends
//! on run-time data is a rule of the word (§189 rule 6). A member that the
//! build copies as bytes has no line of its own.

use super::{
    call_sites, CallPlan, CallSite, ElementsPlan, MemberCrossing, NotLowered, PairPlan,
    ParameterPlan, PointerPlan, ResultPlan, StructPlan, WriteBackPlan,
};
use crate::boundary_pass::Classes;
use crate::lir as l;
use crate::types::ClassId;
use subscript_boundary::{FieldShape, PointerPass, StructPass, StructView};

/// The closed set of `<what>` words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Word {
    /// A scalar, an enum, a wire alias, or a handle.
    Value,
    /// A by-value struct whose bytes the call copies (§187 rule 10).
    ByValueBytes,
    /// A by-value struct that the call builds as a scratch struct.
    ByValueScratch,
    /// The call passes the script memory.
    ScriptMemory,
    /// A scratch copy that the call does not write back.
    ScratchReadOnly,
    /// A scratch copy that the call writes back (§187 rule 7).
    ScratchWrittenBack,
    /// An embedded struct that the build copies as bytes.
    EmbeddedBytes,
    /// An embedded struct that the build builds member by member.
    EmbeddedScratch,
    /// A collapsed pair.
    Pair,
    /// Pair elements that the call copies into a scratch array.
    ScratchPerElement,
    /// A string view of the script bytes.
    StringView,
    /// A callback that binds for the Context lifetime.
    CallbackBinding,
    /// A callback with an explicit-lifetime registration (§111).
    CallbackRegistration,
    /// The completion endpoint of a host completion.
    CompletionEndpoint,
    /// A form that code generation does not lower.
    NotLowered,
    /// A struct result that the script reads as its bytes.
    ReadBytes,
    /// A struct result whose read goes through a pointer or a pair.
    ReadByMembers,
}

impl Word {
    /// Every word, in the order of the contract.
    pub const ALL: [Word; 17] = [
        Word::Value,
        Word::ByValueBytes,
        Word::ByValueScratch,
        Word::ScriptMemory,
        Word::ScratchReadOnly,
        Word::ScratchWrittenBack,
        Word::EmbeddedBytes,
        Word::EmbeddedScratch,
        Word::Pair,
        Word::ScratchPerElement,
        Word::StringView,
        Word::CallbackBinding,
        Word::CallbackRegistration,
        Word::CompletionEndpoint,
        Word::NotLowered,
        Word::ReadBytes,
        Word::ReadByMembers,
    ];

    /// The text of the word.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::ByValueBytes => "by-value bytes",
            Self::ByValueScratch => "by-value scratch",
            Self::ScriptMemory => "script memory",
            Self::ScratchReadOnly => "scratch read-only, none for null",
            Self::ScratchWrittenBack => "scratch written-back if changed, none for null",
            Self::EmbeddedBytes => "embedded bytes",
            Self::EmbeddedScratch => "embedded scratch",
            Self::Pair => "pair",
            Self::ScratchPerElement => "scratch read-only per element",
            Self::StringView => "string view of script bytes",
            Self::CallbackBinding => "callback binding",
            Self::CallbackRegistration => "callback registration",
            Self::CompletionEndpoint => "completion endpoint",
            Self::NotLowered => "not lowered:",
            Self::ReadBytes => "read bytes",
            Self::ReadByMembers => "read by members",
        }
    }
}

/// The word of a member that makes a struct a scratch struct.
fn cause_word(shape: &FieldShape<ClassId>) -> &'static str {
    match shape {
        FieldShape::StringView => "string view",
        FieldShape::Pair { .. } => "pair",
        FieldShape::ValidatedPair => "validated pair",
        FieldShape::Callback => "callback",
        FieldShape::DescriptorAggregate => "descriptor aggregate",
        FieldShape::Embedded(_) => "embedded scratch struct",
        FieldShape::Pointer { header: true, .. } => "link to scratch header",
        FieldShape::Pointer { constant: true, .. } => "const pointer to scratch struct",
        FieldShape::Pointer { .. } => "non-const pointer",
        FieldShape::Bytes | FieldShape::Userdata => "bytes",
        _ => "no lowering",
    }
}

/// The lines of every foreign call site of `module`, sorted by position.
/// Sites at one position keep their LIR order.
///
/// # Errors
///
/// An internal error from [`super::call_plan`].
pub fn report(module: &l::Module) -> Result<Vec<String>, String> {
    let mut sites = call_sites(module)?;
    sites.sort_by(|a, b| {
        (a.pos.file.as_str(), a.pos.line, a.pos.col).cmp(&(
            b.pos.file.as_str(),
            b.pos.line,
            b.pos.col,
        ))
    });
    let mut lines = Vec::new();
    for site in &sites {
        lines.extend(render_site(module, site)?);
    }
    Ok(lines)
}

/// The lines of one call site.
///
/// # Errors
///
/// An internal error when the module does not define the callee or a class
/// that the plan names.
pub fn render_site(module: &l::Module, site: &CallSite) -> Result<Vec<String>, String> {
    let declaration = module
        .foreign_functions
        .get(site.callee.0 as usize)
        .filter(|declaration| declaration.id == site.callee)
        .ok_or_else(|| super::internal(format!("foreign function {} is missing", site.callee.0)))?;
    let mut writer = Writer {
        module,
        prefix: format!(
            "{}:{}:{} {}",
            site.pos.file, site.pos.line, site.pos.col, declaration.source_name
        ),
        lines: Vec::new(),
    };
    writer.call(declaration, &site.plan)?;
    Ok(writer.lines)
}

struct Writer<'m> {
    module: &'m l::Module,
    prefix: String,
    lines: Vec<String>,
}

impl Writer<'_> {
    fn class(&self, id: ClassId) -> Result<&l::Class, String> {
        self.module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| super::internal(format!("boundary class {} is missing", id.0)))
    }

    fn line(&mut self, path: &str, what: &str, causes: &[String], writes_back: &[String]) {
        let mut line = format!("{} {path}: {what}", self.prefix);
        if !causes.is_empty() {
            line.push_str(&format!(" [{}]", causes.join(", ")));
        }
        if !writes_back.is_empty() {
            line.push_str(&format!(" writes-back {}", writes_back.join(" ")));
        }
        self.lines.push(line);
    }

    fn call(&mut self, declaration: &l::ForeignFunction, plan: &CallPlan) -> Result<(), String> {
        for (parameter, plan) in declaration.parameters.iter().zip(&plan.parameters) {
            let path = parameter.source_name.as_str();
            match plan {
                ParameterPlan::Value => self.line(path, Word::Value.text(), &[], &[]),
                ParameterPlan::StringView => self.line(path, Word::StringView.text(), &[], &[]),
                ParameterPlan::CompletionEndpoint => {
                    self.line(path, Word::CompletionEndpoint.text(), &[], &[]);
                }
                ParameterPlan::Pair(pair) => self.pair(path, pair)?,
                ParameterPlan::ByValue(structure) => {
                    let word = match structure.pass {
                        StructPass::Bytes => Word::ByValueBytes,
                        StructPass::Scratch => Word::ByValueScratch,
                        StructPass::Cycle => {
                            return self.not_lowered(path, NotLowered::StructCycle.reason());
                        }
                    };
                    self.structure(path, word, structure, None)?;
                }
                ParameterPlan::Pointer(pointer) => self.pointer(path, pointer)?,
            }
        }
        match &plan.result {
            ResultPlan::Void => {}
            ResultPlan::Value => self.line("result", Word::Value.text(), &[], &[]),
            ResultPlan::Struct(read) => {
                let class = self.class(read.class)?;
                let causes = read
                    .by_members
                    .iter()
                    .filter_map(|index| class.fields.get(*index))
                    .map(|field| format!("{}.{}", class.source_name, field.source_name))
                    .collect::<Vec<_>>();
                let word = if causes.is_empty() {
                    Word::ReadBytes
                } else {
                    Word::ReadByMembers
                };
                self.line("result", word.text(), &causes, &[]);
            }
        }
        Ok(())
    }

    fn not_lowered(&mut self, path: &str, reason: &str) -> Result<(), String> {
        self.line(
            path,
            &format!("{} {reason}", Word::NotLowered.text()),
            &[],
            &[],
        );
        Ok(())
    }

    fn causes(&self, structure: &StructPlan) -> Result<Vec<String>, String> {
        let class = self.class(structure.class)?;
        let shapes = Classes(self.module)
            .fields(structure.class)
            .unwrap_or_default();
        Ok(structure
            .causes
            .iter()
            .filter_map(|index| Some((class.fields.get(*index)?, shapes.get(*index)?)))
            .map(|(field, shape)| {
                format!(
                    "{}.{} {}",
                    class.source_name,
                    field.source_name,
                    cause_word(shape)
                )
            })
            .collect())
    }

    fn written(&self, write_back: Option<&WriteBackPlan>) -> Result<Vec<String>, String> {
        let Some(write_back) = write_back else {
            return Ok(Vec::new());
        };
        let class = self.class(write_back.class)?;
        Ok(write_back
            .members
            .iter()
            .filter_map(|member| class.fields.get(member.field))
            .map(|field| field.source_name.clone())
            .collect())
    }

    /// The line of a struct node, then the lines of its members.
    fn structure(
        &mut self,
        path: &str,
        word: Word,
        structure: &StructPlan,
        write_back: Option<&WriteBackPlan>,
    ) -> Result<(), String> {
        let causes = self.causes(structure)?;
        let written = self.written(write_back)?;
        self.line(path, word.text(), &causes, &written);
        self.members(path, structure)
    }

    fn members(&mut self, path: &str, structure: &StructPlan) -> Result<(), String> {
        let class = self.class(structure.class)?;
        let names = class
            .fields
            .iter()
            .map(|field| field.source_name.clone())
            .collect::<Vec<_>>();
        for member in &structure.members {
            let name = names
                .get(member.field)
                .ok_or_else(|| super::internal("a plan member has no field"))?;
            let path = format!("{path}.{name}");
            match &member.crossing {
                MemberCrossing::Bytes | MemberCrossing::FixedBytes => {}
                MemberCrossing::StringView => self.line(&path, Word::StringView.text(), &[], &[]),
                MemberCrossing::Callback(callback) => {
                    let word = if callback.explicit {
                        Word::CallbackRegistration
                    } else {
                        Word::CallbackBinding
                    };
                    self.line(&path, word.text(), &[], &[]);
                }
                MemberCrossing::Pair(pair) => self.pair(&path, pair)?,
                MemberCrossing::Embedded(nested) => match nested.pass {
                    StructPass::Bytes => self.line(&path, Word::EmbeddedBytes.text(), &[], &[]),
                    StructPass::Scratch => {
                        self.structure(&path, Word::EmbeddedScratch, nested, None)?;
                    }
                    StructPass::Cycle => {
                        self.not_lowered(&path, NotLowered::StructCycle.reason())?
                    }
                },
                MemberCrossing::Pointer(pointer) => self.pointer(&path, pointer)?,
                MemberCrossing::NotLowered(reason) => self.not_lowered(&path, reason.reason())?,
            }
        }
        Ok(())
    }

    fn pointer(&mut self, path: &str, pointer: &PointerPlan) -> Result<(), String> {
        let word = match pointer.pass {
            PointerPass::ScriptMemory => {
                self.line(path, Word::ScriptMemory.text(), &[], &[]);
                return Ok(());
            }
            PointerPass::Cycle => return self.not_lowered(path, NotLowered::StructCycle.reason()),
            PointerPass::ScratchReadOnly => Word::ScratchReadOnly,
            PointerPass::ScratchWrittenBack => Word::ScratchWrittenBack,
        };
        let target = pointer
            .target
            .as_ref()
            .ok_or_else(|| super::internal("a scratch pointer has no target plan"))?;
        self.structure(path, word, target, pointer.write_back.as_ref())
    }

    fn pair(&mut self, path: &str, pair: &PairPlan) -> Result<(), String> {
        self.line(path, Word::Pair.text(), &[], &[]);
        let path = format!("{path}[]");
        match &pair.elements {
            None => self.line(&path, Word::ScriptMemory.text(), &[], &[]),
            Some(ElementsPlan {
                pass: PointerPass::ScratchReadOnly,
                element: Some(element),
                ..
            }) => self.structure(&path, Word::ScratchPerElement, element, None)?,
            Some(ElementsPlan {
                pass: PointerPass::Cycle,
                ..
            }) => self.not_lowered(&path, NotLowered::StructCycle.reason())?,
            Some(_) => self.not_lowered(&path, NotLowered::WrittenBackElements.reason())?,
        }
        Ok(())
    }
}
