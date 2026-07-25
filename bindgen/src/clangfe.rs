//! libclang-based C frontend (plan P6.1, `specs/blocks/compiler.md`
//! §13.1). Replaces the narrow fixture parser (`cparse`) as the shipped
//! frontend for [`crate::generate`]. It parses a real C header — including
//! preprocessor `#define`s, function/nullable attribute macros, doc
//! comments, `typedef`, nested structs, function-pointer typedefs,
//! `static const` constants, enums, and scalar (flag) typedefs — into the
//! same [`CField`]/[`Decl`] internal representation that
//! [`crate::emit`](../emit/index.html) already consumes, plus the extra
//! production-C facts ([`Parsed`]) that P6.2 will map.
//!
//! # libclang location
//!
//! The `clang-sys` `runtime` feature dlopens libclang at run time. The
//! shared library is located by `clang-sys`'s own search, which honours
//! the `LIBCLANG_PATH` environment variable first and otherwise searches
//! the platform defaults (on macOS this includes the Command Line Tools
//! directory `…/CommandLineTools/usr/lib`). As a documented fallback, when
//! `LIBCLANG_PATH` is unset this module points it at that Command Line
//! Tools directory *only when the library actually exists there*, so the
//! path never breaks a machine that keeps libclang elsewhere (set
//! `LIBCLANG_PATH` to override). A missing libclang is a returned error,
//! never a panic.

// The clang-sys cursor/type-kind constants (`CXType_Record`,
// `CXCursor_TypedefDecl`, …) are external binding names, not ours; matching
// them in patterns trips the style-only `non_upper_case_globals` lint. This
// allow is scoped to this frontend module and covers only those names.
#![allow(non_upper_case_globals)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Once;

use clang_sys::*;

use crate::cparse::{CField, Decl, ParseError};

/// Everything the frontend extracts from a header: the boundary
/// declarations consumed by the emitter, plus the production-C facts
/// (macros, constants, scalar/flag typedef aliases, doc comments) that the
/// P6.2 shape mapping will use. P6.1 emits only from [`Parsed::decls`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Parsed {
    /// Boundary declarations in source order (the emitter's input).
    pub decls: Vec<Decl>,
    /// `#define` macros defined in the main file, in source order.
    pub macros: Vec<Macro>,
    /// File-scope constants (`static const …`, `const …`) with an integer
    /// value, in source order.
    pub constants: Vec<Constant>,
    /// Scalar `typedef`s that are not structs/enums/handles/function
    /// pointers — e.g. a flag typedef `typedef uint64_t XFlags;`.
    pub aliases: Vec<Alias>,
    /// Raw doc comments keyed by the declared name they document.
    pub docs: Vec<(String, String)>,
}

/// A `#define` macro definition seen in the main file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Macro {
    /// Macro name.
    pub name: String,
    /// True for a function-like macro (`#define F(x) …`).
    pub function_like: bool,
}

/// A file-scope integer constant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Constant {
    /// Declared name.
    pub name: String,
    /// Base type spelling (`const`/`struct` stripped), e.g. `int`,
    /// `SubFlags`.
    pub type_base: String,
    /// Evaluated integer value.
    pub value: i64,
}

/// A scalar `typedef` alias (`typedef <base> <name>;`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Alias {
    /// Alias name.
    pub name: String,
    /// Immediate underlying type spelling as written, e.g. `uint64_t`
    /// (the first link of [`Alias::chain`]).
    pub underlying: String,
    /// The typedef chain under this alias, from the immediate underlying
    /// type to the canonical builtin, each spelling stripped of
    /// `const`/`struct`/… (compiler.md §14.1). A two-level flag alias
    /// `typedef uint32_t B; typedef B X;` records `["B", "uint32_t",
    /// "unsigned int"]` for `X`, so the emitter can follow the chain to the
    /// first spelling that maps to a language integer. Empty for a pointer
    /// alias.
    pub chain: Vec<String>,
}

