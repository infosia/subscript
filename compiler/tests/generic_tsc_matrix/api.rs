//! Ambient callable positions of §143 rule 4.

use super::*;

struct Api {
    name: String,
    owner: String,
    static_member: bool,
    outer: Vec<(String, String)>,
    generic: Vec<(String, String)>,
    arguments: Vec<String>,
}

fn parameters(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (name, constraint) = part.split_once('~').unwrap();
            (name.into(), constraint.into())
        })
        .collect()
}

/// Replaces type-parameter words without changes to constructor names.
pub(super) fn substitute(text: &str, replacements: &[(String, String)]) -> String {
    let mut output = String::new();
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            output.push_str(
                replacements
                    .iter()
                    .find(|(name, _)| name == &word)
                    .map_or(word.as_str(), |(_, value)| value.as_str()),
            );
            word.clear();
            output.push(c);
        }
    }
    output.trim().to_string()
}

fn default_type(name: &str, constraint: &str) -> String {
    if constraint == "object" {
        "Box".into()
    } else if name == "N" {
        "2".into()
    } else {
        "i32".into()
    }
}

// §143 rule 4: every unexpressed prelude declaration needs an explicit reason.
const OMITTED: &[&str] = &[
    "Descriptor",
    "FixedArray.index",
    "FixedArray.length",
    "FixedArray.[Symbol.iterator]",
    "Inbox.constructor",
    "Outbox.constructor",
    "Worker.constructor",
    "ValueType",
];

fn omitted_names(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("omit\t"))
        .map(|line| line.split_once('\t').unwrap().0)
        .collect()
}

fn unlisted_omissions(text: &str) -> Vec<&str> {
    omitted_names(text)
        .into_iter()
        .filter(|name| !OMITTED.contains(name))
        .collect()
}

