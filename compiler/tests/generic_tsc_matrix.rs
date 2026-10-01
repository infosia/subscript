//! The generic body matrix of §143 rule 4.
//! Measured cost: 17,488 cells in 22.174 seconds, with one TypeScript process.
//! The 30-second budget covers each distinct value-role/site pair; no cell is removed for cost.
//! Concrete controls check instance admission apart from the opaque diagnostics (§143 rule 4).

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

#[path = "generic_tsc_matrix/product.rs"]
mod product;

#[path = "generic_tsc_matrix/findings.rs"]
mod findings;

struct Kind {
    name: &'static str,
    constraint: &'static str,
    value_type: &'static str,
    argument_type: &'static str,
    argument: &'static str,
}

const KINDS: &[Kind] = &[
    Kind {
        name: "plain",
        constraint: "",
        value_type: "T",
        argument_type: "i32",
        argument: "3",
    },
    Kind {
        name: "class",
        constraint: " extends Box",
        value_type: "T",
        argument_type: "Box",
        argument: "new Box()",
    },
    Kind {
        name: "numeric",
        constraint: " extends i32",
        value_type: "T",
        argument_type: "i32",
        argument: "3",
    },
    Kind {
        name: "f64",
        constraint: " extends f64",
        value_type: "T",
        argument_type: "i32",
        argument: "3",
    },
    Kind {
        name: "u8",
        constraint: " extends u8",
        value_type: "T",
        argument_type: "i32",
        argument: "3",
    },
    Kind {
        name: "array",
        constraint: " extends i32[]",
        value_type: "T",
        argument_type: "i32[]",
        argument: "[3]",
    },
    Kind {
        name: "nullable",
        constraint: " extends Box",
        value_type: "T | null",
        argument_type: "Box",
        argument: "new Box()",
    },
];

struct Form {
    name: &'static str,
    body: &'static str,
}

