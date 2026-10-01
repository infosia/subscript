//! Comparisons whose bindings have visible Type annotations (§143 rule 1a).

use super::*;

#[derive(Clone, Copy)]
struct Binding {
    collection: bool,
    routed: bool,
}

fn annotation(nodes: &[Token]) -> Option<bool> {
    nodes
        .iter()
        .any(|t| t.text == "Type")
        .then(|| {
            nodes
                .iter()
                .any(|t| t.text == "Vec" || t.text.starts_with('['))
        })
        .or_else(|| {
            nodes.iter().find_map(|t| {
                t.text
                    .starts_with('[')
                    .then(|| annotation(&t.children).map(|_| true))
                    .flatten()
            })
        })
}

/// Reports Type comparisons and searches with visible binding types (§143 rule 1a).
pub(super) fn scan(source: &str) -> Vec<Site> {
    fn walk(
        nodes: &[Token],
        source: &str,
        function: &str,
        inherited: &HashMap<String, Binding>,
        out: &mut Vec<Site>,
    ) {
        let mut bindings = inherited.clone();
        let mut function = function.to_string();
        for (i, token) in nodes.iter().enumerate() {
            if token.text == "fn" {
                function = nodes[i + 1].text.clone();
                bindings.clear();
                if let Some(params) = nodes[i + 2..].iter().find(|t| t.text.starts_with('(')) {
                    for part in params.children.split(|t| t.text == ",") {
                        if let Some(colon) = part.iter().position(|t| t.text == ":") {
                            if let Some(collection) = annotation(&part[colon + 1..]) {
                                bindings.insert(
                                    part[colon - 1].text.clone(),
                                    Binding {
                                        collection,
                                        routed: false,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            if token.text == "let" {
                let end = nodes[i + 1..]
                    .iter()
                    .position(|t| t.text == ";")
                    .map_or(nodes.len(), |n| i + 1 + n);
                let part = &nodes[i + 1..end];
                if let Some(equal) = part.iter().position(|t| t.text == "=") {
                    let name = if part.first().is_some_and(|t| t.text == "mut") {
                        1
                    } else {
                        0
                    };
                    if let Some(binding_name) = part.get(name) {
                        let declared = part[..equal]
                            .iter()
                            .position(|t| t.text == ":")
                            .and_then(|colon| annotation(&part[colon + 1..equal]));
                        let value = &part[equal + 1..];
                        let inferred = value.iter().find_map(|t| bindings.get(&t.text).copied());
                        let apparent = value.iter().any(|t| t.text == "apparent_type");
                        if declared.is_some() || inferred.is_some() || apparent {
                            bindings.insert(
                                binding_name.text.clone(),
                                Binding {
                                    collection: declared
                                        .unwrap_or_else(|| inferred.is_some_and(|b| b.collection)),
                                    routed: apparent || inferred.is_some_and(|b| b.routed),
                                },
                            );
                        }
                    }
                }
            }
            if matches!(token.text.as_str(), "=" | "!")
                && nodes.get(i + 1).is_some_and(|t| t.text == "=")
                && i > 0
            {
                let left = &nodes[i - 1];
                let Some(right) = nodes.get(i + 2) else {
                    continue;
                };
                if let (Some(a), Some(b)) = (bindings.get(&left.text), bindings.get(&right.text)) {
                    if !a.collection && !b.collection {
                        out.push(Site {
                            function: function.clone(),
                            expression: format!("{}{}={}", left.text, token.text, right.text),
                            pattern: "Type comparison".into(),
                            line: source[..token.start].lines().count(),
                            routed: a.routed && b.routed,
                        });
                    }
                }
            }
            if matches!(token.text.as_str(), "position" | "contains")
                && i >= 2
                && nodes[i - 1].text == "."
            {
                let mut begin = i - 2;
                loop {
                    if nodes[begin].text.starts_with('(') && begin > 0 {
                        begin -= 1;
                    } else if begin >= 2 && nodes[begin - 1].text == "." {
                        begin -= 2;
                    } else {
                        break;
                    }
                }
                let Some(receiver) = bindings
                    .get(&nodes[begin].text)
                    .copied()
                    .filter(|b| b.collection)
                else {
                    continue;
                };
                let Some(arguments) = nodes.get(i + 1).filter(|t| t.text.starts_with('(')) else {
                    continue;
                };
                let expression = &source[nodes[begin].start..arguments.end];
                let input_routed = if token.text == "contains" {
                    arguments
                        .children
                        .iter()
                        .find_map(|t| bindings.get(&t.text))
                        .is_some_and(|b| b.routed)
                        || expression.contains("apparent_type(")
                } else {
                    expression.contains("apparent_type(")
                };
                out.push(Site {
                    function: function.clone(),
                    expression: compact(expression),
                    pattern: format!("Type collection .{}", token.text),
                    line: source[..token.start].lines().count(),
                    routed: receiver.routed && input_routed,
                });
                if token.text == "position" {
                    let mut closure = bindings.clone();
                    if arguments.children.first().is_some_and(|t| t.text == "|") {
                        if let Some(name) = arguments.children.get(1) {
                            closure.insert(
                                name.text.clone(),
                                Binding {
                                    collection: false,
                                    routed: receiver.routed,
                                },
                            );
                        }
                    }
                    walk(&arguments.children, source, &function, &closure, out);
                    continue;
                }
            }
            if !token.children.is_empty() {
                walk(&token.children, source, &function, &bindings, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &tokens(source, &mut 0, None),
        source,
        "module",
        &HashMap::new(),
        &mut out,
    );
    out
}

#[test]
fn typed_comparisons_and_collection_searches_require_routes() {
    let raw = scan("fn f(ty: &Type, other: Type, types: &[Type]) { ty == other; other != ty; types.contains(ty); types.iter().position(|candidate| candidate == ty); }");
    assert_eq!(raw.len(), 5);
    assert!(raw.iter().all(|site| !site.routed));
    let routed = scan("fn f(ty: &Type, other: &Type) { let a = self.apparent_type(ty); let b = self.apparent_type(other); a == b; }");
    assert_eq!(routed.len(), 1);
    assert!(routed[0].routed);
    assert!(
        scan("fn f(a: i32, b: i32, values: &[i32]) { a == b; values.contains(&a); }").is_empty()
    );
}
