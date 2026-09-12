//! LIR-to-C transcriber for the shipping tier.
//!
//! LIR fixes evaluation order, control flow, entity identity, traps, and
//! suspension state. This module assigns C storage and writes those blocks
//! and instructions without consulting typed HIR.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use subscript_compiler::lir as l;
use subscript_compiler::types::{ClassId, Type};
use subscript_compiler::Pos;
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{is_unsigned, type_contains_managed, Layouts};
use crate::lir::verify_module;
use crate::lir_types::{
    array_element_kind, array_format_kind, association_key_kind, boundary_class_contains_pointer,
    boundary_class_needs_scratch, boundary_class_requires_build, boundary_type_requires_build,
    capture_parameters, data_type, explicit_parameters, foreign_parameter_type_matches,
    is_userdata_slot, operand_type, runtime_trap_kind, value_type,
};
use crate::root_storage::{self, RootStoragePlan};

mod access;
mod analysis;
mod arith;
mod body;
mod call;
mod collection;
mod emitter;
mod graph;
mod intrinsic;
mod iterator;
mod literal;
mod marshal;
mod suspend;
mod terminator;
mod verify;
mod worker;

use self::analysis::{
    coalesced_value_storage, declaration_can_use_instruction_assignment, declaration_scopes,
    fixed_iterator_values, foldable_local_addresses, promoted_local_values,
    removable_block_parameter_copies,
};
use self::verify::{
    runtime_traps, script_call_requires_pending_check, verify_no_empty_aggregate,
    verify_no_label_before_declaration, verify_trap_consumption,
};

/// An emitted C translation unit and its source-position metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CProgram {
    /// Complete C source.
    pub source: String,
    /// Trap position table indexed by generated `pos_id` values.
    pub positions: Vec<Pos>,
    /// Generated allocation metadata declarations.
    pub allocation_metadata_header: String,
    /// Generated allocation metadata definitions.
    pub allocation_metadata_source: String,
    pub(crate) foreign_symbols: Vec<String>,
}

impl CProgram {
    /// Transcribes a verified LIR module to ship-tier C.
    ///
    /// If `require_main` is true, the module must export `main(): void`.
    /// Returns an error for invalid LIR or unsupported C transcription.
    pub fn from_lir(module: &l::Module, require_main: bool) -> Result<Self, String> {
        emit_lir_c(module, require_main)
    }
}