const FORMS: &[Form] = &[
    Form {
        name: "iteration-compound",
        body: "let total: i32 = 0; for (const e of values) total += e;",
    },
    Form {
        name: "s-add-x",
        body: "let total: i32 = 0; total += x;",
    },
    Form {
        name: "s-subtract-x",
        body: "let total: i32 = 0; total -= x;",
    },
    Form {
        name: "s-bitand-x",
        body: "let total: i32 = 0; total &= x;",
    },
    Form {
        name: "field-add-x",
        body: "box.v += x;",
    },
    Form {
        name: "callback-condition",
        body: "const cb = (a: T): void => {}; if (cb) {}",
    },
    Form {
        name: "remainder-equality",
        body: "const a = x % 2 === 0;",
    },
    Form {
        name: "fresh-equality",
        body: "const a = x + 1 === y;",
    },
    Form {
        name: "fresh-index",
        body: "const a = numbers[x + 1];",
    },
    Form {
        name: "fresh-bitand",
        body: "const a = (x + 1) & (y + 1);",
    },
    Form {
        name: "minus-string-equality",
        body: "const a = -x === \"a\";",
    },
    Form {
        name: "loose-equality",
        body: "const a = x == y;",
    },
    Form {
        name: "loose-inequality",
        body: "const a = x != y;",
    },
    Form {
        name: "copy",
        body: "const a = x; let copied = y; copied = a;",
    },
    Form {
        name: "identity",
        body: "const a: T = x;",
    },
    Form {
        name: "member-read",
        body: "const a = x.v;",
    },
    Form {
        name: "member-write",
        body: "x.v = 3;",
    },
    Form {
        name: "method",
        body: "const a = x.get();",
    },
    Form {
        name: "call",
        body: "x();",
    },
    Form {
        name: "add-literal",
        body: "const a = x + 1;",
    },
    Form {
        name: "add-same",
        body: "const a = x + y;",
    },
    Form {
        name: "add-string",
        body: "const a: string = x + s;",
    },
    Form {
        name: "subtract",
        body: "const a = x - y;",
    },
    Form {
        name: "multiply",
        body: "const a = x * 2;",
    },
    Form {
        name: "divide",
        body: "const a = x / y;",
    },
    Form {
        name: "remainder",
        body: "const a = x % y;",
    },
    Form {
        name: "bitwise",
        body: "const a = x & y;",
    },
    Form {
        name: "shift",
        body: "const a = x << y;",
    },
    Form {
        name: "relational-literal",
        body: "const a = x > 1;",
    },
    Form {
        name: "relational-same",
        body: "const a = x < y;",
    },
    Form {
        name: "relational-distinct",
        body: "const a = x < u;",
    },
    Form {
        name: "relational-string",
        body: "const a = x < s;",
    },
    Form {
        name: "relational-boolean",
        body: "const a = x < b;",
    },
    Form {
        name: "equality-same",
        body: "const a = x === y;",
    },
    Form {
        name: "equality-distinct",
        body: "const a = x === u;",
    },
    Form {
        name: "equality-number",
        body: "const a = x === 1;",
    },
    Form {
        name: "assert-number",
        body: "const a = x as i32;",
    },
    Form {
        name: "assert-distinct",
        body: "const a = x as U;",
    },
    Form {
        name: "logical-or",
        body: "const a = x || y;",
    },
    Form {
        name: "logical-and",
        body: "const a = x && y;",
    },
    Form {
        name: "logical-or-mismatch",
        body: "const a: boolean = b || x;",
    },
    Form {
        name: "logical-and-mismatch",
        body: "const a: boolean = b && x;",
    },
    Form {
        name: "logical-right-mismatch",
        body: "const a: boolean = x || b;",
    },
    Form {
        name: "logical-identity",
        body: "const a: T = x || y;",
    },
    Form {
        name: "unary-minus",
        body: "const a: i32 = -x;",
    },
    Form {
        name: "unary-tilde",
        body: "const a: i32 = ~x;",
    },
    Form {
        name: "unary-bang",
        body: "const a: boolean = !x;",
    },
    Form {
        name: "unary-mismatch",
        body: "const a: T = -x;",
    },
    Form {
        name: "fresh-number-tilde",
        body: "const a: i32 = ~(-x);",
    },
    Form {
        name: "fresh-number-pair",
        body: "const a = -x; const c = ~y; const z = a + c;",
    },
    Form {
        name: "fresh-number-literal",
        body: "const a = -x; let c = a + 1; c++;",
    },
    Form {
        name: "fresh-number-compare",
        body: "const a = -x < -y;",
    },
    Form {
        name: "fresh-number-array",
        body: "const a = [-x, 1];",
    },
    Form {
        name: "template",
        body: "const a: string = `${x}`;",
    },
    Form {
        name: "array-join",
        body: "const a: string = values.join(\",\");",
    },
    Form {
        name: "array-push",
        body: "values.push(x); const a = values[0];",
    },
    Form {
        name: "set-annotation",
        body: "const a: Set<T> = new Set<T>();",
    },
    Form {
        name: "map-key-annotation",
        body: "const a: Map<T, i32> = new Map<T, i32>();",
    },
    Form {
        name: "map-value-annotation",
        body: "const a: Map<string, T> = new Map<string, T>();",
    },
    Form {
        name: "array-annotation",
        body: "const a: Array<T> = [];",
    },
    Form {
        name: "fixed-array-annotation",
        body: "const a: FixedArray<T, 2> = [y, y];",
    },
    Form {
        name: "fixed-array-large-layout",
        body: "const a: FixedArray<T, 2147483647> = large;",
    },
    Form {
        name: "map-copy",
        body: "const m = new Map<T, i32>(); const a = new Map<T, i32>(m);",
    },
    Form {
        name: "map-group-key",
        body: "const a = Map.groupBy(values, (e: T): T => e);",
    },
    Form {
        name: "map-set",
        body: "const a = new Map<T, T>(); a.set(y, y); const c = a.getOr(y, y);",
    },
    Form {
        name: "set-add",
        body: "const a = new Set<T>(); a.add(y);",
    },
    Form {
        name: "array-of",
        body: "const a = Array.of<T>(y);",
    },
    Form {
        name: "array-map",
        body: "const a = values.map((e: T): T => e);",
    },
    Form {
        name: "array-flatmap",
        body: "const a = values.flatMap((e: T): T[] => [e]);",
    },
    Form {
        name: "array-nullable-element",
        body: "const a: (T | null)[] = [];",
    },
    Form {
        name: "map-nullable-value",
        body: "const a: Map<string, T | null> = new Map<string, T | null>();",
    },
    Form {
        name: "map-nullable-key",
        body: "const a: Map<T | null, i32> = new Map<T | null, i32>();",
    },
    Form {
        name: "set-nullable-key",
        body: "const a: Set<T | null> = new Set<T | null>();",
    },
    Form {
        name: "array-map-output",
        body: "const v = y; const a = numbers.map((e: i32): T => v);",
    },
    Form {
        name: "array-reduce",
        body: "const a = numbers.reduce((a: T, e: i32): T => a, y);",
    },
    Form {
        name: "array-reduce-right",
        body: "const a = numbers.reduceRight((a: T, e: i32): T => a, y);",
    },
    Form {
        name: "array-find",
        body: "const a = values.find((e: T): boolean => true);",
    },
    Form {
        name: "array-includes",
        body: "const a = values.includes(y);",
    },
    Form {
        name: "map-get",
        body: "const a = new Map<string, T>(); const c = a.get(s);",
    },
    Form {
        name: "array-from",
        body: "const a = Array.from<T>(values);",
    },
    Form {
        name: "map-key",
        body: "const a = new Map<T, i32>();",
    },
    Form {
        name: "map-value",
        body: "const a = new Map<string, T>();",
    },
    Form {
        name: "set-key",
        body: "const a = new Set<T>();",
    },
    Form {
        name: "string-mismatch",
        body: "const a: string = x;",
    },
    Form {
        name: "constraint-to-parameter",
        body: "const a: T = box;",
    },
    Form {
        name: "parameter-to-constraint",
        body: "const a: Box = x;",
    },
    Form {
        name: "nullable-declaration",
        body: "const a: T | null = null;",
    },
    Form {
        name: "nullish-same",
        body: "const a = x ?? y;",
    },
    Form {
        name: "nullish-box",
        body: "const a = x ?? box;",
    },
    Form {
        name: "nullish-mismatch",
        body: "const a: T = x ?? box;",
    },
    Form {
        name: "narrowing",
        body: "if (x !== null) { const a = x.v; }",
    },
    Form {
        name: "iteration",
        body: "for (const e of x) { const a = e; }",
    },
    Form {
        name: "spread",
        body: "const a = [...x];",
    },
    Form {
        name: "index",
        body: "const a = numbers[x];",
    },
    Form {
        name: "update",
        body: "let a = x; a++;",
    },
    Form {
        name: "switch-same",
        body: "switch (x) { case y: break; }",
    },
    Form {
        name: "switch-number",
        body: "switch (x) { case 1: break; }",
    },
    Form {
        name: "switch-string",
        body: "switch (x) { case \"a\": break; }",
    },
    Form {
        name: "conditional",
        body: "const a = x ? y : x;",
    },
    Form {
        name: "if-condition",
        body: "if (x) { const a = y; }",
    },
    Form {
        name: "string-compound",
        body: "let a: string = \"\"; a += x;",
    },
    Form {
        name: "logical-and-right",
        body: "const a: boolean = x && b;",
    },
    Form {
        name: "relational-class",
        body: "const a = x < box;",
    },
    Form {
        name: "relational-fresh",
        body: "const a = x < -x;",
    },
    Form {
        name: "logical-relational",
        body: "const a = (x || y) < x;",
    },
    Form {
        name: "nullish-relational",
        body: "const a = (x ?? y) < x;",
    },
    Form {
        name: "while-condition",
        body: "while (x) { break; }",
    },
    Form {
        name: "do-condition",
        body: "do { break; } while (x);",
    },
    Form {
        name: "for-condition",
        body: "for (; x;) { break; }",
    },
    Form {
        name: "assert-to-parameter",
        body: "const a = box as T;",
    },
    Form {
        name: "nested-constraint",
        body: "take<T>(x);",
    },
];