/// Parses `source` as a C header via libclang.
///
/// The `source` is presented to libclang as an in-memory (unsaved) file,
/// so no temporary file is written; system/builtin headers such as
/// `<stdint.h>` resolve from libclang's own resource directory.
///
/// # Errors
///
/// Returns [`ParseError`] when libclang cannot be loaded, when the
/// translation unit fails to parse, or when a declaration uses a construct
/// the frontend does not model.
pub fn parse(source: &str) -> Result<Parsed, ParseError> {
    ensure_libclang()?;
    // SAFETY: libclang is loaded (checked above). Every raw handle created
    // below is disposed before this function returns, and no borrowed
    // pointer outlives the object it points into.
    unsafe { parse_inner(source) }
}

/// Loads libclang on the current thread if it is not already loaded.
fn ensure_libclang() -> Result<(), ParseError> {
    // Point `LIBCLANG_PATH` at the platform default once, only if the user
    // has not set it and libclang actually lives there. `clang-sys`
    // already searches these defaults; this makes the fallback explicit
    // and never references a path that does not exist on this machine.
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var_os("LIBCLANG_PATH").is_none() {
            for dir in DEFAULT_LIBCLANG_DIRS {
                if dir_has_libclang(dir) {
                    std::env::set_var("LIBCLANG_PATH", dir);
                    break;
                }
            }
        }
    });

    if clang_createIndex::is_loaded() {
        return Ok(());
    }
    load().map_err(|e| {
        ParseError(format!(
            "libclang could not be loaded ({e}); set LIBCLANG_PATH to the \
             directory containing the libclang shared library"
        ))
    })
}

/// Platform default directories to try when `LIBCLANG_PATH` is unset. Kept
/// deliberately short; `clang-sys` performs the broader search.
const DEFAULT_LIBCLANG_DIRS: &[&str] = &[
    // macOS Command Line Tools.
    "/Library/Developer/CommandLineTools/usr/lib",
    // Common Linux locations.
    "/usr/lib",
    "/usr/lib/llvm/lib",
];

/// True when a directory contains a libclang shared library under any of
/// the platform-specific names.
fn dir_has_libclang(dir: &str) -> bool {
    const NAMES: &[&str] = &["libclang.dylib", "libclang.so", "libclang.dll"];
    NAMES.iter().any(|n| Path::new(dir).join(n).exists())
}

/// The unsafe core: builds a translation unit and walks it. Callers must
/// have loaded libclang.
unsafe fn parse_inner(source: &str) -> Result<Parsed, ParseError> {
    let index = clang_createIndex(0, 0);
    if index.is_null() {
        return Err(ParseError("libclang: could not create an index".into()));
    }
    let _index = IndexGuard(index);

    // Present the source as an unsaved file named `header.h`; that name
    // becomes the translation unit's main file, so `isFromMainFile`
    // separates our declarations from included system headers.
    let filename = CString::new("header.h").expect("no NUL in literal");
    let contents = CString::new(source)
        .map_err(|_| ParseError("header source contains a NUL byte".into()))?;
    let mut unsaved = CXUnsavedFile {
        Filename: filename.as_ptr(),
        Contents: contents.as_ptr(),
        Length: source.len() as std::os::raw::c_ulong,
    };
    // Parse as C11; request a detailed preprocessing record so `#define`
    // macros are represented as cursors.
    let args: [CString; 3] = [
        CString::new("-x").expect("no NUL in literal"),
        CString::new("c").expect("no NUL in literal"),
        CString::new("-std=c11").expect("no NUL in literal"),
    ];
    let arg_ptrs: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
    let tu = clang_parseTranslationUnit(
        index,
        filename.as_ptr(),
        arg_ptrs.as_ptr(),
        arg_ptrs.len() as std::os::raw::c_int,
        &mut unsaved,
        1,
        CXTranslationUnit_DetailedPreprocessingRecord,
    );
    if tu.is_null() {
        return Err(ParseError("libclang: failed to parse the header".into()));
    }
    let _tu = TuGuard(tu);

    // Fail loud on a fatal parse error (e.g. a missing include); warnings
    // are tolerated.
    if let Some(msg) = first_fatal_diagnostic(tu) {
        return Err(ParseError(format!("libclang: {msg}")));
    }

    let root = clang_getTranslationUnitCursor(tu);
    let mut parsed = Parsed::default();
    for cursor in children(root) {
        if is_from_main_file(cursor) {
            visit_top_level(cursor, &mut parsed)?;
        }
    }
    Ok(parsed)
}