/// Transcribes one verified LIR module to ship-tier C.
pub(crate) fn emit_lir_c(module: &l::Module, require_main: bool) -> Result<CProgram, String> {
    verify_module(module).map_err(|errors| {
        format!(
            "internal error: LIR verification failed before C transcription:\n{}",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    let program = Emitter::new(module)?.emit(require_main)?;
    verify_no_empty_aggregate(&program)?;
    verify_no_label_before_declaration(&program)?;
    Ok(program)
}

/// One emitted type whose member list is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EmptyAggregate {
    /// The generated text this site is in.
    text: &'static str,
    /// The 1-based line of the keyword in that text.
    line: usize,
    /// `struct`, `union`, or `enum`.
    keyword: &'static str,
    /// The tag name, or the empty string for an anonymous type.
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LabelBeforeDeclaration {
    text: &'static str,
    line: usize,
    label: String,
}

fn internal(message: impl AsRef<str>) -> String {
    format!("internal error: {}", message.as_ref())
}

struct Emitter<'m> {
    module: &'m l::Module,
    layouts: Layouts,
    positions: Vec<Pos>,
    runtime_symbols: BTreeMap<String, (String, Vec<String>)>,
    foreign_symbols: Vec<String>,
    field_owners: HashMap<l::FieldId, (ClassId, usize)>,
    helper_prototypes: String,
    helpers: String,
    helper_count: u32,
    long_string_data: String,
    long_string_symbols: HashMap<Vec<u8>, String>,
}

struct BoundaryPtrWriteback {
    class: ClassId,
    source: String,
    scratch: String,
}

fn local_contains_managed(layouts: &Layouts, ty: &l::ValueType) -> Result<bool, String> {
    match ty {
        l::ValueType::Data(ty) => type_contains_managed(layouts, ty),
        // Every iterator owns the shared four-word root storage plan.
        l::ValueType::Iterator(_) => Ok(true),
        l::ValueType::Address(_) => Ok(false),
    }
}

struct Body<'e, 'm, 'f> {
    emitter: &'e mut Emitter<'m>,
    function: &'f l::Function,
    coroutine: bool,
    suspend_states: Vec<Option<u32>>,
    rooted_values: HashSet<l::ValueId>,
    rooted_locals: HashSet<l::LocalId>,
    promoted_locals: HashMap<l::LocalId, l::ValueId>,
    address_definitions: HashMap<l::ValueId, &'f l::Instruction>,
    folded_addresses: HashSet<l::ValueId>,
    function_scoped_values: HashSet<l::ValueId>,
    block_value_declarations: Vec<Vec<l::ValueId>>,
    dominator_children: Vec<Vec<l::BlockId>>,
    graph_roots: Vec<l::BlockId>,
    removable_edge_copies: HashSet<(l::BlockId, l::BlockId, usize)>,
    value_storage: Vec<l::ValueId>,
    root_storage: RootStoragePlan,
    dead_forward_iterator_results: HashSet<l::ValueId>,
    fixed_iterators: HashSet<l::ValueId>,
    delayed_declarations: HashMap<String, l::ValueId>,
    consumed_traps: Vec<l::Trap>,
    temporary: u32,
    /// Whether `emit_storage` declared a shadow-root frame. `emit_pop`
    /// reads the same fact, so push and pop cannot disagree.
    shadow_frame: bool,
}

enum EdgeCopySource {
    Value(l::ValueId),
    Constant(l::Constant),
    Temporary(String),
}

struct EdgeCopy {
    destination: l::ValueId,
    source: EdgeCopySource,
}

#[derive(Clone, Copy)]
enum AddressUse {
    Chain(l::ValueId),
    Terminal,
    Escape,
}

/// Emission lookups derived once from the function's definitions and mentions.
struct EmissionIndex<'f> {
    definitions: HashMap<l::ValueId, &'f l::Instruction>,
    definition_blocks: Vec<Option<l::BlockId>>,
    use_blocks: Vec<HashSet<l::BlockId>>,
    reachable_use_rows: Vec<Option<usize>>,
    reachable_uses: Vec<bool>,
    parameter_blocks: Vec<Option<l::BlockId>>,
    incoming_targets: Vec<Vec<l::BlockTarget>>,
    local_seeds: Vec<LocalSeed>,
    local_addresses: Vec<(l::LocalId, Option<l::ValueId>)>,
    first_stores: HashMap<l::LocalId, l::ValueId>,
    iterator_seeds: Vec<(l::ValueId, l::IteratorBoundKind)>,
    propagated_values: Vec<(l::ValueId, l::ValueId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalSeed {
    Unused,
    Local(l::LocalId),
    Other,
}

struct DeclarationScopes {
    function_values: HashSet<l::ValueId>,
    block_values: Vec<Vec<l::ValueId>>,
    dominator_children: Vec<Vec<l::BlockId>>,
    graph_roots: Vec<l::BlockId>,
}

struct Coalescing {
    parents: Vec<usize>,
    groups: Vec<root_storage::InterferenceGroup>,
}

fn collect_aggregates(ty: &Type, result: &mut Vec<Type>) {
    match ty {
        Type::FixedArray(element, _) | Type::IterResult(element) => {
            collect_aggregates(element, result);
            if !result.contains(ty) {
                result.push(ty.clone());
            }
        }
        Type::Class(_) | Type::Func(_) | Type::Array(_) | Type::Nullable(_) => {}
        _ => {}
    }
}

fn runtime_header_declares(symbol: &str) -> bool {
    symbol.starts_with("subscript_rt_ctx_") || symbol.starts_with("subscript_rt_worker_")
}

fn array_symbol(name: &str, fixed: bool) -> Result<&'static str, String> {
    if fixed {
        return Ok(match name {
            "ForEach" => "subscript_rt_fixed_arr_for_each",
            "Map" => "subscript_rt_fixed_arr_map",
            "Filter" => "subscript_rt_fixed_arr_filter",
            "Reduce" => "subscript_rt_fixed_arr_reduce",
            "Some" => "subscript_rt_fixed_arr_some",
            "Every" => "subscript_rt_fixed_arr_every",
            "FindIndex" => "subscript_rt_fixed_arr_find_index",
            "ReduceRight" => "subscript_rt_fixed_arr_reduce_right",
            other => {
                return Err(internal(format!(
                    "Array.{other} is not a FixedArray method"
                )))
            }
        });
    }
    Ok(match name {
        "IndexOf" => "subscript_rt_arr_index_of",
        "LastIndexOf" => "subscript_rt_arr_last_index_of",
        "Includes" => "subscript_rt_arr_includes",
        "Join" => "subscript_rt_arr_join",
        "Slice" => "subscript_rt_arr_slice",
        "Fill" => "subscript_rt_arr_fill",
        "Reverse" => "subscript_rt_arr_reverse",
        "Concat" => "subscript_rt_arr_concat",
        "ForEach" => "subscript_rt_arr_for_each",
        "Map" => "subscript_rt_arr_map",
        "Filter" => "subscript_rt_arr_filter",
        "Reduce" => "subscript_rt_arr_reduce",
        "Some" => "subscript_rt_arr_some",
        "Every" => "subscript_rt_arr_every",
        "FindIndex" => "subscript_rt_arr_find_index",
        "Sort" => "subscript_rt_arr_sort",
        "ReduceRight" => "subscript_rt_arr_reduce_right",
        "Splice" => "subscript_rt_arr_splice",
        "Shift" => "subscript_rt_arr_shift",
        "Unshift" => "subscript_rt_arr_unshift",
        "CopyWithin" => "subscript_rt_arr_copy_within",
        other => return Err(internal(format!("unknown Array intrinsic {other}"))),
    })
}

fn emit_padding_zero(
    emitter: &Emitter<'_>,
    out: &mut String,
    pointer: &str,
    ty: &Type,
) -> Result<(), String> {
    for range in emitter.layouts.padding_ranges(ty)? {
        let _ = writeln!(
            out,
            "    memset((unsigned char*)({pointer}) + {}u, 0, {}u);",
            range.start,
            range.end - range.start
        );
    }
    Ok(())
}

fn binary_symbol(operator: l::BinaryOp) -> Result<&'static str, String> {
    Ok(match operator {
        l::BinaryOp::Add => "+",
        l::BinaryOp::Sub => "-",
        l::BinaryOp::Mul => "*",
        l::BinaryOp::Div => "/",
        l::BinaryOp::Rem => "%",
        l::BinaryOp::Eq => "==",
        l::BinaryOp::Ne => "!=",
        l::BinaryOp::Lt => "<",
        l::BinaryOp::Le => "<=",
        l::BinaryOp::Gt => ">",
        l::BinaryOp::Ge => ">=",
        l::BinaryOp::BitAnd => "&",
        l::BinaryOp::BitOr => "|",
        l::BinaryOp::BitXor => "^",
        l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr => {
            return Err(internal("shift needs a typed expression"))
        }
    })
}