#[derive(Clone, Copy)]
struct Divergence {
    code: RuleCode,
    record: &'static str,
    token: &'static str,
}

const LOOSE_EQUALITY: Divergence = Divergence {
    code: RuleCode::S100,
    record: "C20",
    token: "rejects `==` and `!=` with S100 on every operand type",
};

const DO_WHILE: Divergence = Divergence {
    code: RuleCode::S100,
    record: "compiler.md §124",
    token: "`do…while` stays outside it",
};

const VALUE_FIELD: Divergence = Divergence {
    code: RuleCode::S100,
    record: "C2",
    token: concat!(
        "Reference-class fields, `string` fields, and\n",
        "  nullable fields inside value classes are deferred",
    ),
};

struct Cell {
    name: String,
    source: String,
    divergence: Option<Divergence>,
    concrete_source: Option<String>,
}

struct CellInput<'a> {
    name: &'a str,
    declaration: &'a str,
    main_body: &'a str,
    instance: bool,
    concrete_source: Option<String>,
    divergence: Option<Divergence>,
}

fn build_cell(input: CellInput<'_>) -> Cell {
    let column = if input.instance {
        "instance"
    } else {
        "no-instance"
    };
    Cell {
        name: format!("{}-{column}", input.name),
        source: format!(
            "{}\nexport function main(): void {{ {} }}\n",
            input.declaration, input.main_body
        ),
        divergence: input.divergence,
        concrete_source: input.concrete_source,
    }
}

fn form_declaration(kind: &Kind, form: &Form) -> String {
    let constraint = kind.constraint;
    let ty = kind.value_type;
    let body = form.body;
    let extra = if form.name == "fixed-array-large-layout" {
        ", large: FixedArray<T, 2147483647>"
    } else {
        ""
    };
    format!(
        "class Box {{ v: i32 = 1; get(): i32 {{ return this.v; }} }}\n\
         function take<A extends Box>(a: A): void {{}}\n\
         function g<T{constraint}, U{constraint}>(\
         x: {ty}, y: {ty}, u: U, s: string, b: boolean, box: Box, \
         numbers: i32[], values: T[]{extra}): void {{ {body} }}"
    )
}

fn form_main(ty: &str, value: &str) -> String {
    format!(
        "g<{ty}, {ty}>({value}, {value}, {value}, \
         \"s\", true, new Box(), [3], [{value}]);"
    )
}

fn replace_parameter_names(source: &str, ty: &str) -> String {
    let mut result = String::new();
    let mut word = String::new();
    for c in source.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            result.push_str(if word == "T" || word == "U" || word == "V" {
                ty
            } else {
                &word
            });
            word.clear();
            result.push(c);
        }
    }
    result
}

