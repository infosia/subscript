//! The value-role and consumer-site product of §143 rule 4.

use super::*;
use std::collections::HashMap;

/// Function constraints for the role/site product (§143 rule 4).
pub(super) const FUNCTION_KINDS: &[Kind] = &[
    Kind {
        name: "function-return",
        constraint: " extends () => i32",
        value_type: "T",
        argument_type: "() => i32",
        argument: "(): i32 => 1",
    },
    Kind {
        name: "function-parameter",
        constraint: " extends (x: i32) => void",
        value_type: "T",
        argument_type: "(x: i32) => void",
        argument: "(x: i32): void => {}",
    },
];

struct Role {
    name: &'static str,
    expression: &'static str,
    nullable: bool,
    order: Option<bool>,
}

const ROLES: &[Role] = &[
    Role {
        name: "ternary-result",
        expression: "true ? x : y",
        nullable: false,
        order: None,
    },
    Role {
        name: "nullish-result",
        expression: "x ?? y",
        nullable: false,
        order: None,
    },
    Role {
        name: "array-literal-element",
        expression: "[x, y][0]",
        nullable: false,
        order: None,
    },
    Role {
        name: "destructuring-source",
        expression: "derived",
        nullable: false,
        order: None,
    },
    Role {
        name: "throw-operand",
        expression: "identity<T>(x)",
        nullable: false,
        order: None,
    },
    Role {
        name: "array-element",
        expression: "xs[0]",
        nullable: false,
        order: None,
    },
    Role {
        name: "fixed-array-element",
        expression: "fixed[0]",
        nullable: false,
        order: None,
    },
    Role {
        name: "generic-field",
        expression: "holder.value",
        nullable: false,
        order: None,
    },
    Role {
        name: "generic-method",
        expression: "holder.get()",
        nullable: false,
        order: None,
    },
    Role {
        name: "closure-parameter",
        expression: "closure<T>(x)",
        nullable: false,
        order: None,
    },
    Role {
        name: "await-result",
        expression: "await promise<T>(x)",
        nullable: false,
        order: None,
    },
    Role {
        name: "yield-value",
        expression: "yielded",
        nullable: false,
        order: None,
    },
    Role {
        name: "call-return",
        expression: "identity<T>(x)",
        nullable: false,
        order: None,
    },
    Role {
        name: "parameter",
        expression: "x",
        nullable: false,
        order: None,
    },
    Role {
        name: "nullable",
        expression: "x",
        nullable: true,
        order: None,
    },
    Role {
        name: "fresh-add",
        expression: "x + 1",
        nullable: false,
        order: None,
    },
    Role {
        name: "fresh-minus",
        expression: "-x",
        nullable: false,
        order: None,
    },
    Role {
        name: "linked-backward",
        expression: "x",
        nullable: false,
        order: Some(false),
    },
    Role {
        name: "linked-forward",
        expression: "x",
        nullable: false,
        order: Some(true),
    },
    Role {
        name: "concrete",
        expression: "n",
        nullable: false,
        order: None,
    },
];

struct Site {
    name: String,
    body: String,
    result: &'static str,
    divergence: Option<Divergence>,
}