fn integer_width(ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 => 32,
        Type::I64 | Type::U64 => 64,
        other => return Err(internal(format!("integer width for {other:?}"))),
    })
}

fn unsigned_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => "uint8_t",
        Type::I16 | Type::U16 => "uint16_t",
        Type::I32 | Type::U32 => "uint32_t",
        Type::I64 | Type::U64 => "uint64_t",
        other => return Err(internal(format!("unsigned carrier for {other:?}"))),
    })
}

fn integer_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 => "int8_t",
        Type::U8 => "uint8_t",
        Type::I16 => "int16_t",
        Type::U16 => "uint16_t",
        Type::I32 => "int32_t",
        Type::U32 => "uint32_t",
        Type::I64 => "int64_t",
        Type::U64 => "uint64_t",
        other => return Err(internal(format!("integer carrier for {other:?}"))),
    })
}

fn shift_expression(
    operator: l::BinaryOp,
    ty: &Type,
    left: &str,
    right: &str,
) -> Result<String, String> {
    let amount = format!("(({right}) & {}u)", integer_width(ty)? - 1);
    let carrier = match operator {
        l::BinaryOp::Shl | l::BinaryOp::UShr => unsigned_ctype(ty)?,
        l::BinaryOp::Shr => integer_ctype(ty)?,
        other => return Err(internal(format!("shift expression for {other:?}"))),
    };
    let symbol = if operator == l::BinaryOp::Shl {
        "<<"
    } else {
        ">>"
    };
    Ok(format!(
        "(({})((({carrier})({left})) {symbol} {amount}))",
        integer_ctype(ty)?
    ))
}

fn float_to_int_helper(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 => "subscript_f2i8",
        Type::U8 => "subscript_f2u8",
        Type::I16 => "subscript_f2i16",
        Type::U16 => "subscript_f2u16",
        Type::I32 => "subscript_f2i32",
        Type::U32 => "subscript_f2u32",
        Type::I64 => "subscript_f2i64",
        Type::U64 => "subscript_f2u64",
        other => return Err(internal(format!("float conversion target {other:?}"))),
    })
}

fn int_literal(value: i64, ty: &Type) -> String {
    match ty {
        Type::U8 => format!("((uint8_t){})", value as u8),
        Type::U16 => format!("((uint16_t){})", value as u16),
        Type::I8 => format!("((int8_t){value})"),
        Type::I16 => format!("((int16_t){value})"),
        Type::U32 => format!("{}u", value as u32),
        Type::U64 => format!("{}ull", value as u64),
        Type::I64 | Type::Date if value == i64::MIN => "(-9223372036854775807ll - 1)".into(),
        Type::I64 | Type::Date => format!("{value}ll"),
        _ if value == i64::from(i32::MIN) => "(-2147483647 - 1)".into(),
        _ => value.to_string(),
    }
}

fn float_literal(value: f64, ty: &Type) -> String {
    if *ty == Type::F32 {
        let value = value as f32;
        if value.is_nan() {
            return "((float)(0.0f/0.0f))".into();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "((float)(-1.0f/0.0f))"
            } else {
                "((float)(1.0f/0.0f))"
            }
            .into();
        }
        let mut result = format!("{value:?}");
        if !result.contains(['.', 'e', 'E']) {
            result.push_str(".0");
        }
        result.push('f');
        result
    } else {
        if value.is_nan() {
            return "(0.0/0.0)".into();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "(-1.0/0.0)"
            } else {
                "(1.0/0.0)"
            }
            .into();
        }
        let mut result = format!("{value:?}");
        if !result.contains(['.', 'e', 'E']) {
            result.push_str(".0");
        }
        result
    }
}