// §143 rule 4 requires an accepted concrete instance. The tracking note lists each omission.
fn instance_argument(kind: &Kind, form: &Form) -> Option<(&'static str, &'static str)> {
    let new_numeric_form = matches!(
        form.name,
        "s-add-x"
            | "s-subtract-x"
            | "s-bitand-x"
            | "field-add-x"
            | "iteration-compound"
            | "remainder-equality"
            | "fresh-equality"
            | "fresh-index"
            | "fresh-bitand"
            | "minus-string-equality"
    );
    if form.name == "callback-condition"
        || form.name == "minus-string-equality"
        || (new_numeric_form && matches!(kind.name, "class" | "array" | "nullable"))
    {
        return None;
    }
    let omitted: &[&str] = match kind.name {
        "plain" => &[
            "relational-string",
            "relational-boolean",
            "fixed-array-large-layout",
            "map-nullable-key",
            "set-nullable-key",
            "relational-class",
            "logical-relational",
            "nullish-relational",
            "assert-to-parameter",
        ],
        "class" => &[
            "call",
            "add-literal",
            "add-same",
            "add-string",
            "subtract",
            "multiply",
            "divide",
            "remainder",
            "bitwise",
            "shift",
            "relational-literal",
            "relational-same",
            "relational-distinct",
            "relational-string",
            "relational-boolean",
            "equality-number",
            "assert-number",
            "assert-distinct",
            "logical-or",
            "logical-and",
            "logical-or-mismatch",
            "logical-and-mismatch",
            "logical-right-mismatch",
            "logical-identity",
            "unary-minus",
            "unary-tilde",
            "unary-bang",
            "unary-mismatch",
            "fresh-number-tilde",
            "fresh-number-pair",
            "fresh-number-literal",
            "fresh-number-compare",
            "fresh-number-array",
            "template",
            "array-join",
            "fixed-array-large-layout",
            "map-nullable-key",
            "set-nullable-key",
            "string-mismatch",
            "nullish-same",
            "nullish-box",
            "nullish-mismatch",
            "narrowing",
            "iteration",
            "spread",
            "index",
            "update",
            "switch-same",
            "switch-number",
            "switch-string",
            "conditional",
            "if-condition",
            "string-compound",
            "logical-and-right",
            "relational-class",
            "relational-fresh",
            "logical-relational",
            "nullish-relational",
            "while-condition",
            "for-condition",
            "assert-to-parameter",
        ],
        "numeric" | "f64" | "u8" => &[
            "member-read",
            "member-write",
            "method",
            "call",
            "add-string",
            "relational-string",
            "relational-boolean",
            "logical-or",
            "logical-and",
            "logical-or-mismatch",
            "logical-and-mismatch",
            "logical-right-mismatch",
            "logical-identity",
            "unary-bang",
            "fixed-array-large-layout",
            "array-nullable-element",
            "map-nullable-value",
            "map-nullable-key",
            "set-nullable-key",
            "array-find",
            "map-get",
            "string-mismatch",
            "constraint-to-parameter",
            "parameter-to-constraint",
            "nullable-declaration",
            "nullish-same",
            "nullish-box",
            "nullish-mismatch",
            "narrowing",
            "iteration",
            "spread",
            "switch-string",
            "conditional",
            "if-condition",
            "string-compound",
            "logical-and-right",
            "relational-class",
            "logical-relational",
            "nullish-relational",
            "while-condition",
            "for-condition",
            "assert-to-parameter",
            "nested-constraint",
        ],
        "array" => &[
            "member-read",
            "member-write",
            "method",
            "call",
            "add-literal",
            "add-same",
            "add-string",
            "subtract",
            "multiply",
            "divide",
            "remainder",
            "bitwise",
            "shift",
            "relational-literal",
            "relational-same",
            "relational-distinct",
            "relational-string",
            "relational-boolean",
            "equality-same",
            "equality-distinct",
            "equality-number",
            "assert-number",
            "assert-distinct",
            "logical-or",
            "logical-and",
            "logical-or-mismatch",
            "logical-and-mismatch",
            "logical-right-mismatch",
            "logical-identity",
            "unary-minus",
            "unary-tilde",
            "unary-bang",
            "unary-mismatch",
            "fresh-number-tilde",
            "fresh-number-pair",
            "fresh-number-literal",
            "fresh-number-compare",
            "fresh-number-array",
            "template",
            "array-join",
            "set-annotation",
            "map-key-annotation",
            "fixed-array-large-layout",
            "map-copy",
            "map-group-key",
            "map-set",
            "set-add",
            "array-nullable-element",
            "map-nullable-value",
            "map-nullable-key",
            "set-nullable-key",
            "map-key",
            "set-key",
            "string-mismatch",
            "constraint-to-parameter",
            "parameter-to-constraint",
            "nullable-declaration",
            "nullish-same",
            "nullish-box",
            "nullish-mismatch",
            "narrowing",
            "index",
            "update",
            "switch-same",
            "switch-number",
            "switch-string",
            "conditional",
            "if-condition",
            "string-compound",
            "logical-and-right",
            "relational-class",
            "relational-fresh",
            "logical-relational",
            "nullish-relational",
            "while-condition",
            "for-condition",
            "assert-to-parameter",
            "nested-constraint",
        ],
        "nullable" => &[
            "identity",
            "member-read",
            "member-write",
            "method",
            "call",
            "add-literal",
            "add-same",
            "add-string",
            "subtract",
            "multiply",
            "divide",
            "remainder",
            "bitwise",
            "shift",
            "relational-literal",
            "relational-same",
            "relational-distinct",
            "relational-string",
            "relational-boolean",
            "equality-same",
            "equality-distinct",
            "equality-number",
            "assert-number",
            "assert-distinct",
            "logical-or",
            "logical-and",
            "logical-or-mismatch",
            "logical-and-mismatch",
            "logical-right-mismatch",
            "logical-identity",
            "unary-minus",
            "unary-tilde",
            "unary-bang",
            "unary-mismatch",
            "fresh-number-tilde",
            "fresh-number-pair",
            "fresh-number-literal",
            "fresh-number-compare",
            "fresh-number-array",
            "template",
            "array-join",
            "array-push",
            "fixed-array-annotation",
            "fixed-array-large-layout",
            "map-set",
            "set-add",
            "array-of",
            "map-nullable-key",
            "set-nullable-key",
            "array-map-output",
            "array-reduce",
            "array-reduce-right",
            "array-includes",
            "string-mismatch",
            "parameter-to-constraint",
            "iteration",
            "spread",
            "index",
            "update",
            "switch-same",
            "switch-number",
            "switch-string",
            "conditional",
            "if-condition",
            "string-compound",
            "logical-and-right",
            "relational-class",
            "relational-fresh",
            "logical-relational",
            "nullish-relational",
            "while-condition",
            "for-condition",
            "assert-to-parameter",
            "nested-constraint",
        ],
        _ => unreachable!("matrix kind"),
    };
    if omitted.contains(&form.name) {
        return None;
    }
    if kind.name == "plain" {
        let argument = match form.name {
            "call" => ("() => void", "(): void => {}"),
            "member-read" => ("Box", "new Box()"),
            "member-write" => ("Box", "new Box()"),
            "method" => ("Box", "new Box()"),
            "array-nullable-element" => ("Box", "new Box()"),
            "map-nullable-value" => ("Box", "new Box()"),
            "array-find" => ("Box", "new Box()"),
            "map-get" => ("Box", "new Box()"),
            "constraint-to-parameter" => ("Box", "new Box()"),
            "parameter-to-constraint" => ("Box", "new Box()"),
            "nullable-declaration" => ("Box", "new Box()"),
            "nested-constraint" => ("Box", "new Box()"),
            "add-string" => ("string", "\"s\""),
            "string-mismatch" => ("string", "\"s\""),
            "iteration" => ("string", "\"s\""),
            "spread" => ("string", "\"s\""),
            "switch-string" => ("string", "\"s\""),
            "string-compound" => ("string", "\"s\""),
            "logical-or" => ("boolean", "true"),
            "logical-and" => ("boolean", "true"),
            "logical-or-mismatch" => ("boolean", "true"),
            "logical-and-mismatch" => ("boolean", "true"),
            "logical-right-mismatch" => ("boolean", "true"),
            "logical-identity" => ("boolean", "true"),
            "unary-bang" => ("boolean", "true"),
            "conditional" => ("boolean", "true"),
            "if-condition" => ("boolean", "true"),
            "logical-and-right" => ("boolean", "true"),
            "while-condition" => ("boolean", "true"),
            "for-condition" => ("boolean", "true"),
            "nullish-same" => ("Box | null", "null"),
            "nullish-box" => ("Box | null", "null"),
            "nullish-mismatch" => ("Box | null", "null"),
            "narrowing" => ("Box | null", "null"),
            _ => (kind.argument_type, kind.argument),
        };
        return Some(argument);
    }
    Some((kind.argument_type, kind.argument))
}

fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    for kind in KINDS {
        for form in FORMS {
            let name = format!("{}-{}", kind.name, form.name);
            let declaration = form_declaration(kind, form);
            let divergence = if form.name.starts_with("loose-") {
                Some(LOOSE_EQUALITY)
            } else if form.name == "do-condition" {
                Some(DO_WHILE)
            } else {
                None
            };
            for instance in [false, true] {
                let main_body = if instance {
                    let Some((ty, value)) = instance_argument(kind, form) else {
                        continue;
                    };
                    form_main(ty, value)
                } else {
                    String::new()
                };
                cells.push(build_cell(CellInput {
                    name: &name,
                    declaration: &declaration,
                    main_body: &main_body,
                    instance,
                    concrete_source: if instance {
                        let (ty, _) = instance_argument(kind, form).unwrap();
                        let replacement = if ty.contains('|') || ty.contains("=>") {
                            format!("({ty})")
                        } else {
                            ty.to_string()
                        };
                        let concrete = declaration.replace(
                            &format!("g<T{}, U{}>", kind.constraint, kind.constraint),
                            "g",
                        );
                        Some(format!(
                            "{}\nexport function main(): void {{ {} }}",
                            replace_parameter_names(&concrete, &replacement),
                            main_body.replace(&format!("g<{ty}, {ty}>"), "g")
                        ))
                    } else {
                        None
                    },
                    divergence,
                }));
            }
        }
    }
    let extra = [
        (
            "measured-relational-literal",
            "function gf<T>(x: T): boolean { return x > 1; }",
            Some("gf<i32>(3);"),
            None,
        ),
        (
            "nullable-constraint-minus",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends Box | null>(x: T): ",
                "void { const a = -x; }",
            ),
            None,
            None,
        ),
        (
            "nullable-constraint-tilde",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends Box | null>(x: T): ",
                "void { const a = ~x; }",
            ),
            None,
            None,
        ),
        (
            "forward-constraint",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends U, U extends Box>",
                "(x: T): i32 { return x.v; }",
            ),
            Some("g<Box, Box>(new Box());"),
            None,
        ),
        (
            "linked-parameter-copy",
            concat!("function g<U, T extends U>(x: T): U { return x; }",),
            Some("g<i32, i32>(3);"),
            None,
        ),
        (
            "linked-parameter-equality",
            concat!(
                "function g<U, T extends U>(x: T, y: U): boolean { return x === y; ",
                "}",
            ),
            Some("g<i32, i32>(3, 3);"),
            None,
        ),
        (
            "linked-parameter-cast",
            concat!("function g<U, T extends U>(x: T): U { return x as U; }",),
            Some("g<i32, i32>(3);"),
            None,
        ),
        (
            "deep-constraint-backward",
            concat!(
                "class Box { v: i32 = 1; } function g<V extends Box, U extends V, ",
                "T extends U>(x: T): U { return x; }"
            ),
            Some("g<Box, Box, Box>(new Box());"),
            None,
        ),
        (
            "deep-constraint-forward",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends U, U extends V, ",
                "V extends Box>(x: T): U { return x; }"
            ),
            Some("g<Box, Box, Box>(new Box());"),
            None,
        ),
        (
            "cyclic-self-constraint",
            "function g<T extends T>(x: T): i32 { return 0; }",
            None,
            None,
        ),
        (
            "cyclic-pair-constraint",
            "function g<T extends U, U extends T>(x: T): i32 { return 0; }",
            None,
            None,
        ),
        (
            "uninitialized-field",
            concat!("class G<T> { v: i32; w: T | null = null; }",),
            None,
            None,
        ),
        (
            "class-field-assignment",
            concat!(
                "class G<T> { v: i32 = 0; w: T; constructor(w: T) { this.w = w; } ",
                "run(): void { this.v = this.w; } }",
            ),
            Some("const a = new G<i32>(3); a.run();"),
            None,
        ),
        (
            "value-string-field",
            concat!(
                "@ValueType class G<T> { v: string = \"\"; w: T; constructor(w: T) { ",
                "this.w = w; } }",
            ),
            Some("const a = new G<i32>(3);"),
            Some(VALUE_FIELD),
        ),
        (
            "value-parameter-field",
            concat!("@ValueType class G<T> { w: T; constructor(w: T) { this.w = w; } }",),
            Some("const a = new G<i32>(3);"),
            None,
        ),
        (
            "value-fixed-parameter-field",
            concat!(
                "@ValueType class G<T> { w: FixedArray<T, 2>; constructor(w: ",
                "FixedArray<T, 2>) { this.w = w; } }",
            ),
            Some("const a = new G<i32>([3, 3]);"),
            None,
        ),
        (
            "nullable-constraint-narrowing",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends Box | null>(x: T): ",
                "i32 { if (x === null) return 0; return x.v; }",
            ),
            Some("g<Box | null>(null);"),
            None,
        ),
        (
            "nullable-constraint-nullish",
            concat!(
                "class Box { v: i32 = 1; } function g<T extends Box | null>(x: T, ",
                "b: Box): Box { return x ?? b; }",
            ),
            Some("g<Box | null>(null, new Box());"),
            None,
        ),
        (
            "boolean-constraint-and",
            concat!(
                "function g<T extends boolean>(x: T, y: T): boolean { return y && ",
                "x; }",
            ),
            Some("g<boolean>(true, false);"),
            None,
        ),
        (
            "boolean-constraint-bang",
            concat!("function g<T extends boolean>(x: T): boolean { return !x && x; }",),
            Some("g<boolean>(true);"),
            None,
        ),
        (
            "unknown-name",
            concat!("function g<T>(x: T): i32 { return nope(); }",),
            None,
            None,
        ),
        (
            "const-write",
            concat!("function g<T>(x: T): void { const a: i32 = 1; a = 2; }",),
            None,
            None,
        ),
    ];
    for (name, declaration, main_body, divergence) in extra {
        for instance in [false, true] {
            if instance && main_body.is_none() {
                continue;
            }
            cells.push(build_cell(CellInput {
                name,
                declaration,
                main_body: if instance { main_body.unwrap() } else { "" },
                instance,
                concrete_source: if instance {
                    Some(extra_concrete_source(name, declaration, main_body.unwrap()))
                } else {
                    None
                },
                divergence,
            }));
        }
    }
    cells.extend(product::cells());
    cells.extend(findings::cells());
    cells
}