/// Handles one top-level cursor, appending to `parsed`.
unsafe fn visit_top_level(cursor: CXCursor, parsed: &mut Parsed) -> Result<(), ParseError> {
    match clang_getCursorKind(cursor) {
        CXCursor_TypedefDecl => visit_typedef(cursor, parsed)?,
        CXCursor_FunctionDecl => {
            let name = cursor_spelling(cursor);
            let ret = strip_name(field_from_type(clang_getCursorResultType(cursor), String::new()));
            let params = function_params(cursor);
            record_doc(cursor, &name, parsed);
            parsed.decls.push(Decl::Func { name, ret, params });
        }
        CXCursor_VarDecl => {
            if let Some(constant) = constant_from_var(cursor) {
                parsed.constants.push(constant);
            }
        }
        CXCursor_MacroDefinition => {
            let name = cursor_spelling(cursor);
            // An include-guard's own symbol cannot be told apart from a
            // real object-like macro reliably, so all main-file macros are
            // kept; downstream decides which matter.
            parsed.macros.push(Macro {
                name,
                function_like: clang_Cursor_isMacroFunctionLike(cursor) != 0,
            });
        }
        // Bare struct/enum/union definitions are reached through their
        // typedef; skip the standalone cursor to avoid a duplicate. Other
        // cursor kinds (macro expansions, `#include` markers, attributes)
        // carry no boundary declaration.
        _ => {}
    }
    Ok(())
}

/// Classifies a `typedef` by its underlying (canonical) type and appends
/// the matching declaration or alias.
unsafe fn visit_typedef(cursor: CXCursor, parsed: &mut Parsed) -> Result<(), ParseError> {
    let name = cursor_spelling(cursor);
    let under = clang_getTypedefDeclUnderlyingType(cursor);
    let canon = clang_getCanonicalType(under);
    match canon.kind {
        CXType_Enum => {
            record_doc(cursor, &name, parsed);
            let decl = clang_getTypeDeclaration(canon);
            let members = enum_members(decl);
            parsed.decls.push(Decl::Enum { name, members });
        }
        CXType_Record => {
            record_doc(cursor, &name, parsed);
            let decl = clang_getTypeDeclaration(canon);
            validate_record_layout(&name, cursor, canon, decl)?;
            let fields = struct_fields(decl);
            parsed.decls.push(Decl::Struct { name, fields });
        }
        CXType_Pointer => {
            let pointee = clang_getCanonicalType(clang_getPointeeType(canon));
            match pointee.kind {
                // `typedef struct X_T *X;` — opaque handle.
                CXType_Record => {
                    let decl = clang_getTypeDeclaration(pointee);
                    validate_record_layout(&name, cursor, pointee, decl)?;
                    record_doc(cursor, &name, parsed);
                    parsed.decls.push(Decl::Handle { name });
                }
                // `typedef R (*X)(params);` — function pointer.
                CXType_FunctionProto => {
                    record_doc(cursor, &name, parsed);
                    let ret = strip_name(field_from_type(clang_getResultType(pointee), String::new()));
                    let params = typedef_params(cursor);
                    parsed.decls.push(Decl::FnPtr { name, ret, params });
                }
                // Pointer to a scalar/other — record as a plain alias with
                // no integer chain (a pointer never maps to a language
                // integer; its use site fails loud).
                _ => parsed.aliases.push(Alias {
                    name,
                    underlying: type_spelling(under),
                    chain: Vec::new(),
                }),
            }
        }
        // Scalar typedef, including flag typedefs (`typedef uintN XFlags`)
        // and multi-level flag aliases (`typedef uint32_t B; typedef B X`,
        // §14.1). The chain is followed so the emitter can resolve any
        // depth that bottoms out in a mapped integer.
        _ => parsed.aliases.push(Alias {
            name,
            underlying: type_spelling(under),
            chain: typedef_chain(under),
        }),
    }
    Ok(())
}