fn c_string_literal(bytes: &[u8]) -> String {
    let mut result = String::from("\"");
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 && index % 4000 == 0 {
            result.push_str("\"\n\"");
        }
        match byte {
            b'"' => result.push_str("\\\""),
            b'\\' => result.push_str("\\\\"),
            0x20..=0x7e => result.push(char::from(*byte)),
            other => {
                let _ = write!(result, "\\{other:03o}");
            }
        }
    }
    result.push('"');
    result
}

#[test]
fn c_string_literal_splits_at_source_byte_boundaries() {
    let mut octal = vec![b'a'; 3999];
    octal.extend_from_slice(&[0xff, 0x01]);
    let mut quote = vec![b'a'; 3999];
    quote.extend_from_slice(b"\"b");
    let cases = [
        ("a", vec![b'a'; 4000], 1),
        ("b", vec![b'a'; 4001], 2),
        ("c", octal, 2),
        ("d", vec![b'a'; 20000], 5),
        ("e", quote, 2),
    ];
    let mut failures = Vec::new();
    for (name, input, expected_count) in cases {
        let result = std::panic::catch_unwind(|| {
            let literal = c_string_literal(&input);
            let pieces: Vec<_> = literal.split('\n').collect();
            assert_eq!(pieces.len(), expected_count, "case {name}: piece count");
            match name {
                "a" | "b" | "d" => {
                    assert_eq!(pieces[0].len(), 4002);
                    assert!(pieces[0].starts_with('"') && pieces[0].ends_with('"'));
                    assert_eq!(pieces[0].bytes().filter(|byte| *byte == b'a').count(), 4000);
                    if name == "b" {
                        assert_eq!(pieces[1], "\"a\"");
                    }
                    if name == "d" {
                        assert!(pieces.iter().all(|piece| *piece == pieces[0]));
                    }
                }
                "c" => {
                    assert_eq!(pieces[0].len(), 4005);
                    assert!(pieces[0].starts_with('"'));
                    assert_eq!(pieces[0].bytes().filter(|byte| *byte == b'a').count(), 3999);
                    assert!(pieces[0].ends_with("\\377\""));
                    assert_eq!(pieces[1], "\"\\001\"");
                }
                "e" => {
                    assert_eq!(pieces[0].len(), 4003);
                    assert!(pieces[0].starts_with('"'));
                    assert_eq!(pieces[0].bytes().filter(|byte| *byte == b'a').count(), 3999);
                    assert!(pieces[0].ends_with("\\\"\""));
                    assert_eq!(pieces[1], "\"b\"");
                }
                _ => unreachable!(),
            }
        });
        if result.is_err() {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "failed cases: {failures:?}");
}

fn render_allocation_metadata_header() -> String {
    r#"/* DO NOT EDIT. Generated by subscript-codegen from the checked program. */
#ifndef SUBSCRIPT_ALLOCATION_METADATA_H
#define SUBSCRIPT_ALLOCATION_METADATA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t class_id;
    const char *name;
} subscript_alloc_class_info;

typedef struct {
    const char *file;
    uint32_t line;
    uint32_t column;
} subscript_alloc_position_info;

extern const subscript_alloc_class_info subscript_alloc_classes[];
extern const uint64_t subscript_alloc_class_count;
extern const subscript_alloc_position_info subscript_alloc_positions[];
extern const uint64_t subscript_alloc_position_count;

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUBSCRIPT_ALLOCATION_METADATA_H */
"#
    .into()
}

fn render_allocation_metadata_definitions(module: &l::Module, positions: &[Pos]) -> String {
    let mut out = String::from(
        "\n/* Allocation attribution tables. Generated from checked HIR and the\n\
* exact pos_id sequence above; consume through the generated\n\
* allocation metadata header. */\n\
typedef struct { uint32_t class_id; const char *name; } subscript_alloc_class_info;\n\
typedef struct { const char *file; uint32_t line; uint32_t column; } subscript_alloc_position_info;\n\n\
const subscript_alloc_class_info subscript_alloc_classes[] = {\n",
    );
    for (class_id, name) in [
        (rtc::CLASS_STRING, "string"),
        (rtc::CLASS_ARRAY, "Array"),
        (rtc::CLASS_ARRAY_DATA, "ArrayData"),
        (rtc::CLASS_GENERATOR, "GeneratorFrame"),
        (rtc::CLASS_MAP, "Map"),
        (rtc::CLASS_SET, "Set"),
        (rtc::CLASS_MAP_DATA, "MapData"),
        (rtc::CLASS_MAP_INDEX, "MapIndex"),
    ] {
        let _ = writeln!(
            out,
            "    {{ {class_id}u, {} }},",
            c_string_literal(name.as_bytes())
        );
    }
    for class in &module.classes {
        let _ = writeln!(
            out,
            "    {{ {}u, {} }},",
            class.id.0,
            c_string_literal(class.source_name.as_bytes())
        );
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_class_count = {}u;\n",
        8 + module.classes.len()
    );
    out.push_str("const subscript_alloc_position_info subscript_alloc_positions[] = {\n");
    if positions.is_empty() {
        out.push_str("    { \"\", 0u, 0u },\n");
    } else {
        for pos in positions {
            let _ = writeln!(
                out,
                "    {{ {}, {}u, {}u }},",
                c_string_literal(pos.file.as_bytes()),
                pos.line,
                pos.col
            );
        }
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_position_count = {}u;",
        positions.len()
    );
    out
}