fn extra_concrete_source(name: &str, declaration: &str, main_body: &str) -> String {
    let (parameters, ty) = if name == "forward-constraint" {
        ("<T extends U, U extends Box>", "Box")
    } else if name == "deep-constraint-backward" {
        ("<V extends Box, U extends V, T extends U>", "Box")
    } else if name == "deep-constraint-forward" {
        ("<T extends U, U extends V, V extends Box>", "Box")
    } else if name.starts_with("linked-parameter-") {
        ("<U, T extends U>", "i32")
    } else if name.starts_with("nullable-constraint-") {
        ("<T extends Box | null>", "Box | null")
    } else if name.starts_with("boolean-constraint-") {
        ("<T extends boolean>", "boolean")
    } else {
        ("<T>", "i32")
    };
    let concrete = declaration.replace(parameters, "");
    let replacement = if ty.contains('|') || ty.contains("=>") {
        format!("({ty})")
    } else {
        ty.to_string()
    };
    let main = main_body
        .replace(&format!("<{ty}>"), "")
        .replace(&format!("<{ty}, {ty}>"), "")
        .replace(&format!("<{ty}, {ty}, {ty}>"), "");
    format!(
        "{}\nexport function main(): void {{ {main} }}",
        replace_parameter_names(&concrete, &replacement),
    )
}

