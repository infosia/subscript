//! The value-role and consumer-site product of §143 rule 4.

use super::*;
use std::collections::HashMap;

struct Role {
    name: &'static str,
    expression: &'static str,
    nullable: bool,
    order: Option<bool>,
}

const ROLES: &[Role] = &[
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
        for (name_op, operator) in [
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
        ("loose-equality", "==", Some(LOOSE_EQUALITY)),
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
    sites
}

pub(super) fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    let sites = sites();
    let mut omitted = Vec::new();
    let mut controls = HashMap::new();
    for kind in KINDS {
        for role in ROLES {
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
                let declaration = format!(
                    "class Box {{ v: i32 = 1; }} function take<A>(a: A): void {{}} \
                     function g<{parameters}>(x: {x_type}, y: T, u: U, n: i32, \
                     s: string, numbers: i32[]): {} {{ {body} }}",
                    site.result,
                );
                let name = format!("product-{}-{}-{}", kind.name, role.name, site.name);
                let divergence = if role.name == "concrete" && site.name == "condition" {
                    Some(Divergence {
                        code: RuleCode::S100,
                        record: "compiler.md §68",
                        token: "the condition is a `boolean` value",
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
                let mut admitted = None;
                let mut reasons = Vec::new();
                for (ty, value) in candidates {
                    let main = format!("g<{ty}, {ty}>({value}, {value}, {value}, 1, \"s\", [1]);");
                    let concrete = replace_parameter_names(
                        &declaration.replace(&format!("g<{parameters}>"), "g"),
                        ty,
                    );
                    let concrete_source = format!(
                        "{concrete}\nexport function main(): void {{ {} }}",
                        main.replace(&format!("g<{ty}, {ty}>"), "g"),
                    );
                    let errors = controls.entry(concrete_source.clone()).or_insert_with(|| {
                        check_program(&[SourceFile::new("control.ts", &concrete_source)])
                            .err()
                            .unwrap_or_default()
                    });
                    if errors.is_empty()
                        || divergence.is_some_and(|record| {
                            errors.iter().all(|error| error.code == record.code)
                        })
                    {
                        admitted = Some(main);
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
                    cells.push(build_cell(CellInput {
                        name: &name,
                        declaration: &declaration,
                        main_body: if instance {
                            admitted.as_ref().unwrap()
                        } else {
                            ""
                        },
                        instance,
                        concrete_source: None,
                        divergence,
                    }));
                }
            }
        }
    }
    eprintln!(
        "product candidates: {} roles × {} sites × {} kinds × 2 = {}; omitted instances={}",
        ROLES.len(),
        sites.len(),
        KINDS.len(),
        ROLES.len() * sites.len() * KINDS.len() * 2,
        omitted.len()
    );
    // The measurement destination is optional and does not change the gate's cell set.
    if let Some(path) = std::env::var_os("SUBSCRIPT_MATRIX_OMISSIONS") {
        fs::write(path, omitted.join("\n")).unwrap();
    }
    cells
}