fn sites() -> Vec<Site> {
    let mut sites = Vec::new();
    for (name, ty, target) in [("u", "U", "u"), ("t", "T", "y"), ("concrete", "i32", "n")] {
        for (operation, body, result) in [
            ("initializer", format!("const a: {ty} = $;"), "void"),
            ("assignment", format!("{target} = $;"), "void"),
            ("return", "return $;".into(), ty),
            ("argument", format!("take<{ty}>($);"), "void"),
            (
                "push",
                format!("const out: {ty}[] = []; out.push($);"),
                "void",
            ),
        ] {
            sites.push(Site {
                name: format!("{operation}-{name}"),
                body,
                result,
                divergence: None,
            });
        }
        for (name_op, operator) in [("add", "+="), ("sub", "-="), ("and", "&=")] {
            sites.push(Site {
                name: format!("compound-{name_op}-{name}"),
                body: format!("{target} {operator} $;"),
                result: "void",
                divergence: None,
            });
        }
    }
    for (name, operator, divergence) in [
        ("equality", "===", None),
        ("loose-equality", "==", None),
        ("relational", "<", None),
    ] {
        for (side, left, right) in [("left", "$", "n"), ("right", "n", "$")] {
            sites.push(Site {
                name: format!("{name}-{side}"),
                body: format!("const a = {left} {operator} {right};"),
                result: "void",
                divergence,
            });
        }
    }
    for (name, body) in [
        ("assert-i32", "const a = $ as i32;"),
        ("assert-u8", "const a = $ as u8;"),
        ("assert-string", "const a = $ as string;"),
        ("call-value", "const a = ($)();"),
        ("call-value-argument", "($)(n);"),
        ("forward-argument", "const a = forward<T>($);"),
        (
            "class-field-type",
            "const a: Constrained<T> = new Constrained<T>($);",
        ),
        ("switch", "switch ($) { default: break; }"),
        (
            "case-parameter",
            "switch (y) { case $: break; default: break; }",
        ),
        (
            "case-number",
            "switch (n) { case $: break; default: break; }",
        ),
        (
            "case-string",
            "switch (s) { case $: break; default: break; }",
        ),
        ("condition", "if ($) {}"),
        ("template", "const a = `${$}`;"),
        ("index", "const a = numbers[$];"),
        (
            "map-key-parameter",
            "const m = new Map<T, i32>(); m.set($, 1);",
        ),
        (
            "map-key-concrete",
            "const m = new Map<i32, i32>(); m.set($, 1);",
        ),
    ] {
        sites.push(Site {
            name: name.into(),
            body: body.into(),
            result: "void",
            divergence: None,
        });
    }
    for (name, body) in [
        ("ternary-arm", "const a = true ? $ : y;"),
        ("nullish-right", "const a = y ?? $;"),
        ("array-literal", "const a = [$, y];"),
        ("object-pattern", "const { v } = $;"),
        ("array-pattern", "const [a] = $;"),
        ("throw", "throw $;"),
        ("field-initializer", "const a = $;"),
        (
            "default-parameter",
            "const default_value = $; const callback = (a: T = default_value): void => {};",
        ),
        ("for-of-binding", "for (const { v } of [$]) {}"),
        ("member-read", "const a = ($).v;"),
        ("method-call", "const a = ($).get();"),
    ] {
        sites.push(Site {
            name: name.into(),
            body: body.into(),
            result: "void",
            divergence: None,
        });
    }
    sites
}

fn field_value(source: &str) -> String {
    let mut result = String::new();
    let mut word = String::new();
    for character in source.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_alphanumeric() || character == '_' {
            word.push(character);
        } else {
            result.push_str(match word.as_str() {
                "x" => "this.read_x()",
                "y" => "this.read_y()",
                "n" => "this.read_n()",
                _ => &word,
            });
            word.clear();
            result.push(character);
        }
    }
    result
}