fn disagreement(
    cell: &Cell,
    tsc_accepts: bool,
    diagnostics: &[Diagnostic],
    record_result: Result<(), String>,
) -> Option<String> {
    if !tsc_accepts && diagnostics.is_empty() {
        return Some(format!("{}: §143 rule 4a", cell.name));
    }
    if tsc_accepts && !diagnostics.is_empty() {
        if !cell
            .divergence
            .is_some_and(|record| diagnostics.iter().all(|d| d.code == record.code))
        {
            return Some(format!("{}: §143 rule 4b {diagnostics:?}", cell.name));
        }
    }
    if let Err(error) = record_result {
        return Some(format!("{}: §143 rule 4b {error}", cell.name));
    }
    None
}

fn record_section<'a>(text: &'a str, heading: &str) -> Result<&'a str, String> {
    let start = text
        .match_indices(heading)
        .find(|(offset, _)| *offset == 0 || text.as_bytes()[offset - 1] == b'\n')
        .map(|(offset, _)| offset)
        .ok_or_else(|| format!("missing record {heading}"))?;
    let level = heading.chars().take_while(|c| *c == '#').count();
    let rest = &text[start..];
    let mut offset = rest.find('\n').map_or(rest.len(), |end| end + 1);
    for line in rest[offset..].split_inclusive('\n') {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && hashes <= level && line.as_bytes().get(hashes) == Some(&b' ') {
            return Ok(&rest[..offset]);
        }
        offset += line.len();
    }
    Ok(rest)
}

fn check_record_text(text: &str, heading: &str, token: &str) -> Result<(), String> {
    let section = record_section(text, heading)?;
    if !section.contains(token) {
        return Err(format!(
            "record {heading} does not state restriction {token:?}"
        ));
    }
    Ok(())
}