/// Collects the typedef chain under a scalar typedef's immediate underlying
/// type, from that immediate type down to the canonical builtin, each as a
/// stripped base spelling (compiler.md §14.1). Each link is one typedef
/// level: `typedef uint32_t B; typedef B X;` yields `["B", "uint32_t",
/// "unsigned int"]` under `X`. The walk is bounded so a pathological input
/// cannot loop.
unsafe fn typedef_chain(under: CXType) -> Vec<String> {
    let mut chain = Vec::new();
    let mut t = under;
    for _ in 0..64 {
        chain.push(base_spelling(t));
        let decl = clang_getTypeDeclaration(t);
        if clang_getCursorKind(decl) != CXCursor_TypedefDecl {
            break;
        }
        t = clang_getTypedefDeclUnderlyingType(decl);
    }
    chain
}

/// Reads the members (name, value) of an enum declaration, in order.
unsafe fn enum_members(decl: CXCursor) -> Vec<(String, i64)> {
    let mut members = Vec::new();
    for child in children(decl) {
        if clang_getCursorKind(child) == CXCursor_EnumConstantDecl {
            let name = cursor_spelling(child);
            let value = clang_getEnumConstantDeclValue(child) as i64;
            members.push((name, value));
        }
    }
    members
}

/// Rejects records whose C layout the language cannot reproduce.
unsafe fn validate_record_layout(
    name: &str,
    typedef_cursor: CXCursor,
    record_ty: CXType,
    decl: CXCursor,
) -> Result<(), ParseError> {
    if clang_getCursorKind(decl) == CXCursor_UnionDecl {
        return Err(ParseError(format!(
            "record `{name}` is a union; the language cannot reproduce union layout"
        )));
    }

    let decl_children = children(decl);
    if children(typedef_cursor)
        .iter()
        .chain(decl_children.iter())
        .any(|child| clang_getCursorKind(*child) == CXCursor_PackedAttr)
    {
        return Err(ParseError(format!(
            "record `{name}` uses packed layout; the language cannot reproduce its field offsets"
        )));
    }
    if children(typedef_cursor)
        .iter()
        .chain(decl_children.iter())
        .any(|child| clang_getCursorKind(*child) == CXCursor_AlignedAttr)
    {
        let align = clang_Type_getAlignOf(record_ty);
        return Err(ParseError(format!(
            "record `{name}` is explicitly aligned to {align} bytes; the language cannot reproduce that alignment"
        )));
    }

    let mut natural_align = 1i64;
    for child in &decl_children {
        if clang_getCursorKind(*child) != CXCursor_FieldDecl {
            continue;
        }
        let field_name = cursor_spelling(*child);
        if clang_Cursor_isBitField(*child) != 0 {
            return Err(ParseError(format!(
                "record `{name}` contains bitfield member `{field_name}`; the language cannot reproduce bitfield layout"
            )));
        }
        if children(*child)
            .iter()
            .any(|attr| clang_getCursorKind(*attr) == CXCursor_AlignedAttr)
        {
            return Err(ParseError(format!(
                "record `{name}` contains explicitly aligned member `{field_name}`; the language cannot reproduce its field offsets"
            )));
        }
        let align = clang_Type_getAlignOf(clang_getCursorType(*child));
        if align > 0 {
            natural_align = natural_align.max(align);
        }
    }
    let record_align = clang_Type_getAlignOf(record_ty);
    if record_align > 0 && record_align < natural_align {
        return Err(ParseError(format!(
            "record `{name}` uses packed layout; the language cannot reproduce its field offsets"
        )));
    }
    if record_align > natural_align {
        return Err(ParseError(format!(
            "record `{name}` is over-aligned to {record_align} bytes; the language cannot reproduce that alignment"
        )));
    }
    Ok(())
}