pub(super) fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    let sites = sites();
    let mut omitted = Vec::new();
    let mut controls = HashMap::new();
    for kind in &kinds::all() {
        for role in ROLES {
            let current = KINDS
                .iter()
                .chain(FUNCTION_KINDS)
                .any(|old| old.name == kind.name);
            if !current
                && !(matches!(role.name, "parameter" | "nullable")
                    || (role.name == "fresh-add" && kinds::numeric(kind)))
            {
                continue;
            }
            // Linked roles always have a constrained intermediate parameter (§143 rule 1d).
            let constraint = if role.order.is_some() && kind.constraint.is_empty() {
                " extends Box"
            } else {
                kind.constraint
            };
            let parameters = match role.order {
                Some(false) => format!("U{constraint}, T extends U"),
                Some(true) => format!("T extends U, U{constraint}"),
                None => format!("T{constraint}, U{constraint}"),
            };
            let x_type = if role.nullable { "T | null" } else { "T" };
            for site in &sites {
                let body = site.body.replace('$', &format!("({})", role.expression));
                let asynchronous = role.name == "await-result";
                let asynchronous_word = if asynchronous { "async " } else { "" };
                let result = if asynchronous {
                    format!("Promise<{}>", site.result)
                } else {
                    site.result.to_string()
                };
                let setup = match role.name {
                    "destructuring-source" => "const { value: derived } = new G<T>(y);",
                    "array-element" => "const xs: T[] = [y];",
                    "fixed-array-element" => "const fixed: FixedArray<T, 1> = [y];",
                    "generic-field" | "generic-method" => "const holder = new G<T>(y);",
                    "yield-value" => "let yielded: T = y; for (const value of generator<T>(y)) { yielded = value; }",
                    _ => "",
                };
                let helpers = match role.name {
                    "destructuring-source" | "generic-field" | "generic-method" =>
                        "class G<A> { value: A; constructor(value: A) { this.value = value; } get(): A { return this.value; } }",
                    "throw-operand" | "call-return" => "function identity<A>(value: A): A { return value; }",
                    "closure-parameter" => "function closure<A>(value: A): A { const cb = (p: A): A => p; return cb(value); }",
                    "await-result" => "async function promise<A>(value: A): Promise<A> { return value; }",
                    "yield-value" => "function* generator<A>(value: A): Generator<A> { yield value; }",
                    _ => "",
                };
                let constraint_helpers = if matches!(
                    site.name.as_str(),
                    "forward-argument" | "class-field-type"
                ) {
                    format!("function forward<A{constraint}>(a: A): A {{ return a; }} class Constrained<A{constraint}> {{ value: A; constructor(value: A) {{ this.value = value; }} }}")
                } else {
                    String::new()
                };
                let mut declaration = format!(
                    "class Box {{ v: i32 = 1; get(): i32 {{ return this.v; }} }} \
                     {helpers} function take<A>(a: A): void {{}} \
                     {constraint_helpers} \
                     {asynchronous_word}function g<{parameters}>(x: {x_type}, y: T, u: U, n: i32, \
                     s: string, numbers: i32[]): {result} {{ {setup} {body} }}",
                );
                declaration.push_str(kinds::prelude(kind));
                if site.name == "field-initializer" {
                    let value = match role.name {
                        "destructuring-source" => "new G<T>(y).value".to_string(),
                        "array-element" => "[y][0]".to_string(),
                        "fixed-array-element" => "this.fixed()[0]".to_string(),
                        "generic-field" => "new G<T>(y).value".to_string(),
                        "generic-method" => "new G<T>(y).get()".to_string(),
                        "yield-value" => "this.yielded()".to_string(),
                        _ => role.expression.to_string(),
                    };
                    let value = field_value(&value);
                    let fields = format!(
                        "class Field<{parameters}> {{ x: {x_type}; y: T; n: i32; \
                         value: T = {value}; \
                         constructor(x: {x_type}, y: T, n: i32) {{ this.x = x; this.y = y; this.n = n; }} \
                         read_x(): {x_type} {{ return this.x; }} read_y(): T {{ return this.y; }} \
                         read_n(): i32 {{ return this.n; }} {} }}",
                        match role.name {
                            "fixed-array-element" => "fixed(): FixedArray<T, 1> { return [this.read_y()]; }",
                            "yield-value" => "yielded(): T { let value: T = this.read_y(); for (const item of generator<T>(this.read_y())) { value = item; } return value; }",
                            _ => "",
                        },
                    );
                    declaration.push_str(&fields);
                }
                let name = format!("product-{}-{}-{}", kind.name, role.name, site.name);
                let divergence = if site.name == "field-initializer" {
                    Some(Divergence {
                        code: RuleCode::S100,
                        record: "C9",
                        message: "§147 rule 2:",
                        token: "a method call, a getter or setter, a write, `this` as a value, and",
                    })
                } else if site.name == "throw" {
                    Some(Divergence {
                        code: RuleCode::S010,
                        record: "C6",
                        message: "`throw` requires an Error-family object",
                        token: "`throw` of a non-Error value is rejected",
                    })
                } else if site.name == "condition"
                    && (role.name == "concrete" || kind.name.starts_with("function-"))
                {
                    Some(Divergence {
                        code: RuleCode::S100,
                        record: "C24",
                        message: "condition must be boolean",
                        token: "The language has no implicit conversion (C3).",
                    })
                } else {
                    site.divergence
                };
                let candidates: &[(&str, &str)] = if constraint.is_empty() {
                    &[
                        ("i32", "3"),
                        ("boolean", "true"),
                        ("string", "\"s\""),
                        ("Box", "new Box()"),
                        ("i32[]", "[3]"),
                    ]
                } else if role.order.is_some() && kind.constraint.is_empty() {
                    &[("Box", "new Box()")]
                } else {
                    &[(kind.argument_type, kind.argument)]
                };
                let admission_cell = Cell {
                    name: format!("{name}-instance"),
                    source: declaration.clone(),
                    divergence,
                    concrete_source: None,
                };
                let records = kinds::records(&admission_cell);
                let mut admitted = None;
                let mut reasons = Vec::new();
                for (ty, value) in candidates {
                    let field_instance = if site.name == "field-initializer" {
                        format!(" new Field<{ty}, {ty}>({value}, {value}, 1);")
                    } else {
                        String::new()
                    };
                    let main = format!(
                        "{}g<{ty}, {ty}>({value}, {value}, {value}, 1, \"s\", [1]);{field_instance}",
                        if asynchronous { "await " } else { "" }
                    );
                    let concrete = replace_parameter_names(
                        &declaration
                            .replace(&format!("g<{parameters}>"), "g")
                            .replace(&format!("Field<{parameters}>"), "Field"),
                        ty,
                    );
                    let concrete_source = format!(
                        "{concrete}\nexport {asynchronous_word}function main(): {} {{ {} }}",
                        if asynchronous {
                            "Promise<void>"
                        } else {
                            "void"
                        },
                        main.replace(&format!("g<{ty}, {ty}>"), "g")
                            .replace(&format!("Field<{ty}, {ty}>"), "Field"),
                    );
                    let errors = controls.entry(concrete_source.clone()).or_insert_with(|| {
                        check_program(&[SourceFile::new("control.ts", &concrete_source)])
                            .err()
                            .unwrap_or_default()
                    });
                    // A callable constraint cannot admit a boolean condition (§143 rule 4).
                    if errors.is_empty()
                        || (!errors.iter().any(|error| {
                            error.divergence
                                == Some(subscript_compiler::divergence::Divergence::VoidValue)
                        }) && !(kind.name.starts_with("function-") && site.name == "condition")
                            && errors.iter().all(|error| {
                                records
                                    .iter()
                                    .any(|record| diagnostic_matches(error, record))
                            }))
                    {
                        admitted = Some((main, concrete_source));
                        break;
                    }
                    reasons.push(format!(
                        "`{ty}`: {}",
                        errors
                            .iter()
                            .map(|error| error.message.replace('|', "\\|"))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
                for instance in [false, true] {
                    // §143 rule 4 requires an accepted concrete body form.
                    if instance && admitted.is_none() {
                        omitted.push(format!("| `{name}-instance` | {} |", reasons.join("; ")));
                        continue;
                    }
                    let mut cell = build_cell(CellInput {
                        name: &name,
                        declaration: &declaration,
                        main_body: if instance {
                            &admitted.as_ref().unwrap().0
                        } else {
                            ""
                        },
                        instance,
                        // The builder checks each concrete control once above.
                        concrete_source: None,
                        divergence,
                    });
                    if asynchronous {
                        cell.source = cell.source.replace(
                            "export function main(): void",
                            "export async function main(): Promise<void>",
                        );
                    }
                    cells.push(cell);
                }
            }
        }
    }
    // C21 excludes void controls; C24 row 1 records truth controls.
    // The split rejection classes omit concrete forms outside their recorded restrictions.
    assert_eq!(omitted.len(), 7740, "the omitted instance set changed");
    let additional_pairs: usize = kinds::additional()
        .iter()
        .map(|kind| 2 + usize::from(kinds::numeric(kind)))
        .sum();
    let current_kinds = KINDS.len() + FUNCTION_KINDS.len();
    eprintln!(
        "product: {} current kinds × {} roles; {} additional kinds × 2 base roles plus {} numeric roles; {} sites; {} candidates; {} cells; {} omitted instances",
        current_kinds, ROLES.len(), kinds::additional().len(),
        additional_pairs - kinds::additional().len() * 2, sites.len(),
        (ROLES.len() * current_kinds + additional_pairs) * sites.len() * 2,
        cells.len(), omitted.len()
    );
    // The measurement destination is optional and does not change the gate's cell set.
    if let Some(path) = std::env::var_os("SUBSCRIPT_MATRIX_OMISSIONS") {
        fs::write(path, omitted.join("\n")).unwrap();
    }
    cells
}

// §143 rule 4 covers every surface compound operator on one parameter pair per kind.
// Measured cost: 396 cells in 0.376 seconds, with one TypeScript process.
pub(super) fn compound_cells() -> Vec<Cell> {
    let mut result = Vec::new();
    for kind in &kinds::all() {
        for (name, operator) in [
            ("add", "+="),
            ("sub", "-="),
            ("mul", "*="),
            ("div", "/="),
            ("mod", "%="),
            ("and", "&="),
            ("or", "|="),
            ("xor", "^="),
            ("shl", "<<="),
            ("shr", ">>="),
            ("ushr", ">>>="),
        ] {
            let constraint = kind.constraint;
            let declaration = format!("{} class Box {{ v: i32 = 1; }} function g<T{constraint}>(x: T, y: T): void {{ y {operator} x; }}", kinds::prelude(kind));
            result.push(build_cell(CellInput {
                name: &format!("compound-{}-{name}", kind.name),
                declaration: &declaration,
                main_body: "",
                instance: false,
                concrete_source: None,
                divergence: None,
            }));
        }
    }
    result
}