/// Derives each ambient callable position from the prelude (§143 rule 4).
pub(super) fn cells() -> Vec<Cell> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new("node")
        .arg(root.join("compiler/tests/generic_tsc_matrix/api.cjs"))
        .arg(root.join("prelude/lang.d.ts"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        unlisted_omissions(&text).is_empty(),
        "unlisted ambient declarations: {:?}",
        unlisted_omissions(&text)
    );
    assert_eq!(omitted_names(&text), OMITTED);
    let mut apis = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] == "omit" {
            assert!(!fields[2].is_empty());
            eprintln!("ambient API omitted: {}: {}", fields[1], fields[2]);
            continue;
        }
        apis.push(Api {
            name: fields[1].into(),
            owner: fields[2].into(),
            static_member: fields[3] == "static",
            outer: parameters(fields[4]),
            generic: parameters(fields[5]),
            arguments: fields[6..]
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| (*s).into())
                .collect(),
        });
    }
    if let Some(path) = std::env::var_os("SUBSCRIPT_API_SITES") {
        fs::write(path, &text).unwrap();
    }
    let site_count: usize = apis
        .iter()
        .map(|api| (api.arguments.len() + api.generic.len() + api.outer.len()).max(1))
        .sum();
    let mut cells = Vec::new();
    let mut omissions = Vec::new();
    let mut controls = std::collections::HashMap::new();
    for api in &apis {
        let defaults: Vec<_> = api
            .outer
            .iter()
            .map(|(n, c)| (n.clone(), default_type(n, c)))
            .collect();
        for position in 0..(api.arguments.len() + api.generic.len() + api.outer.len()).max(1) {
            for (role, expression, role_type) in [
                ("value", "x", "T"),
                ("array", "xs", "T[]"),
                ("nullable", "nullable", "T | null"),
            ] {
                for kind in &kinds::all() {
                    let mut replacements: Vec<_> = api
                        .generic
                        .iter()
                        .map(|(n, c)| (n.clone(), default_type(n, c)))
                        .collect();
                    if api.name.starts_with("Context.bytes") || api.name == "Context.fromBytes" {
                        replacements[0].1 = "FixedArray<u8, 2>".into();
                    }
                    let mut outer = defaults.clone();
                    if position >= api.arguments.len()
                        && position < api.arguments.len() + api.generic.len()
                    {
                        replacements[position - api.arguments.len()].1 = role_type.into();
                    } else if position >= api.arguments.len() + api.generic.len()
                        && !outer.is_empty()
                    {
                        outer[position - api.arguments.len() - api.generic.len()].1 =
                            role_type.into();
                    }
                    let all: Vec<_> = outer.iter().chain(&replacements).cloned().collect();
                    let baseline: Vec<_> = api
                        .arguments
                        .iter()
                        .map(|arg| substitute(arg, &all))
                        .collect();
                    let mut args: Vec<_> = (0..baseline.len()).map(|i| format!("p{i}")).collect();
                    if position < args.len() {
                        args[position] = expression.into();
                    }
                    let member = api
                        .name
                        .split_once('.')
                        .map_or(api.name.as_str(), |(_, name)| name);
                    let receiver_type = if outer.is_empty() {
                        if api.owner == "String" {
                            "string".into()
                        } else {
                            api.owner.clone()
                        }
                    } else {
                        format!(
                            "{}<{}>",
                            api.owner,
                            outer
                                .iter()
                                .map(|(_, t)| t.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let (callee, receiver) = if api.owner.is_empty() {
                        (api.name.clone(), String::new())
                    } else if matches!(
                        api.owner.as_str(),
                        "Context" | "JSON" | "Math" | "MapConstructor"
                    ) || api.static_member
                    {
                        (
                            format!("{}.{member}", api.owner.trim_end_matches("Constructor")),
                            String::new(),
                        )
                    } else {
                        (
                            format!("receiver.{member}"),
                            format!(", receiver: {receiver_type}"),
                        )
                    };
                    let type_arguments = if replacements.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            replacements
                                .iter()
                                .map(|(_, t)| t.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let extra: String = baseline
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != position)
                        .map(|(i, t)| format!(", p{i}: {t}"))
                        .collect();
                    let body = format!("{callee}{type_arguments}({});", args.join(", "));
                    let body = if api.name == "Context.suspend" {
                        format!("await {body}")
                    } else {
                        body
                    };
                    let async_word = if api.name == "Context.suspend" {
                        "async "
                    } else {
                        ""
                    };
                    let result_type = if api.name == "Context.suspend" {
                        "Promise<void>"
                    } else {
                        "void"
                    };
                    let declaration = format!("class Box {{ v: i32 = 1; }} @ValueType class Value {{ v: i32 = 1; }} {async_word}function g<T{}>({expression}: {role_type}{receiver}{extra}): {result_type} {{ {body} }}", kind.constraint);
                    let declaration = format!(
                        "{} {declaration}",
                        if kind.name == "value-class" {
                            ""
                        } else {
                            kinds::prelude(kind)
                        }
                    );
                    let supplied: String = baseline
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != position)
                        .map(|(i, _)| format!(", p{i}"))
                        .collect();
                    let supplied = format!(
                        "{}{supplied}",
                        if receiver.is_empty() {
                            ""
                        } else {
                            ", receiver"
                        }
                    );
                    let name = format!(
                        "api-{}-{position}-{role}-{}",
                        api.name.replace('.', "-"),
                        kind.name
                    );
                    let divergence = if api.owner == "FixedArray"
                        && position >= api.arguments.len() + api.generic.len()
                        && api.outer[position - api.arguments.len() - api.generic.len()].0 == "N"
                        && role == "value"
                        && kinds::numeric(kind)
                    {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "Q3",
                            message: "non-negative integer literal",
                            token: "non-negative integer literal",
                        })
                    } else if api.name == "FixedArray.map"
                        && position == 1
                        && role == "value"
                        && kind.name == "void"
                    {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "C21",
                            message: "the `map` callback must return a value",
                            token: "`map` callback that returns `void`",
                        })
                    } else if api.name == "Worker.spawn" {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "compiler.md §40",
                            message:
                                "`Worker.spawn` entry must name a module-level function directly",
                            token: "`entry` is a directly named module-level",
                        })
                    } else if matches!(api.owner.as_str(), "Worker" | "Inbox" | "Outbox") {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "compiler.md §40",
                            message: "worker message type",
                            token: "Message classes are transferable per stdlib §16.2",
                        })
                    } else if matches!(api.name.as_str(), "Context.bytesOf" | "Context.bytesInto")
                        && position == 0
                        && (role == "array" || (role == "value" && kind.name == "array"))
                    {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "stdlib.md §18.1",
                            message: if api.name == "Context.bytesOf" {
                                "type mismatch: `Context.bytesOf` argument expects"
                            } else {
                                "type mismatch: `Context.bytesInto` argument expects"
                            },
                            token: "The argument `value` has type `T` (nominal equality)",
                        })
                    } else if api.name == "Context.free" && (role == "array" || role == "value") {
                        Some(Divergence {
                            code: RuleCode::S100,
                            record: "corpus.md §5 Q6",
                            message: "type mismatch: the argument expects `object`",
                            token: "reference-class instance immediately",
                        })
                    } else if api.owner == "FixedArray"
                        && position == 0
                        && role == "value"
                        && kind.name == "function-return"
                    {
                        Some(Divergence {
                            code: RuleCode::S014,
                            record: "stdlib.md §9",
                            message: "callbacks take",
                            token: "two arities accepted since Q27",
                        })
                    } else if api.name == "MapConstructor.groupBy"
                        && position == 1
                        && role == "value"
                        && kind.name == "function-return"
                    {
                        Some(Divergence {
                            code: RuleCode::S014,
                            record: "stdlib.md §10",
                            message: "callbacks take",
                            token: "as Q22 fixes callback arities",
                        })
                    } else {
                        None
                    };
                    cells.push(Cell {
                        name: format!("{name}-no-instance"),
                        source: format!("{declaration} export function main(): void {{}}"),
                        divergence,
                        concrete_source: None,
                    });
                    let mut admitted = None;
                    let mut rejections = Vec::new();
                    for (argument_type, argument) in candidates(kind) {
                        let replacement = if argument_type.contains("=>") {
                            format!("({argument_type})")
                        } else {
                            argument_type.to_string()
                        };
                        let concrete_extra =
                            replace_parameter_names(&format!("{receiver}{extra}"), &replacement);
                        let value = if role == "array" {
                            format!("[{argument}]")
                        } else {
                            argument.to_string()
                        };
                        let main = format!("function admitted({}): void {{ g<{argument_type}>({value}{supplied}); }}", concrete_extra.trim().trim_start_matches(',').trim());
                        let source =
                            format!("{declaration} {main} export function main(): void {{}}");
                        let concrete = replace_parameter_names(
                            &source
                                .replace(&format!("<T{}>", kind.constraint), "")
                                .replace(&format!("g<{argument_type}>"), "g"),
                            &replacement,
                        );
                        let errors = controls.entry(concrete.clone()).or_insert_with(|| {
                            check_program(&[SourceFile::new("control.ts", concrete.clone())])
                                .err()
                                .unwrap_or_default()
                        });
                        if let Some(directory) = std::env::var_os("SUBSCRIPT_API_CONTROLS") {
                            let directory = PathBuf::from(directory);
                            fs::create_dir_all(&directory).unwrap();
                            let suffix = candidates(kind)
                                .iter()
                                .position(|(ty, _)| *ty == argument_type)
                                .unwrap();
                            let path = directory.join(format!("{name}-control-{suffix}.ts"));
                            fs::write(&path, &concrete).unwrap();
                            let verdict = errors
                                .iter()
                                .map(|d| format!("{}: {}", d.code, d.message))
                                .collect::<Vec<_>>()
                                .join("; ");
                            fs::write(
                                path.with_extension("verdict"),
                                if verdict.is_empty() {
                                    "accept"
                                } else {
                                    &verdict
                                },
                            )
                            .unwrap();
                        }
                        for error in errors.iter() {
                            assert!(
                                !error.message.contains("TypeParameter(")
                                    && !error.message.contains("ClassId("),
                                "{}",
                                error.message
                            );
                        }
                        if errors.is_empty() {
                            admitted = Some(Cell {
                                name: format!("{name}-instance"),
                                source,
                                divergence,
                                // The admission check above checks this concrete source once.
                                concrete_source: None,
                            });
                            break;
                        }
                        rejections.push(format!(
                            "{argument_type}: {}",
                            errors
                                .iter()
                                .map(|d| d.message.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        ));
                    }
                    if let Some(cell) = admitted {
                        cells.push(cell);
                    } else {
                        omissions.push(format!("{name}: {}", rejections.join("; ")));
                    }
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SUBSCRIPT_API_OMISSIONS") {
        fs::write(path, omissions.join("\n")).unwrap();
    }
    assert_eq!(
        omissions.len(),
        8775,
        "ambient instance admission changed; inspect SUBSCRIPT_API_OMISSIONS"
    );
    eprintln!(
        "ambient API: {} callables, {site_count} sites, {} cells, {} omitted instances",
        apis.len(),
        cells.len(),
        omissions.len()
    );
    cells
}