/// Reads the fields of a struct declaration, in order.
unsafe fn struct_fields(decl: CXCursor) -> Vec<CField> {
    let mut fields = Vec::new();
    for child in children(decl) {
        if clang_getCursorKind(child) == CXCursor_FieldDecl {
            let name = cursor_spelling(child);
            fields.push(field_from_type(clang_getCursorType(child), name));
        }
    }
    fields
}

/// Reads a function declaration's parameters (from `ParmDecl` cursors so
/// the parameter names are preserved), in order.
unsafe fn function_params(cursor: CXCursor) -> Vec<CField> {
    let n = clang_Cursor_getNumArguments(cursor);
    if n < 0 {
        return Vec::new();
    }
    (0..n as u32)
        .map(|i| {
            let arg = clang_Cursor_getArgument(cursor, i);
            field_from_type(clang_getCursorType(arg), cursor_spelling(arg))
        })
        .collect()
}

/// Reads a function-pointer typedef's parameters from the `ParmDecl`
/// children of the typedef cursor (which carry the parameter names).
unsafe fn typedef_params(cursor: CXCursor) -> Vec<CField> {
    let mut params = Vec::new();
    for child in children(cursor) {
        if clang_getCursorKind(child) == CXCursor_ParmDecl {
            let name = cursor_spelling(child);
            params.push(field_from_type(clang_getCursorType(child), name));
        }
    }
    params
}

/// Builds a [`CField`] from a clang type and a declared name, reproducing
/// exactly the base spelling and `const`/pointer/array flags that `cparse`
/// produced, so the emitter's output is byte-identical.
unsafe fn field_from_type(ty: CXType, name: String) -> CField {
    let mut t = ty;
    let mut array_len = None;
    if t.kind == CXType_ConstantArray {
        let n = clang_getArraySize(t);
        if n >= 0 {
            array_len = Some(n as u32);
        }
        t = clang_getArrayElementType(t);
    }
    let mut pointer = false;
    let is_const;
    if t.kind == CXType_Pointer {
        pointer = true;
        let pointee = clang_getPointeeType(t);
        is_const = clang_isConstQualifiedType(pointee) != 0;
        t = pointee;
    } else {
        is_const = clang_isConstQualifiedType(t) != 0;
    }
    CField {
        base: base_spelling(t),
        is_const,
        pointer,
        array_len,
        name,
    }
}

/// The base type name with leading `const`/`struct`/`union`/`enum`/
/// `volatile` and any trailing `const` removed. `_Bool`/`bool` and `void`
/// are normalised to the spellings the emitter's scalar table expects.
unsafe fn base_spelling(t: CXType) -> String {
    match t.kind {
        // `<stdbool.h>` makes `bool` a macro for `_Bool`; the emitter's
        // scalar table keys on `bool`.
        CXType_Bool => return "bool".to_string(),
        CXType_Void => return "void".to_string(),
        // Plain `char` is target-dependent. Preserve the ambiguous source
        // spelling instead of importing libclang's host-default target
        // signedness (compiler.md §16.1).
        CXType_Char_S | CXType_Char_U => return "char".to_string(),
        _ => {}
    }
    let mut s = type_spelling(t);
    let mut changed = true;
    while changed {
        changed = false;
        for kw in ["const ", "struct ", "union ", "enum ", "volatile "] {
            if let Some(rest) = s.strip_prefix(kw) {
                s = rest.trim_start().to_string();
                changed = true;
            }
        }
    }
    if let Some(rest) = s.strip_suffix(" const") {
        s = rest.trim_end().to_string();
    }
    s
}

