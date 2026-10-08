use super::*;

fn field(base: &str, pointer: bool, is_const: bool, name: &str) -> CField {
    CField {
        base: base.into(),
        is_const,
        nullable: false,
        pointer,
        array_len: None,
        name: name.into(),
    }
}

fn parsed_with(decls: Vec<Decl>) -> Parsed {
    let mut p = Parsed::default();
    p.decls = decls;
    p
}

#[test]
fn raw_builtins_map_to_sized_numerics() {
    // A struct whose fields are raw C builtins (no stdint typedef).
    let decls = vec![Decl::Struct {
        name: "SubRaw".into(),
        fields: vec![
            field("int", false, false, "a"),
            field("unsigned int", false, false, "b"),
            field("long long", false, false, "c"),
            field("unsigned long long", false, false, "d"),
            field("float", false, false, "e"),
            field("double", false, false, "f"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("raw builtins map cleanly");
    assert!(m.contains("a: i32;"), "{m}");
    assert!(m.contains("b: u32;"), "{m}");
    assert!(m.contains("c: i64;"), "{m}");
    assert!(m.contains("d: u64;"), "{m}");
    assert!(m.contains("e: f32;"), "{m}");
    assert!(m.contains("f: f64;"), "{m}");
}

#[test]
fn bare_char_scalar_field_is_a_clean_err() {
    // Without the libclang target's signedness marker, plain `char`
    // remains ambiguous: fail loud, never guess.
    let decls = vec![Decl::Struct {
        name: "SubHasChar".into(),
        fields: vec![field("char", false, false, "c")],
    }];
    let err = emit(&parsed_with(decls)).expect_err("bare char must fail loud");
    assert!(err.0.contains("char"), "message names the type: {}", err.0);
}

#[test]
fn narrow_scalar_spellings_map_without_guessing() {
    let decls = vec![Decl::Struct {
        name: "SubNarrow".into(),
        fields: vec![
            field("int8_t", false, false, "a"),
            field("unsigned char", false, false, "b"),
            field("short", false, false, "c"),
            field("uint16_t", false, false, "d"),
            field("_Float16", false, false, "e"),
            field("signed char", false, false, "f"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("narrow scalars map");
    for expected in [
        "a: i8;", "b: u8;", "c: i16;", "d: u16;", "e: f16;", "f: i8;",
    ] {
        assert!(m.contains(expected), "{m}");
    }
}

#[test]
fn fp16_without_a_known_format_fails_loud() {
    let decls = vec![Decl::Struct {
        name: "SubHalf".into(),
        fields: vec![field("__fp16", false, false, "value")],
    }];
    let err = emit(&parsed_with(decls)).expect_err("__fp16 must not guess a format");
    assert_eq!(
        err.0,
        "`__fp16` has a target-dependent half format; use `_Float16` for unambiguous IEEE binary16"
    );
}

#[test]
fn double_pointer_field_is_a_clean_err() {
    // A double pointer surfaces as a pointer to an unnamed pointer type
    // (`SubThing *`), which is not a registered named type → fail loud.
    let decls = vec![Decl::Struct {
        name: "SubHasPP".into(),
        fields: vec![field("SubThing *", true, false, "pp")],
    }];
    let err = emit(&parsed_with(decls)).expect_err("double pointer must fail loud");
    assert!(
        err.0.contains("SubThing"),
        "message names the type: {}",
        err.0
    );
}

#[test]
fn anonymous_inline_struct_field_is_a_clean_err() {
    // An anonymous inline struct field carries an unnamed record
    // spelling that is not in the registry → fail loud.
    let decls = vec![Decl::Struct {
        name: "SubHasAnon".into(),
        fields: vec![field(
            "SubOuter::(unnamed at header.h:3:5)",
            false,
            false,
            "inner",
        )],
    }];
    let err = emit(&parsed_with(decls)).expect_err("anonymous struct must fail loud");
    assert!(
        err.0.contains("unnamed"),
        "message names the type: {}",
        err.0
    );
}

#[test]
fn embedded_count_pointer_pair_collapses_to_array() {
    // `uint32_t layer; size_t drawsCount; const uint32_t* draws;` →
    // the count field is elided and the pointer becomes `u32[]`.
    let decls = vec![Decl::Struct {
        name: "SubDrawList".into(),
        fields: vec![
            field("uint32_t", false, false, "layer"),
            field("size_t", false, false, "drawsCount"),
            field("uint32_t", true, true, "draws"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("embedded pair maps");
    assert!(m.contains("layer: u32;"), "{m}");
    assert!(m.contains("draws: u32[];"), "{m}");
    assert!(!m.contains("drawsCount"), "count field is elided: {m}");
    assert!(
        m.contains("constructor(layer: u32, draws: u32[]);"),
        "constructor drops the count: {m}"
    );
}

#[test]
fn mutable_struct_scalar_pair_collapses_to_array() {
    let decls = vec![Decl::Struct {
        name: "SubFillList".into(),
        fields: vec![
            field("size_t", false, false, "valuesCount"),
            field("uint16_t", true, false, "values"),
            field("uint32_t", false, false, "tag"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("mutable embedded scalar pair maps");
    assert!(m.contains("values: u16[];"), "{m}");
    assert!(!m.contains("valuesCount"), "count field is elided: {m}");
}

#[test]
fn registered_enum_struct_pair_collapses_to_array() {
    let decls = vec![
        Decl::Enum {
            name: "SubFormat".into(),
            members: vec![("SUB_FORMAT_A".into(), 1), ("SUB_FORMAT_B".into(), 2)],
        },
        Decl::Struct {
            name: "SubTexture".into(),
            fields: vec![
                field("size_t", false, false, "viewFormatsCount"),
                field("SubFormat", true, true, "viewFormats"),
                field("uint32_t", false, false, "usage"),
            ],
        },
    ];
    let m = emit(&parsed_with(decls)).expect("registered enum pair maps");
    assert!(m.contains("viewFormats: SubFormat[];"), "{m}");
    assert!(
        !m.contains("viewFormatsCount"),
        "count field is elided: {m}"
    );
    assert!(
        m.contains("constructor(viewFormats: SubFormat[], usage: u32);"),
        "{m}"
    );
}

#[test]
fn const_scalar_parameter_pair_collapses_to_array() {
    let decls = vec![Decl::Func {
        name: "subReadBytes".into(),
        ret: field("uint32_t", false, false, ""),
        params: vec![
            field("size_t", false, false, "dataCount"),
            field("uint8_t", true, true, "data"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("const scalar parameter pair maps");
    assert!(
        m.contains("declare function subReadBytes(data: u8[]): u32;"),
        "{m}"
    );
    assert!(m.contains(
            "// @subscript-c-scalar-pair function=\"subReadBytes\" parameter=\"data\" element=\"uint8_t\" const=true"
        ), "{m}");
    assert!(!m.contains("dataCount: u64"), "count is elided: {m}");
}

#[test]
fn mutable_scalar_parameter_pair_collapses_to_array() {
    let decls = vec![Decl::Func {
        name: "subFillBytes".into(),
        ret: field("void", false, false, ""),
        params: vec![
            field("size_t", false, false, "dataCount"),
            field("uint8_t", true, false, "data"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("mutable scalar parameter pair maps");
    assert!(
        m.contains("declare function subFillBytes(data: u8[]): void;"),
        "{m}"
    );
    assert!(m.contains(
            "// @subscript-c-scalar-pair function=\"subFillBytes\" parameter=\"data\" element=\"uint8_t\" const=false"
        ), "{m}");
}

#[test]
fn u16_scalar_parameter_pair_collapses_to_array() {
    let decls = vec![Decl::Func {
        name: "subFillShorts".into(),
        ret: field("void", false, false, ""),
        params: vec![
            field("size_t", false, false, "valuesCount"),
            field("uint16_t", true, false, "values"),
        ],
    }];
    let m = emit(&parsed_with(decls)).expect("u16 scalar parameter pair maps");
    assert!(
        m.contains("declare function subFillShorts(values: u16[]): void;"),
        "{m}"
    );
}

#[test]
fn lone_scalar_pointer_parameter_fails_loud() {
    let decls = vec![Decl::Func {
        name: "subReadBytes".into(),
        ret: field("void", false, false, ""),
        params: vec![field("uint8_t", true, true, "data")],
    }];
    let err = emit(&parsed_with(decls)).expect_err("lone scalar pointer must fail loud");
    assert!(err.0.contains("uint8_t"), "{}", err.0);
}

#[test]
fn non_adjacent_scalar_parameter_pair_fails_loud() {
    let decls = vec![Decl::Func {
        name: "subReadBytes".into(),
        ret: field("void", false, false, ""),
        params: vec![
            field("size_t", false, false, "dataCount"),
            field("uint32_t", false, false, "tag"),
            field("uint8_t", true, true, "data"),
        ],
    }];
    let err = emit(&parsed_with(decls)).expect_err("non-adjacent scalar pair must fail loud");
    assert!(
        err.0
            .contains("not the supported adjacent count-first shape"),
        "{}",
        err.0
    );
    assert!(err.0.contains("bare count"), "{}", err.0);
}

#[test]
fn every_stdint_lang_scalar_maps_at_scalar_parameter_pair_site() {
    let stdint = [
        ("int8_t", "i8"),
        ("uint8_t", "u8"),
        ("int16_t", "i16"),
        ("uint16_t", "u16"),
        ("int32_t", "i32"),
        ("uint32_t", "u32"),
        ("int64_t", "i64"),
        ("uint64_t", "u64"),
    ];
    let decls = stdint
        .iter()
        .enumerate()
        .map(|(index, (c_type, _))| Decl::Func {
            name: format!("subScalarPair{index}"),
            ret: field("void", false, false, ""),
            params: vec![
                field("size_t", false, false, "itemsCount"),
                field(c_type, true, index % 2 == 0, "items"),
            ],
        })
        .collect();
    let m = emit(&parsed_with(decls)).expect("every stdint scalar maps at a pair site");
    for (index, (_, lang_type)) in stdint.iter().enumerate() {
        assert!(
            m.contains(&format!(
                "declare function subScalarPair{index}(items: {lang_type}[]): void;"
            )),
            "missing {lang_type} pair mapping in {m}"
        );
    }
}

#[test]
fn pointer_first_embedded_shape_fails_loud() {
    // Pointer-before-count inside a larger struct is NOT the shape both
    // lowerings reconstruct (count immediately before pointer), so the
    // pointer stays a lone scalar pointer → fail loud. (A bare two-field
    // `{const T*; size_t}` is instead the standalone descriptor absorbed
    // to `T[]`, a26/a31 — a different, correct path; hence the third
    // field here forces the embedded interpretation.)
    let decls = vec![Decl::Struct {
        name: "SubPtrFirst".into(),
        fields: vec![
            field("uint32_t", false, false, "layer"),
            field("uint32_t", true, true, "draws"),
            field("size_t", false, false, "drawsCount"),
        ],
    }];
    let err = emit(&parsed_with(decls)).expect_err("pointer-first must fail loud");
    assert!(err.0.contains("SubPtrFirst"), "{}", err.0);
    assert!(err.0.contains("drawsCount"), "{}", err.0);
    assert!(err.0.contains("adjacent count-first"), "{}", err.0);
}

#[test]
fn non_adjacent_count_fails_loud() {
    // A count separated from the pointer by another field is not an
    // embedded pair; the pointer then fails loud as a lone scalar ptr.
    let decls = vec![Decl::Struct {
        name: "SubGap".into(),
        fields: vec![
            field("size_t", false, false, "drawsCount"),
            field("uint32_t", false, false, "layer"),
            field("uint32_t", true, true, "draws"),
        ],
    }];
    let err = emit(&parsed_with(decls)).expect_err("non-adjacent count must fail loud");
    assert!(err.0.contains("SubGap"), "{}", err.0);
    assert!(err.0.contains("bare count"), "{}", err.0);
}

#[test]
fn adjacent_struct_pair_with_mismatched_names_fails_loud() {
    let decls = vec![
        Decl::Enum {
            name: "SubFormat".into(),
            members: vec![("SUB_FORMAT_A".into(), 1)],
        },
        Decl::Struct {
            name: "SubBadTexture".into(),
            fields: vec![
                field("size_t", false, false, "viewFormatsCount"),
                field("SubFormat", true, true, "formats"),
            ],
        },
    ];
    let err = emit(&parsed_with(decls)).expect_err("mismatched adjacency must fail loud");
    assert!(err.0.contains("SubBadTexture"), "{}", err.0);
    assert!(err.0.contains("names do not collapse"), "{}", err.0);
    assert!(err.0.contains("viewFormats"), "{}", err.0);
}

#[test]
fn lone_scalar_pointer_field_fails_loud() {
    // A `const uint32_t*` field with no paired count is not an array
    // descriptor and has no boundary type → fail loud, not `u32 | null`.
    let decls = vec![Decl::Struct {
        name: "SubLonePtr".into(),
        fields: vec![field("uint32_t", true, true, "items")],
    }];
    let err = emit(&parsed_with(decls)).expect_err("lone scalar pointer must fail loud");
    assert!(err.0.contains("uint32_t"), "{}", err.0);
}

#[test]
fn bare_long_is_unmapped_and_fails_loud() {
    // `long`/`unsigned long` are target-width-dependent (LP64 vs LLP64)
    // and dropped from the builtin map.
    for spelling in ["long", "unsigned long"] {
        let decls = vec![Decl::Struct {
            name: "SubLong".into(),
            fields: vec![field(spelling, false, false, "n")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("bare long must fail loud");
        assert!(err.0.contains("long"), "{spelling}: {}", err.0);
    }
}

#[test]
fn flag_value_above_f64_exact_range_is_emitted_exactly() {
    let mut p = Parsed::default();
    p.aliases = vec![Alias {
        name: "SubBig".into(),
        underlying: "uint64_t".into(),
        chain: vec!["uint64_t".into()],
    }];
    p.constants = vec![crate::clangfe::Constant {
        name: "SUB_BIG_ONE".into(),
        type_base: "SubBig".into(),
        value: (1i64 << 53) + 1,
    }];
    let mirror = emit(&p).expect("flag value above 2^53 emits");
    assert!(
        mirror.contains("declare const SUB_BIG_ONE = 9007199254740993;"),
        "{mirror}"
    );
}

#[test]
fn flag_typedef_emits_alias_and_folded_members() {
    let mut p = Parsed::default();
    p.aliases = vec![Alias {
        name: "SubAccess".into(),
        underlying: "uint64_t".into(),
        chain: vec!["uint64_t".into()],
    }];
    p.constants = vec![
        Constant("SUB_ACCESS_READ", "SubAccess", 1),
        Constant("SUB_ACCESS_WRITE", "SubAccess", 2),
    ]
    .into_iter()
    .map(|c| c.into())
    .collect();
    let m = emit(&p).expect("flags emit");
    assert!(m.contains("type SubAccess = u64;"), "{m}");
    assert!(m.contains("declare const SUB_ACCESS_READ = 1;"), "{m}");
    assert!(m.contains("declare const SUB_ACCESS_WRITE = 2;"), "{m}");
}

#[test]
fn two_level_flag_alias_resolves_through_the_chain() {
    // `typedef uint32_t B; typedef B X;` — the immediate underlying is
    // the intermediate typedef `B` (not a mapped integer); the emitter
    // follows the chain to `uint32_t` → `u32` (§14.1).
    let mut p = Parsed::default();
    p.aliases = vec![Alias {
        name: "SubStageFlags".into(),
        underlying: "SubStageBits".into(),
        chain: vec![
            "SubStageBits".into(),
            "uint32_t".into(),
            "unsigned int".into(),
        ],
    }];
    p.constants = vec![Constant("SUB_STAGE_VERTEX", "SubStageFlags", 1)]
        .into_iter()
        .map(|c| c.into())
        .collect();
    let m = emit(&p).expect("two-level alias resolves");
    assert!(m.contains("type SubStageFlags = u32;"), "{m}");
    assert!(m.contains("declare const SUB_STAGE_VERTEX = 1;"), "{m}");
}

#[test]
fn alias_chain_not_reaching_an_integer_fails_loud_at_use() {
    // A typedef chain that never bottoms out in a mapped integer is not
    // registered, so a boundary use site of it fails loud (§14.1) —
    // never a silently wrong mirror.
    let mut p = Parsed::default();
    p.aliases = vec![Alias {
        name: "SubOpaqueId".into(),
        underlying: "SubBase".into(),
        chain: vec!["SubBase".into(), "SubThing".into()],
    }];
    p.decls = vec![Decl::Func {
        name: "subUse".into(),
        ret: field("void", false, false, ""),
        params: vec![field("SubOpaqueId", false, false, "id")],
    }];
    let err = emit(&p).expect_err("unresolvable alias chain must fail loud");
    assert!(err.0.contains("SubOpaqueId"), "{}", err.0);
}

struct Constant(&'static str, &'static str, i64);
impl From<Constant> for crate::clangfe::Constant {
    fn from(c: Constant) -> Self {
        crate::clangfe::Constant {
            name: c.0.into(),
            type_base: c.1.into(),
            value: c.2,
        }
    }
}