fn check_record(root: &Path, record: Divergence) -> Result<(), String> {
    let blocks = root.join("specs/blocks");
    let (path, heading) = if let Some(number) = record.record.strip_prefix("compiler.md §") {
        let index = fs::read_to_string(blocks.join("compiler.md")).map_err(|e| e.to_string())?;
        let section = record_section(&index, "## 0.")?;
        let row = section
            .lines()
            .find(|line| line.starts_with(&format!("| §{number} |")))
            .ok_or_else(|| format!("missing index entry {}", record.record))?;
        let link = row
            .split_once("](")
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(link, _)| link)
            .ok_or_else(|| format!("missing index link {}", record.record))?;
        (blocks.join(link), format!("## {number}."))
    } else {
        (
            blocks.join("collisions.md"),
            format!("### {}.", record.record),
        )
    };
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    check_record_text(&text, &heading, record.token)
}

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generic_forms_follow_tsc() {
    let start = Instant::now();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let project = Project(std::env::temp_dir().join(format!(
            "subscript-generic-matrix-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir(&project.0).unwrap();
    let cells = cells();
    let mut files = vec![root.join("prelude/lang.d.ts")];
    for cell in &cells {
        let path = project.0.join(format!("{}.ts", cell.name));
        fs::write(&path, &cell.source).unwrap();
        files.push(path);
    }
    let listed = files
        .iter()
        .map(|p| format!("{:?}", p.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
    let config = project.0.join("tsconfig.json");
    let options = concat!(
        r#""strict":true,"noEmit":true,"target":"ES2022","module":"ESNext","#,
        r#""moduleResolution":"Bundler","lib":["ES2022","ESNext.Disposable"],"#,
        r#""types":[],"forceConsistentCasingInFileNames":true"#,
    );
    fs::write(
        &config,
        format!(r#"{{"compilerOptions":{{{options}}},"files":[{listed}]}}"#),
    )
    .unwrap();
    let output = Command::new(root.join(if cfg!(windows) {
        "node_modules/.bin/tsc.cmd"
    } else {
        "node_modules/.bin/tsc"
    }))
    .args(["--project", config.to_str().unwrap(), "--pretty", "false"])
    .output()
    .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let cell_names: HashSet<_> = cells.iter().map(|cell| cell.name.as_str()).collect();
    let mut rejected = HashSet::new();
    for line in text.lines().filter(|l| l.contains("error TS")) {
        let (file, _) = line
            .split_once('(')
            .unwrap_or_else(|| panic!("unowned diagnostic: {line}"));
        let name = std::path::Path::new(file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            cell_names.contains(name.as_str()),
            "unowned diagnostic: {line}"
        );
        rejected.insert(name);
    }
    assert!(output.status.success() || !rejected.is_empty(), "{text}");
    let mut failures = Vec::new();
    for cell in &cells {
        if cell.name.starts_with("derived-") && rejected.contains(&cell.name) {
            failures.push(format!(
                "{}: the TypeScript accept control rejects",
                cell.name
            ));
        }
        let record_result = cell
            .divergence
            .map_or(Ok(()), |record| check_record(&root, record));
        if let Some(source) = &cell.concrete_source {
            let errors = check_program(&[SourceFile::new("concrete.ts", source.clone())])
                .err()
                .unwrap_or_default();
            if !errors.is_empty()
                && !cell
                    .divergence
                    .is_some_and(|record| errors.iter().all(|error| error.code == record.code))
            {
                failures.push(format!(
                    "{}: per-instance control rejected: {errors:?}",
                    cell.name
                ));
            }
        }
        let errors = check_program(&[SourceFile::new(
            format!("{}.ts", cell.name),
            cell.source.clone(),
        )])
        .err()
        .unwrap_or_default();
        if let Some(failure) =
            disagreement(cell, !rejected.contains(&cell.name), &errors, record_result)
        {
            failures.push(failure);
        }
    }
    eprintln!(
        "matrix columns: no-instance={}, instance={}",
        cells
            .iter()
            .filter(|cell| cell.name.ends_with("-no-instance"))
            .count(),
        cells
            .iter()
            .filter(|cell| cell.name.ends_with("-instance") && !cell.name.ends_with("-no-instance"))
            .count()
    );
    eprintln!(
        "generic matrix: {} cells, {} failures, {:?}",
        cells.len(),
        failures.len(),
        start.elapsed()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn each_failure_kind_is_reported() {
    let cell = Cell {
        name: "bad-operator".into(),
        source: "function g<T>(x: T): boolean { return x > 1; }".into(),
        divergence: None,
        concrete_source: None,
    };
    assert!(disagreement(&cell, false, &[], Ok(()))
        .unwrap()
        .contains("4a"));
    let source = "@ValueType class G<T> { value: string = \"\"; }";
    let errors = check_program(&[SourceFile::new("restriction.ts", source)]).unwrap_err();
    let cell = Cell {
        name: "missing-record".into(),
        source: source.into(),
        divergence: None,
        concrete_source: None,
    };
    assert!(disagreement(&cell, true, &errors, Ok(()))
        .unwrap()
        .contains("4b"));
}

#[test]
fn a_record_that_does_not_state_the_restriction_fails_the_cell() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = "@ValueType class G<T> { value: string = \"\"; }";
    let errors = check_program(&[SourceFile::new("restriction.ts", source)]).unwrap_err();
    let record = Divergence {
        code: RuleCode::S100,
        record: "C20",
        token: VALUE_FIELD.token,
    };
    let cell = build_cell(CellInput {
        name: "wrong-record",
        declaration: source,
        main_body: "",
        instance: false,
        concrete_source: None,
        divergence: Some(record),
    });
    let failure = disagreement(&cell, true, &errors, check_record(root, record)).unwrap();
    assert!(failure.contains("§143 rule 4b"), "{failure}");
    assert!(failure.contains("does not state restriction"), "{failure}");
}

#[test]
fn a_restriction_in_another_section_does_not_count() {
    let text = concat!(
        "### C1. Record\nNo operator restriction.\n",
        "#### Detail\nThis detail belongs to C1.\n",
        "### C2. Other record\nThe checker rejects `==`.\n",
    );
    assert!(check_record_text(text, "### C1.", "rejects `==`").is_err());
    assert!(check_record_text(text, "### C1.", "This detail belongs to C1.").is_ok());
    assert!(check_record_text(text, "### C2.", "rejects `==`").is_ok());
}

#[test]
fn every_union_member_must_supply_the_member() {
    let source = "class Box { v: i32 = 1; } class Other { w: i32 = 2; } function g<T extends Box, U extends Other>(x: T, y: U, c: boolean): i32 { return (c ? x : y).v; }";
    let errors = check_program(&[SourceFile::new("union.ts", source)]).unwrap_err();
    assert!(errors.iter().any(|error| error.code == RuleCode::S018));
}

#[test]
fn lambda_defaults_use_the_declared_parameter_type() {
    for (source, accepts) in [
        ("function g(): void { const v: i32 = 1; const f = (x: i32 = v): void => {}; }", true),
        ("function g<T extends i32>(v: i32): void { const value = v; const f = (x: T = value): void => {}; }", false),
    ] {
        assert_eq!(check_program(&[SourceFile::new("default.ts", source)]).is_ok(), accepts);
    }
}