const CALLBACK_VIEW: &str = r#"
typedef struct subscript_callback_string_view { const uint8_t* data; size_t len; } subscript_callback_string_view;
extern void subscript_rt_cb_trampoline(subscript_callback_string_view message, void* userdata1, void* userdata2);
"#;

const PREAMBLE: &str = concat!(
    include_str!("../../runtime/include/subscript_runtime.h"),
    r#"

/* Generated by subscript's LIR-to-C ship transcriber. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>

extern double subscript_rt_fmod(void* ctx, double left, double right);

typedef uint8_t (*SubAsyncResume)(void*, void*, void*);
typedef struct { const unsigned char* data; uint64_t len; } SubStringAliasMember;
typedef struct { uint64_t len; uint64_t cap; uint64_t elem_size; unsigned char* data; } SsArrayHeader;
typedef struct { int32_t state; uint32_t reserved; SubAsyncResume resume; } SubCoroutinePrefix;

static int8_t subscript_f2i8(double v) { if (v != v) return 0; if (v <= -128.0) return -128; if (v >= 127.0) return 127; return (int8_t)v; }
static uint8_t subscript_f2u8(double v) { if (v != v || v <= 0.0) return 0; if (v >= 255.0) return 255; return (uint8_t)v; }
static int16_t subscript_f2i16(double v) { if (v != v) return 0; if (v <= -32768.0) return -32768; if (v >= 32767.0) return 32767; return (int16_t)v; }
static uint16_t subscript_f2u16(double v) { if (v != v || v <= 0.0) return 0; if (v >= 65535.0) return 65535; return (uint16_t)v; }
static int32_t subscript_f2i32(double v) { if (v != v) return 0; if (v <= -2147483648.0) return (-2147483647 - 1); if (v >= 2147483647.0) return 2147483647; return (int32_t)v; }
static uint32_t subscript_f2u32(double v) { if (v != v || v <= 0.0) return 0; if (v >= 4294967295.0) return 4294967295u; return (uint32_t)v; }
static int64_t subscript_f2i64(double v) { if (v != v) return 0; if (v <= -9223372036854775808.0) return (-9223372036854775807ll - 1); if (v >= 9223372036854775807.0) return 9223372036854775807ll; return (int64_t)v; }
static uint64_t subscript_f2u64(double v) { if (v != v || v <= 0.0) return 0; if (v >= 18446744073709551615.0) return 18446744073709551615ull; return (uint64_t)v; }
"#
);

#[cfg(test)]
mod tests {
    use super::*;

    fn lower_test_source(name: &str, source: &str) -> l::Module {
        let hir =
            subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(name, source)])
                .expect("test source checks");
        crate::lir::lower_module(&hir).expect("test source lowers")
    }

    fn emission_index_fixture() -> l::Function {
        let pos = Pos::new("emission-index.ts", 1, 1);
        let operand = |value| l::Operand::Value(l::ValueId(value));
        let instruction = |result: Option<u32>, kind, operands| l::Instruction {
            result: result.map(l::ValueId),
            kind,
            operands,
            invalidates: Vec::new(),
            traps: Vec::new(),
            pos: pos.clone(),
        };
        let constant = || {
            l::Operand::Constant(l::Constant {
                ty: Type::I32,
                kind: l::ConstantKind::Integer(7),
            })
        };
        l::Function {
            id: l::FunctionId(0),
            source_name: "index".into(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::I32,
            locals: (0..3)
                .map(|id| l::Local {
                    id: l::LocalId(id),
                    source_name: format!("local{id}"),
                    ty: l::ValueType::Data(Type::I32),
                    mutable: true,
                    storage: l::LocalStorageClass::Activation,
                    pos: pos.clone(),
                })
                .collect(),
            values: (0..6)
                .map(|id| l::Value {
                    id: l::ValueId(id),
                    ty: l::ValueType::Data(Type::I32),
                    fresh_owner: false,
                    source_name: None,
                })
                .collect(),
            liveness: l::Liveness {
                live_ins: vec![Vec::new(), Vec::new()],
                value_origins: (0..6).map(l::ValueId).collect(),
            },
            blocks: vec![
                l::BasicBlock {
                    id: l::BlockId(0),
                    source_name: None,
                    parameters: Vec::new(),
                    instructions: vec![
                        instruction(Some(0), l::InstructionKind::Copy, vec![constant()]),
                        instruction(
                            None,
                            l::InstructionKind::StoreLocal(l::LocalId(0)),
                            vec![operand(0)],
                        ),
                        instruction(
                            None,
                            l::InstructionKind::StoreLocal(l::LocalId(1)),
                            vec![operand(0)],
                        ),
                        instruction(Some(1), l::InstructionKind::Copy, vec![constant()]),
                        instruction(Some(4), l::InstructionKind::Copy, vec![constant()]),
                        instruction(
                            None,
                            l::InstructionKind::StoreLocal(l::LocalId(2)),
                            vec![operand(4)],
                        ),
                        instruction(
                            None,
                            l::InstructionKind::StoreLocal(l::LocalId(2)),
                            vec![operand(4)],
                        ),
                        instruction(Some(5), l::InstructionKind::Copy, vec![constant()]),
                        instruction(
                            None,
                            l::InstructionKind::StoreLocal(l::LocalId(0)),
                            vec![operand(5)],
                        ),
                    ],
                    terminator: l::Terminator::Branch(l::BlockTarget {
                        block: l::BlockId(1),
                        arguments: vec![operand(1), operand(1)],
                    }),
                },
                l::BasicBlock {
                    id: l::BlockId(1),
                    source_name: None,
                    parameters: vec![l::ValueId(2), l::ValueId(3)],
                    instructions: Vec::new(),
                    terminator: l::Terminator::Return {
                        value: Some(constant()),
                        pos: pos.clone(),
                    },
                },
            ],
            entry: l::BlockId(0),
            pos,
        }
    }

    #[test]
    fn emission_index_rows_match_definitions_and_mentions() {
        let mut function = emission_index_fixture();
        let index = EmissionIndex::build(&function).unwrap();
        let b0 = l::BlockId(0);
        let b1 = l::BlockId(1);
        assert_eq!(
            index.use_blocks,
            vec![
                HashSet::from([b0]),
                HashSet::from([b0]),
                HashSet::new(),
                HashSet::new(),
                HashSet::from([b0]),
                HashSet::from([b0]),
            ]
        );
        assert_eq!(
            index.local_seeds,
            vec![
                LocalSeed::Other,
                LocalSeed::Other,
                LocalSeed::Unused,
                LocalSeed::Unused,
                LocalSeed::Local(l::LocalId(2)),
                LocalSeed::Local(l::LocalId(0)),
            ]
        );
        assert_eq!(
            index.first_stores,
            HashMap::from([
                (l::LocalId(0), l::ValueId(0)),
                (l::LocalId(1), l::ValueId(0)),
                (l::LocalId(2), l::ValueId(4)),
            ])
        );
        assert_eq!(
            index.parameter_blocks,
            vec![None, None, Some(b1), Some(b1), None, None]
        );
        assert_eq!(
            index.definition_blocks,
            vec![Some(b0), Some(b0), None, None, Some(b0), Some(b0)]
        );
        assert_eq!(
            index.reachable_use_rows,
            vec![None, Some(0), None, None, None, None]
        );
        assert_eq!(index.reachable_uses, vec![true, false]);
        assert_eq!(
            removable_block_parameter_copies(&function, &index),
            (HashSet::from([(b0, b1, 1)]), HashSet::from([l::ValueId(3)]),)
        );
        // A terminator-only use must also populate the mention and seed tables.
        function.blocks[1].terminator = l::Terminator::Return {
            value: Some(l::Operand::Value(l::ValueId(2))),
            pos: function.pos.clone(),
        };
        let index = EmissionIndex::build(&function).unwrap();
        assert_eq!(index.use_blocks[2], HashSet::from([b1]));
        assert_eq!(index.local_seeds[2], LocalSeed::Other);
    }

    #[test]
    fn emission_index_rejects_out_of_range_ids() {
        let mut function = emission_index_fixture();
        function.blocks[0].instructions[0].result = Some(l::ValueId(6));
        let error = EmissionIndex::build(&function)
            .err()
            .expect("invalid result must return an error");
        assert!(error.contains("invalid emission value id 6"), "{error}");
        let mut function = emission_index_fixture();
        function.blocks[0].terminator = l::Terminator::Branch(l::BlockTarget {
            block: l::BlockId(2),
            arguments: Vec::new(),
        });
        let error = EmissionIndex::build(&function)
            .err()
            .expect("invalid target must return an error");
        assert!(error.contains("invalid emission block id 2"), "{error}");
    }

    #[test]
    fn emission_index_rejects_out_of_range_local_ids() {
        for kind in [
            l::InstructionKind::StoreLocal(l::LocalId(3)),
            l::InstructionKind::LoadLocal(l::LocalId(3)),
            l::InstructionKind::AddressOfLocal(l::LocalId(3)),
        ] {
            let mut function = emission_index_fixture();
            function.blocks[0].instructions[1].kind = kind;
            let error = EmissionIndex::build(&function)
                .err()
                .expect("invalid local must return an error");
            assert!(error.contains("invalid emission local id 3"), "{error}");
        }
        let mut function = emission_index_fixture();
        function.parameters.push(l::Parameter {
            storage: Some(l::LocalId(3)),
            value: l::ValueId(0),
            source_name: "parameter".into(),
            kind: l::ParameterKind::Explicit,
            pos: function.pos.clone(),
        });
        let error = EmissionIndex::build(&function)
            .err()
            .expect("invalid parameter storage must return an error");
        assert!(error.contains("invalid emission local id 3"), "{error}");
    }

    #[test]
    fn iterator_bounds_cross_back_edges_and_combine_fixed_and_live_paths() {
        let mut module = lower_test_source("iterator-index.ts", "export function main(): void {}");
        let function = module
            .functions
            .iter_mut()
            .find(|function| function.source_name == "main")
            .unwrap();
        let pos = function.pos.clone();
        function.values = (0..7)
            .map(|id| l::Value {
                id: l::ValueId(id),
                ty: l::ValueType::Iterator(l::IteratorType {
                    kind: l::ForOfKind::ArrayValues,
                    element: Type::I32,
                }),
                fresh_owner: false,
                source_name: None,
            })
            .collect();
        let instruction = |result, kind, operands| l::Instruction {
            result: Some(l::ValueId(result)),
            kind,
            operands,
            invalidates: Vec::new(),
            traps: Vec::new(),
            pos: pos.clone(),
        };
        let argument = |value| l::Operand::Value(l::ValueId(value));
        let target = |block, values: &[u32]| l::BlockTarget {
            block: l::BlockId(block),
            arguments: values.iter().copied().map(argument).collect(),
        };
        function.blocks = vec![
            l::BasicBlock {
                id: l::BlockId(0),
                source_name: None,
                parameters: Vec::new(),
                instructions: vec![
                    instruction(
                        0,
                        l::InstructionKind::IteratorCreate {
                            kind: l::ForOfKind::ArrayValues,
                            bound: l::IteratorBoundKind::Fixed,
                        },
                        Vec::new(),
                    ),
                    instruction(
                        1,
                        l::InstructionKind::IteratorCreate {
                            kind: l::ForOfKind::ArrayValues,
                            bound: l::IteratorBoundKind::Live,
                        },
                        Vec::new(),
                    ),
                ],
                terminator: l::Terminator::ConditionalBranch {
                    condition: l::Operand::Constant(l::Constant {
                        ty: Type::Bool,
                        kind: l::ConstantKind::Boolean(true),
                    }),
                    then_target: target(1, &[0, 0]),
                    else_target: target(1, &[1, 0]),
                },
            },
            l::BasicBlock {
                id: l::BlockId(1),
                source_name: None,
                parameters: vec![l::ValueId(2), l::ValueId(3)],
                instructions: vec![
                    instruction(4, l::InstructionKind::IteratorAdvance, vec![argument(2)]),
                    instruction(5, l::InstructionKind::Copy, vec![argument(3)]),
                    instruction(6, l::InstructionKind::IteratorAdvance, vec![argument(5)]),
                ],
                terminator: l::Terminator::Branch(target(1, &[4, 6])),
            },
        ];
        let index = EmissionIndex::build(function).unwrap();
        assert_eq!(
            fixed_iterator_values(function, &index),
            HashSet::from([l::ValueId(0), l::ValueId(3), l::ValueId(5), l::ValueId(6)])
        );
        // The live bit reaches the second parameter only through the back edge.
        function.blocks[1].terminator = l::Terminator::Branch(target(1, &[6, 4]));
        let index = EmissionIndex::build(function).unwrap();
        assert_eq!(
            fixed_iterator_values(function, &index),
            HashSet::from([l::ValueId(0)])
        );
    }

    #[test]
    fn duplicate_lir_site_fails_with_function_and_site() {
        let pos = Pos::new("trap-probe.ts", 4, 9);
        let trap = l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        };
        let function = l::Function {
            id: l::FunctionId(7),
            source_name: "probe".into(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::Void,
            locals: Vec::new(),
            values: Vec::new(),
            liveness: l::Liveness::default(),
            blocks: Vec::new(),
            entry: l::BlockId(0),
            pos,
        };
        let error = verify_trap_consumption(&function, &[trap.clone(), trap.clone()], &[trap])
            .expect_err("one consumed site cannot satisfy two LIR sites");
        assert!(error.contains("function 7 `probe`"), "{error}");
        assert!(error.contains("trap-probe.ts:4:9"), "{error}");
        assert!(
            error.contains("LIR carries 2 site(s), transcriber consumed 1"),
            "{error}"
        );
    }

    #[test]
    fn managed_word_counts_cover_all_handles_and_iterators() {
        let mut module = lower_test_source(
            "rooted-locals.ts",
            "@CStruct class Boundary { x: i32 = 0; }\nclass Ref {}\nexport function main(): void {}\n",
        );
        module.classes[0].is_boundary = true;
        let boundary = Type::Class(module.classes[0].id);
        let reference = Type::Class(module.classes[1].id);
        let function = || {
            Type::Func(Box::new(subscript_compiler::FuncType {
                params: vec![Type::I32],
                ret: Type::Bool,
            }))
        };
        let mut cases = vec![
            (l::ValueType::Data(Type::Str), 1),
            (l::ValueType::Data(Type::RegExp), 1),
            (l::ValueType::Data(Type::Object), 1),
            (l::ValueType::Data(Type::Array(Box::new(Type::I32))), 1),
            (
                l::ValueType::Data(Type::Map(Box::new(Type::I32), Box::new(Type::Bool))),
                1,
            ),
            (l::ValueType::Data(Type::Set(Box::new(Type::I32))), 1),
            (l::ValueType::Data(Type::Generator(Box::new(Type::I32))), 1),
            (
                l::ValueType::Data(Type::AsyncHandle(Box::new(Type::I32))),
                1,
            ),
            (
                l::ValueType::Data(Type::Worker(Box::new(Type::I32), Box::new(Type::Bool))),
                0,
            ),
            (l::ValueType::Data(Type::Inbox(Box::new(Type::I32))), 0),
            (l::ValueType::Data(Type::Outbox(Box::new(Type::I32))), 0),
            (l::ValueType::Data(function()), 0),
            (l::ValueType::Data(reference), 1),
            (l::ValueType::Data(Type::Nullable(Box::new(function()))), 1),
            (l::ValueType::Data(Type::Nullable(Box::new(boundary))), 1),
        ];
        let iterator_kinds = [
            l::ForOfKind::ArrayValues,
            l::ForOfKind::ArrayKeys,
            l::ForOfKind::ArrayValuesReverse,
            l::ForOfKind::ArrayKeysReverse,
            l::ForOfKind::FixedArrayValues,
            l::ForOfKind::MapKeys,
            l::ForOfKind::MapValues,
            l::ForOfKind::SetValues,
            l::ForOfKind::StringCodePoints,
        ];
        cases.extend(iterator_kinds.map(|kind| {
            (
                l::ValueType::Iterator(l::IteratorType {
                    kind,
                    element: Type::I32,
                }),
                4,
            )
        }));
        let layouts = Layouts::build_lir(&module).expect("test classes have valid layouts");
        for (ty, expected) in cases {
            assert_eq!(
                root_storage::managed_value_words(&layouts, &ty).expect("managed word count"),
                expected,
                "{ty:?}"
            );
        }
    }

    #[test]
    fn c_json_guard_reads_the_lir_field_id() {
        let mut module = lower_test_source(
            "json-field-id.ts",
            "export function main(): void {\n  const result: JsonResult<i32> = JSON.parse<i32>(\"1\");\n  print(`${result.value}`);\n}\n",
        );
        let ok_field = module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .flat_map(|instruction| &instruction.traps)
            .find_map(|trap| match trap.kind {
                l::TrapKind::JsonResultValue(field) => Some(field),
                _ => None,
            })
            .expect("JSON value load names its ok field");
        module
            .classes
            .iter_mut()
            .flat_map(|class| &mut class.fields)
            .find(|field| field.id == ok_field)
            .expect("JSON ok field exists")
            .source_name = "not_ok".to_string();

        let program = emit_lir_c(&module, true).expect("C locates the guard field by LIR id");
        assert!(
            program.source.contains(&format!(")->d{}", ok_field.0)),
            "the emitted guard does not read field {}",
            ok_field.0
        );
    }

    #[test]
    fn c_async_runner_reads_the_lir_entry_id() {
        let mut module = lower_test_source(
            "entry-id.ts",
            "export function main(): void {}\nexport async function auxiliary(): Promise<void> {}\n",
        );
        let entry = module.entry.expect("module entry");
        module.functions[entry.0 as usize].source_name = "renamed_entry".to_string();
        let root = *module.async_roots.first().expect("async root");
        module.functions[root.0 as usize].source_name = "main".to_string();

        let program = CProgram::from_lir(&module, true).expect("C emits renamed LIR functions");
        let runner = program
            .source
            .split_once("void subscript_kick_async_exports(subscript_rt_context* ctx) {")
            .expect("async runner definition")
            .1;
        assert!(
            runner.starts_with("\n    subscript_export_main(ctx);"),
            "the non-entry root was selected by source spelling:\n{runner}"
        );
        assert!(!runner.contains("subscript_export_renamed_entry(ctx);"));
    }
}