fn candidates(kind: &Kind) -> Vec<(&str, &str)> {
    let mut values = vec![(kind.argument_type, kind.argument)];
    match kind.name {
        "plain" => values.extend([
            ("u8", "1 as u8"),
            ("string", "\"a\""),
            ("Box", "new Box()"),
            ("Value", "new Value()"),
            ("i32[]", "[1]"),
            ("u8[]", "[1 as u8]"),
            ("FixedArray<u8, 2>", "[1 as u8, 2 as u8]"),
            (
                "(value: i32, index: i32) => i32",
                "(value: i32, index: i32): i32 => value",
            ),
        ]),
        "numeric" | "f64" | "u8" => values.push(("u8", "1 as u8")),
        "array" => values.push(("u8[]", "[1 as u8]")),
        _ => {}
    }
    values
}

#[test]
fn new_callables_and_unexpressed_signatures_are_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let project =
        Project(std::env::temp_dir().join(format!("subscript-api-parser-{}", std::process::id())));
    fs::create_dir_all(&project.0).unwrap();
    let source = project.0.join("ambient.d.ts");
    fs::write(&source, "declare namespace Added { function call<T>(x: T, n: i32): void; function branch<T>(x: T extends string ? string : i32): void; function overloaded(x: i32): void; function overloaded(x: string): void; } declare class AddedClass { constructor(x: i32); callback: (x: i32) => void; }").unwrap();
    let result = Command::new("node")
        .arg(root.join("compiler/tests/generic_tsc_matrix/api.cjs"))
        .arg(source)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(
        text.contains("call\tAdded.call\tAdded\t\t\tT~\tT\ti32"),
        "{text}"
    );
    assert!(
        text.contains("omit\tAdded.branch\tThe conditional signature"),
        "{text}"
    );
    assert!(
        text.contains("omit\tAdded.overloaded\tThe overload set"),
        "{text}"
    );
    assert!(
        text.contains("omit\tAddedClass.constructor\tThe constructor"),
        "{text}"
    );
    assert!(
        text.contains("omit\tAddedClass.callback\tThe property"),
        "{text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("call\t"))
            .count(),
        1
    );
}

#[test]
fn a_new_unexpressed_declaration_requires_a_list_entry() {
    for name in [
        "New.overload",
        "New.variable",
        "New.property",
        "New.conditional",
    ] {
        let output = format!("omit\t{name}\tA signature-specific form is required.\n");
        assert_eq!(unlisted_omissions(&output), [name]);
    }
}