/// Builds a [`Constant`] from a file-scope `VarDecl` whose initializer
/// evaluates to an integer; returns `None` otherwise.
unsafe fn constant_from_var(cursor: CXCursor) -> Option<Constant> {
    let eval = clang_Cursor_Evaluate(cursor);
    if eval.is_null() {
        return None;
    }
    let kind = clang_EvalResult_getKind(eval);
    let value = if kind == CXEval_Int {
        Some(clang_EvalResult_getAsLongLong(eval) as i64)
    } else {
        None
    };
    clang_EvalResult_dispose(eval);
    let value = value?;
    Some(Constant {
        name: cursor_spelling(cursor),
        type_base: base_spelling(clang_getCursorType(cursor)),
        value,
    })
}

/// Records a declaration's raw doc comment, if any, keyed by `name`.
unsafe fn record_doc(cursor: CXCursor, name: &str, parsed: &mut Parsed) {
    let doc = cxstring(clang_Cursor_getRawCommentText(cursor));
    if !doc.is_empty() {
        parsed.docs.push((name.to_string(), doc));
    }
}

/// Clears the name of a return-type field.
fn strip_name(mut f: CField) -> CField {
    f.name.clear();
    f
}

/// Returns the first fatal-error diagnostic message, if any.
unsafe fn first_fatal_diagnostic(tu: CXTranslationUnit) -> Option<String> {
    let n = clang_getNumDiagnostics(tu);
    for i in 0..n {
        let diag = clang_getDiagnostic(tu, i);
        let sev = clang_getDiagnosticSeverity(diag);
        let msg = if sev >= CXDiagnostic_Error {
            Some(cxstring(clang_formatDiagnostic(diag, 0)))
        } else {
            None
        };
        clang_disposeDiagnostic(diag);
        if let Some(msg) = msg {
            return Some(msg);
        }
    }
    None
}

/// Collects the direct children of a cursor (no recursion).
unsafe fn children(cursor: CXCursor) -> Vec<CXCursor> {
    let mut out: Vec<CXCursor> = Vec::new();
    clang_visitChildren(
        cursor,
        collect_child,
        &mut out as *mut Vec<CXCursor> as CXClientData,
    );
    out
}

/// Visitor callback for [`children`]: pushes each child and does not
/// recurse.
extern "C" fn collect_child(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    // SAFETY: `data` is the `&mut Vec<CXCursor>` passed by `children`,
    // valid for the whole traversal; the visitor runs synchronously on the
    // same thread.
    let out = unsafe { &mut *(data as *mut Vec<CXCursor>) };
    out.push(cursor);
    CXChildVisit_Continue
}

/// True when a cursor's location is in the translation unit's main file.
unsafe fn is_from_main_file(cursor: CXCursor) -> bool {
    clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0
}

/// The spelling (name) of a cursor.
unsafe fn cursor_spelling(cursor: CXCursor) -> String {
    cxstring(clang_getCursorSpelling(cursor))
}

/// The spelling of a type.
unsafe fn type_spelling(t: CXType) -> String {
    cxstring(clang_getTypeSpelling(t))
}

/// Converts a `CXString` to an owned Rust `String` and disposes it.
unsafe fn cxstring(s: CXString) -> String {
    let ptr = clang_getCString(s);
    let out = if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    };
    clang_disposeString(s);
    out
}

/// Disposes a `CXIndex` on drop.
struct IndexGuard(CXIndex);
impl Drop for IndexGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live index created by `clang_createIndex`
        // and disposed exactly once, here, after the TU it owns.
        unsafe { clang_disposeIndex(self.0) };
    }
}

/// Disposes a `CXTranslationUnit` on drop.
struct TuGuard(CXTranslationUnit);
impl Drop for TuGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live TU from `clang_parseTranslationUnit`,
        // disposed exactly once, here, before its owning index.
        unsafe { clang_disposeTranslationUnit(self.0) };
    }
}
