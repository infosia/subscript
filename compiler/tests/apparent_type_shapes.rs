//! §143 rule 1a: every value-shape test reads the apparent type.
//! The source scan uses Rust token groups, so comments and strings do not count.
//! It checks match arms, let patterns, matches!, variant equality, and helpers derived from Type source.
//! Measured cost: 495 sites in 0.205 seconds; 16 named groups pin 45 justified raw-test functions.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::Instant,
};

#[derive(Clone)]
struct Token {
    text: String,
    start: usize,
    end: usize,
    children: Vec<Token>,
}

fn tokens(source: &str, offset: &mut usize, close: Option<u8>) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    while *offset < bytes.len() {
        let start = *offset;
        let byte = bytes[start];
        if Some(byte) == close {
            *offset += 1;
            break;
        }
        if byte.is_ascii_whitespace() {
            *offset += 1;
            continue;
        }
        if source[start..].starts_with("//") {
            *offset += source[start..].find('\n').unwrap_or(bytes.len() - start);
            continue;
        }
        if source[start..].starts_with("/*") {
            *offset += 2;
            let mut depth = 1;
            while depth > 0 && *offset < bytes.len() {
                if source[*offset..].starts_with("/*") {
                    depth += 1;
                    *offset += 2;
                } else if source[*offset..].starts_with("*/") {
                    depth -= 1;
                    *offset += 2;
                } else {
                    *offset += source[*offset..].chars().next().unwrap().len_utf8();
                }
            }
            continue;
        }
        let raw_hashes = if byte == b'r' {
            bytes[start + 1..]
                .iter()
                .take_while(|b| **b == b'#')
                .count()
        } else {
            0
        };
        let raw = byte == b'r' && bytes.get(start + 1 + raw_hashes) == Some(&b'"');
        let character = byte == b'\''
            && (bytes.get(start + 2) == Some(&b'\'')
                || (bytes.get(start + 1) == Some(&b'\\') && bytes.get(start + 3) == Some(&b'\'')));
        if raw {
            let terminator = format!("\"{}", "#".repeat(raw_hashes));
            *offset = start + 2 + raw_hashes;
            *offset += source[*offset..].find(&terminator).unwrap() + terminator.len();
            continue;
        }
        if byte == b'"' || character {
            *offset += 1;
            while *offset < bytes.len() {
                let b = bytes[*offset];
                *offset += 1;
                if b == b'\\' {
                    *offset += 1;
                } else if b == byte {
                    break;
                }
            }
            continue;
        }
        let children = match byte {
            b'(' | b'[' | b'{' => {
                *offset += 1;
                let end = match byte {
                    b'(' => b')',
                    b'[' => b']',
                    _ => b'}',
                };
                tokens(source, offset, Some(end))
            }
            _ => {
                if byte.is_ascii_alphabetic() || byte == b'_' {
                    *offset += 1;
                    while *offset < bytes.len()
                        && (bytes[*offset].is_ascii_alphanumeric() || bytes[*offset] == b'_')
                    {
                        *offset += 1;
                    }
                } else if source[start..].starts_with("=>") || source[start..].starts_with("::") {
                    *offset += 2;
                } else {
                    *offset += source[start..].chars().next().unwrap().len_utf8();
                }
                Vec::new()
            }
        };
        result.push(Token {
            text: source[start..*offset].to_string(),
            start,
            end: *offset,
            children,
        });
    }
    result
}

fn has_type(tokens: &[Token]) -> bool {
    tokens
        .windows(2)
        .any(|pair| pair[0].text == "Type" && pair[1].text == "::")
        || tokens.iter().any(|token| has_type(&token.children))
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

struct Site {
    function: String,
    expression: String,
    pattern: String,
    line: usize,
    routed: bool,
}

fn routed(expression: &str, prefix: &str) -> bool {
    let expression = compact(expression);
    let expression = expression.trim_start_matches('&');
    if expression.starts_with('(') && expression.ends_with(')') {
        let nodes = tokens(expression, &mut 0, None);
        let contents = &nodes[0].children;
        return contents
            .split(|token| token.text == ",")
            .filter(|part| !part.is_empty())
            .all(|part| routed(&expression[part[0].start..part.last().unwrap().end], prefix));
    }
    if [
        "self.apparent_type(",
        "checker.apparent_type(",
        "apparent_type(",
        "self.apparent_expr(",
    ]
    .iter()
    .any(|call| expression.starts_with(call))
    {
        return true;
    }
    if expression.contains(".map(|ty|self.apparent_type(ty))") {
        return true;
    }
    let root = expression
        .trim_start_matches(['&', '*'])
        .split('.')
        .next()
        .unwrap();
    let prefix = compact(prefix);
    let needle = format!("let{root}=");
    let Some((before, binding)) = prefix.rsplit_once(&needle) else {
        return false;
    };
    let initializer = binding.split(';').next().unwrap();
    if initializer.starts_with('&') && !initializer.contains('{') {
        return routed(initializer, before);
    }
    (initializer.starts_with("self.apparent_type(")
        || initializer.starts_with("checker.apparent_type(")
        || initializer.starts_with("self.apparent_expr("))
        && !expression.contains("contained_types")
}

fn routed_pattern(expression: &str, pattern: &str, prefix: &str) -> bool {
    let input = tokens(expression, &mut 0, None);
    let patterns = tokens(pattern, &mut 0, None);
    if input.len() == 1 && input[0].text.starts_with('(') {
        let inputs: Vec<_> = input[0].children.split(|t| t.text == ",").collect();
        let tuples: Vec<_> = patterns
            .iter()
            .filter(|t| t.text.starts_with('('))
            .collect();
        if !tuples.is_empty() {
            return tuples.iter().all(|tuple| {
                tuple
                    .children
                    .split(|t| t.text == ",")
                    .enumerate()
                    .all(|(index, part)| {
                        if !has_type(part) {
                            return true;
                        }
                        let Some(input) = inputs.get(index).filter(|part| !part.is_empty()) else {
                            return false;
                        };
                        routed(
                            &expression[input[0].start..input.last().unwrap().end],
                            prefix,
                        )
                    })
            });
        }
    }
    routed(expression, prefix)
}

#[derive(Default)]
struct ShapeHelpers {
    methods: HashSet<String>,
    functions: HashMap<String, Vec<usize>>,
}

fn shape_helpers(source: &str) -> ShapeHelpers {
    fn tests_input(nodes: &[Token], input: &str) -> bool {
        nodes.iter().enumerate().any(|(index, token)| {
            (token.text == "match" && nodes.get(index + 1).is_some_and(|t| t.text == input))
                || (token.text == "matches"
                    && nodes.get(index + 1).is_some_and(|t| t.text == "!")
                    && nodes.get(index + 2).is_some_and(|t| {
                        t.children
                            .iter()
                            .find(|t| t.text != "&" && t.text != "*")
                            .is_some_and(|t| t.text == input)
                    }))
                || tests_input(&token.children, input)
        })
    }
    fn collect(nodes: &[Token], in_type: bool, helpers: &mut ShapeHelpers) {
        for (index, token) in nodes.iter().enumerate() {
            if token.text == "impl" && nodes.get(index + 1).is_some_and(|t| t.text == "Type") {
                if let Some(body) = nodes[index + 2..].iter().find(|t| t.text.starts_with('{')) {
                    collect(&body.children, true, helpers);
                }
            }
            if token.text != "fn" {
                continue;
            }
            let name = &nodes[index + 1].text;
            let Some(params) = nodes[index + 2..].iter().find(|t| t.text.starts_with('(')) else {
                continue;
            };
            let Some(body) = nodes[index + 2..].iter().find(|t| t.text.starts_with('{')) else {
                continue;
            };
            if in_type && tests_input(&body.children, "self") {
                helpers.methods.insert(name.clone());
            } else if !in_type {
                for (position, parameter) in params.children.split(|t| t.text == ",").enumerate() {
                    if parameter
                        .iter()
                        .position(|t| t.text == "&")
                        .is_some_and(|reference| {
                            let mut target = reference + 1;
                            if parameter.get(target).is_some_and(|t| t.text == "'") {
                                target += 2;
                            }
                            parameter.get(target).is_some_and(|t| t.text == "Type")
                        })
                        && parameter
                            .first()
                            .is_some_and(|t| tests_input(&body.children, &t.text))
                    {
                        helpers
                            .functions
                            .entry(name.clone())
                            .or_default()
                            .push(position);
                    }
                }
            }
        }
    }
    let mut helpers = ShapeHelpers::default();
    collect(&tokens(source, &mut 0, None), false, &mut helpers);
    helpers
}

fn scan(source: &str) -> Vec<Site> {
    let types =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types.rs")).unwrap();
    scan_with_helpers(source, &shape_helpers(&types))
}

fn scan_with_helpers(source: &str, helpers: &ShapeHelpers) -> Vec<Site> {
    fn walk(
        nodes: &[Token],
        source: &str,
        function: &str,
        bindings: &HashMap<String, bool>,
        out: &mut Vec<Site>,
        helpers: &ShapeHelpers,
    ) {
        let mut function = function.to_string();
        let mut bindings = bindings.clone();
        for (index, token) in nodes.iter().enumerate() {
            if token.text == "fn" {
                function = nodes[index + 1].text.clone();
                bindings.clear();
            }
            let prefix = bindings
                .iter()
                .filter(|(_, route)| **route)
                .map(|(name, _)| format!("let{name}=self.apparent_type(dummy);"))
                .collect::<String>();
            if token.text == "let" && nodes.get(index + 2).is_some_and(|t| t.text == "=") {
                let end = nodes[index + 3..]
                    .iter()
                    .position(|t| t.text == ";")
                    .map_or(nodes.len(), |end| index + 3 + end);
                if end > index + 3 {
                    let expression = &source[nodes[index + 3].start..nodes[end - 1].end];
                    bindings.insert(nodes[index + 1].text.clone(), routed(expression, &prefix));
                }
            }
            let mut add = |input: &[Token], pattern: &[Token]| {
                if input.is_empty() || !has_type(pattern) {
                    return;
                }
                let expression = &source[input[0].start..input.last().unwrap().end];
                let pattern = &source[pattern[0].start..pattern.last().unwrap().end];
                out.push(Site {
                    function: function.clone(),
                    expression: compact(expression),
                    pattern: compact(pattern),
                    line: source[..token.start].lines().count(),
                    routed: routed_pattern(expression, pattern, &prefix),
                });
            };
            if token.text == "matches" && nodes.get(index + 1).is_some_and(|t| t.text == "!") {
                let arguments = &nodes[index + 2].children;
                let comma = arguments.iter().position(|t| t.text == ",").unwrap();
                add(&arguments[..comma], &arguments[comma + 1..]);
            }
            if token.text == "let" {
                if let Some(equal) = nodes[index + 1..].iter().position(|t| t.text == "=") {
                    let equal = index + 1 + equal;
                    let pattern = &nodes[index + 1..equal];
                    if has_type(pattern) {
                        let end = nodes[equal + 1..]
                            .iter()
                            .position(|t| {
                                matches!(t.text.as_str(), ";" | "else") || t.text.starts_with('{')
                            })
                            .map_or(nodes.len(), |end| equal + 1 + end);
                        add(&nodes[equal + 1..end], pattern);
                    }
                }
            }
            if token.text == "match" {
                if let Some(body) = nodes[index + 1..]
                    .iter()
                    .position(|t| t.text.starts_with('{'))
                {
                    let body = index + 1 + body;
                    let arms = &nodes[body].children;
                    let mut start = 0;
                    let mut in_body = false;
                    for (arm_index, arm) in arms.iter().enumerate() {
                        if arm.text == "=>" {
                            add(&nodes[index + 1..body], &arms[start..arm_index]);
                            in_body = true;
                        } else if arm.text == "," || (in_body && arm.text.starts_with('{')) {
                            start = arm_index + 1;
                            in_body = false;
                        }
                    }
                }
            }
            if token.text == "Type"
                && index >= 3
                && nodes[index - 1].text == "="
                && matches!(nodes[index - 2].text.as_str(), "=" | "!")
                && nodes.get(index + 1).is_some_and(|t| t.text == "::")
            {
                let end = index - 3;
                let mut begin = end;
                loop {
                    if nodes[begin].text.starts_with('(')
                        && begin > 0
                        && nodes[begin - 1]
                            .text
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_')
                    {
                        begin -= 1;
                    } else if begin >= 2 && nodes[begin - 1].text == "." {
                        begin -= 2;
                    } else if begin > 0
                        && (nodes[begin - 1].text == "*"
                            || (nodes[begin - 1].text == "&"
                                && (begin < 2 || nodes[begin - 2].text != "&")))
                    {
                        begin -= 1;
                    } else {
                        break;
                    }
                }
                let expression = &source[nodes[begin].start..nodes[end].end];
                out.push(Site {
                    function: function.clone(),
                    expression: compact(expression),
                    pattern: format!("{}=Type::{}", nodes[index - 2].text, nodes[index + 2].text),
                    line: source[..token.start].lines().count(),
                    routed: routed(expression, &prefix),
                });
            }
            if token.text == "Type" && nodes.get(index + 1).is_some_and(|t| t.text == "::") {
                let mut operator = index + 3;
                if nodes.get(operator).is_some_and(|t| t.text.starts_with('(')) {
                    operator += 1;
                }
                if nodes
                    .get(operator)
                    .is_some_and(|t| matches!(t.text.as_str(), "=" | "!"))
                    && nodes.get(operator + 1).is_some_and(|t| t.text == "=")
                {
                    let begin = operator + 2;
                    if begin < nodes.len() {
                        let mut end = begin;
                        while nodes
                            .get(end + 1)
                            .is_some_and(|t| t.text == "." || t.text.starts_with('('))
                        {
                            end += if nodes[end + 1].text == "." { 2 } else { 1 };
                        }
                        if end < nodes.len() {
                            let expression = &source[nodes[begin].start..nodes[end].end];
                            out.push(Site {
                                function: function.clone(),
                                expression: compact(expression),
                                pattern: format!(
                                    "Type::{}{}=",
                                    nodes[index + 2].text,
                                    nodes[operator].text
                                ),
                                line: source[..token.start].lines().count(),
                                routed: routed(expression, &prefix),
                            });
                        }
                    }
                }
            }
            // Methods that test a Type shape also require the resolver.
            if helpers.methods.contains(&token.text) && index >= 2 && nodes[index - 1].text == "." {
                let mut begin = index - 2;
                loop {
                    if nodes[begin].text.starts_with('(')
                        && begin > 0
                        && nodes[begin - 1]
                            .text
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_')
                    {
                        begin -= 1;
                    } else if begin >= 2 && nodes[begin - 1].text == "." {
                        begin -= 2;
                    } else {
                        break;
                    }
                }
                let expression = &source[nodes[begin].start..nodes[index - 2].end];
                out.push(Site {
                    function: function.clone(),
                    expression: compact(expression),
                    pattern: token.text.clone(),
                    line: source[..token.start].lines().count(),
                    routed: routed(expression, &prefix),
                });
            }
            if helpers.functions.contains_key(&token.text)
                && nodes
                    .get(index + 1)
                    .is_some_and(|t| t.text.starts_with('('))
                && (index == 0 || nodes[index - 1].text != "fn")
            {
                let arguments = &nodes[index + 1].children;
                for &position in &helpers.functions[&token.text] {
                    let input = arguments
                        .split(|t| t.text == ",")
                        .nth(position)
                        .unwrap_or(&[]);
                    if let (Some(first), Some(last)) = (input.first(), input.last()) {
                        let expression = &source[first.start..last.end];
                        out.push(Site {
                            function: function.clone(),
                            expression: compact(expression),
                            pattern: token.text.clone(),
                            line: source[..token.start].lines().count(),
                            routed: routed(expression, &prefix),
                        });
                    }
                }
            }
            if !token.children.is_empty() {
                walk(&token.children, source, &function, &bindings, out, helpers);
            }
        }
    }
    let nodes = tokens(source, &mut 0, None);
    let mut result = Vec::new();
    walk(
        &nodes,
        source,
        "module",
        &HashMap::new(),
        &mut result,
        helpers,
    );
    result
}

// Each group pins all raw tests by fingerprint; an additional raw test changes the fingerprint.
// Identity and assignability tests retain T itself under §143 rule 1b.
struct AllowGroup {
    name: &'static str,
    sites: &'static [(&'static str, &'static str, u64)],
    reason: &'static str,
}

const ALLOWLIST: &[AllowGroup] = &[
    AllowGroup { name: "numeric-literal-range", reason: "The numeric literal caller supplies a concrete apparent numeric target (§143 rule 1a).", sites: &[
        ("expr.rs", "synthesized_int_range", 0x862a788d604204fc),
    ] },
    AllowGroup { name: "diagnostic-type-name", reason: "Diagnostic names preserve the declared parameter identity (§143 rule 1b).", sites: &[
        ("type_rules.rs", "type_name", 0x8245fa141bdc8bec),
    ] },
    AllowGroup { name: "concrete-captures", reason: "Capture validation runs on the final concrete HIR after opaque instances leave it (§135 rule 1).", sites: &[
        ("capture.rs", "expr", 0x4c573ebc382cad42),
        ("capture.rs", "fact", 0x80bc8ebc9b829d3a),
        ("capture.rs", "type_name", 0xfc88c0f8d7cdf95b),
    ] },
    AllowGroup { name: "union-members", reason: "Union decomposition preserves T identity; each member then uses apparent_type (§143 rule 1c).", sites: &[
        ("expr/call.rs", "check_indirect_call", 0xed9d360786d01965),
        ("expr/call.rs", "check_method_call_on", 0x193473593e642b0b),
        ("expr/member.rs", "member_on", 0xb1ffa3a2183a7b30),
    ] },
    AllowGroup { name: "instance-identity", reason: "Instance keys compare declared type arguments and preserve T identity (§143 rule 1b).", sites: &[
        ("generics.rs", "numeric_normal_form", 0x58518f62e52b0a50),
        ("generics.rs", "same_normal_form", 0x40ae5472edfe0ac7),
        ("generics.rs", "satisfies_constraint_through_parameters", 0x967a092b7f73d8c0),
        ("instance_chain.rs", "argument_has_error", 0xa9def2fd9d81a55),
    ] },
    AllowGroup { name: "concrete-initializers", reason: "Initializer effect analysis runs on the final concrete HIR (§135 rule 1).", sites: &[
        ("init_effects.rs", "class_of", 0xb041c5dbb9a8f91),
    ] },
    AllowGroup { name: "json-number", reason: "The callers supply apparent_type to this scalar conversion-code selector.", sites: &[
        ("json.rs", "json_number_target", 0xb5c32cde6ace4634),
    ] },
    AllowGroup { name: "concrete-layout", reason: "Layout validation runs after opaque instances leave the HIR (§135 rule 1).", sites: &[
        ("layout.rs", "expression_builds_into_destination", 0x749d74fbcd88c2ea),
        ("layout.rs", "has_managed_interior", 0x85b9753494a323a4),
        ("layout.rs", "is_managed", 0x21addcb1ee53ba12),
        ("layout.rs", "is_aggregate", 0x7641530cd3f750a2),
        ("layout.rs", "type_layout", 0x27b75ab225bb5ce7),
        ("layout.rs", "validate_expr_frame", 0xdfca9be927a9059),
    ] },
    AllowGroup { name: "storage-layout", reason: "Storage layout inspects the declared type form; T has no concrete layout (§143 rule 2a).", sites: &[
        ("layout.rs", "independent_type_layout", 0x2dc7ba25d703f629),
    ] },
    AllowGroup { name: "mirror-declarations", reason: "C mirror declarations have concrete boundary types and cannot declare type parameters.", sites: &[
        ("mirror_provenance.rs", "foreign_parameter_provenance", 0xdb604f5c10d571),
        ("signatures.rs", "resolve_mirror_signatures", 0xcbafc9db0a7db707),
    ] },
    AllowGroup { name: "parameter-form", reason: "These tests resolve or preserve T identity; they do not select a value operation (§143 rules 1a–1d).", sites: &[
        ("opaque.rs", "constrain_opaque_param", 0xae68ef4a8c9974a),
        ("opaque.rs", "constraint_cycle", 0x7903218a6714cb24),
        ("opaque.rs", "generic_overlap", 0x26896e6b772a01e2),
        ("opaque.rs", "generic_union", 0x4d201cd57ca0060d),
        ("opaque.rs", "involves_type_parameter", 0x73713f5554ee1dc7),
        ("opaque.rs", "is_type_parameter", 0x291d692ffcac9b32),
        ("opaque.rs", "is_unconstrained_type_parameter", 0xc11d80573c40a67b),
        ("opaque.rs", "non_null_type", 0x2c00558477fad7c),
        ("opaque.rs", "resolve_apparent_type", 0x764b551cf3b227d8),
    ] },
    AllowGroup { name: "operation-signatures", reason: "The operation dispatcher constructs these outer container, function, and Worker shapes before this pass.", sites: &[
        ("pipeline.rs", "normalize_operation_parameter_types", 0x3a8927e5a46cd33f),
    ] },
    AllowGroup { name: "closed-alias", reason: "The switch checker admits exhaustive aliases only from a declared StringAlias, never T.", sites: &[
        ("stmt.rs", "stmt_returns", 0x877d2a688f764b51),
    ] },
    AllowGroup { name: "assignability", reason: "Assignability and its diagnostics must inspect T itself (§143 rule 1b).", sites: &[
        ("type_rules.rs", "assignable_through_constraints", 0xa05ea280743aeccc),
        ("type_rules.rs", "report_not_assignable", 0xa944daeb4d9d32da),
    ] },
    AllowGroup { name: "wire-declarations", reason: "Wire boundary declarations come from concrete mirror types, which cannot declare type parameters.", sites: &[
        ("type_rules.rs", "contains_string_alias", 0xac8041c10ca4be34),
        ("type_rules.rs", "supported_wire_alias_boundary_type", 0xda2c3c78f1e6611c),
    ] },
    AllowGroup {
        name: "signature-identity",
        reason: "These declarations and return checks require exactly Void; constraint projection changes that identity (§143 rule 1b).",
        sites: &[
            ("class_shape.rs", "resolve_class_method", 0x5debbbffdf4b6cb8),
            ("class_shape.rs", "validate_class_index_accessors", 0xbab0ce8cdbcb4a),
            ("expr/method.rs", "check_map_group_by", 0xe321a8c6ea280c71),
            ("expr/namespace.rs", "check_worker_spawn", 0x5debbbffdf4b6cb8),
            ("host_entries.rs", "populate", 0x19f942a8ec1b1313),
            ("pipeline.rs", "visit_expr", 0xbfa87e28830c1a2f),
            ("stmt.rs", "check_return", 0xf0f9958c60713a35),
        ],
    },

];

fn fingerprint(sites: &[&Site]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for site in sites {
        for byte in format!("{}:{};", site.expression, site.pattern).bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn files(directory: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_value_shape_test_uses_the_apparent_type() {
    let start = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/check");
    let types = fs::read_to_string(root.parent().unwrap().join("types.rs")).unwrap();
    let helpers = shape_helpers(&types);
    let mut paths = Vec::new();
    files(&root, &mut paths);
    paths.sort();
    let mut failures = Vec::new();
    let mut inventory = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        let file = path.strip_prefix(&root).unwrap().to_str().unwrap();
        let source = fs::read_to_string(&path).unwrap();
        let sites = scan_with_helpers(&source, &helpers);
        let mut raw = HashMap::<&str, Vec<&Site>>::new();
        for site in &sites {
            inventory.push(format!(
                "| `{file}` | `{}` | {} | `{}` |",
                site.function,
                if site.routed { "routed" } else { "allowlisted" },
                site.expression.replace('|', "\\|")
            ));
            if !site.routed {
                raw.entry(&site.function).or_default().push(site);
            }
        }
        for (function, sites) in raw {
            let hash = fingerprint(&sites);
            let entry = ALLOWLIST.iter().find_map(|group| {
                group
                    .sites
                    .iter()
                    .find(|(f, name, _)| *f == file && *name == function)
                    .map(|(_, _, expected)| (*expected, group.reason))
            });
            if entry.is_some_and(|(expected, reason)| expected == hash && !reason.is_empty()) {
                seen.insert((file.to_string(), function.to_string()));
            } else {
                failures.push(format!(
                    "{file}::{function}: {hash:#x}, {} tests: {}",
                    sites.len(),
                    sites
                        .iter()
                        .map(|s| format!("{}: {} [{}]", s.line, s.expression, s.pattern))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
    }
    for group in ALLOWLIST {
        assert!(!group.name.is_empty() && !group.reason.is_empty());
        for (file, function, _) in group.sites {
            if !seen.contains(&(file.to_string(), function.to_string())) {
                failures.push(format!("stale allowlist: {file}::{function}"));
            }
        }
    }
    if let Some(path) = std::env::var_os("SUBSCRIPT_SHAPE_INVENTORY") {
        fs::write(path, inventory.join("\n")).unwrap();
    }
    failures.sort();
    eprintln!(
        "shape scan: {} sites, {} allowlist groups, {:?}",
        inventory.len(),
        ALLOWLIST.len(),
        start.elapsed()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn raw_shapes_fail_and_apparent_shapes_pass() {
    let raw = scan("fn f() { matches!(x.ty, Type::Class(_)); match x.ty { Type::Array(_) => {}, _ => {} } if let Type::Func(f) = x.ty {} x.ty.is_numeric(); }");
    assert_eq!(raw.len(), 4);
    assert!(raw.iter().all(|site| !site.routed));
    let routed = scan("fn f() { matches!(self.apparent_type(&x.ty), Type::Class(_)); let x = self.apparent_expr(x); match x.ty { Type::Array(_) => {}, _ => {} } }");
    assert_eq!(routed.len(), 2);
    assert!(routed.iter().all(|site| site.routed));
    assert!(scan(
        "fn f() { let s = r#\"matches!(x, Type::Class(_))\"#; /* match x { Type::Str => {} } */ }"
    )
    .is_empty());
}

#[test]
fn a_route_for_one_tuple_member_does_not_cover_another() {
    let sites =
        scan("fn f() { matches!((self.apparent_type(&a), b), (Type::Class(_), Type::Class(_))); }");
    assert_eq!(sites.len(), 1);
    assert!(!sites[0].routed);
    assert!(!routed("choose(self.apparent_type(&a), b)", ""));
}

#[test]
fn an_additional_raw_test_changes_the_allowlist_fingerprint() {
    let first = scan("fn f() { matches!(ty, Type::Class(_)); }");
    let added = scan("fn f() { matches!(ty, Type::Class(_)); matches!(ty, Type::Str); }");
    assert_ne!(
        fingerprint(&first.iter().collect::<Vec<_>>()),
        fingerprint(&added.iter().collect::<Vec<_>>())
    );
}

#[test]
fn a_route_in_a_child_scope_does_not_cover_the_parent_value() {
    let sites = scan("fn f() { let x = raw; { let x = self.apparent_type(&x); matches!(x, Type::Str); } matches!(x, Type::Class(_)); }");
    assert_eq!(sites.len(), 2);
    assert!(sites[0].routed);
    assert!(!sites[1].routed);
}

#[test]
fn variant_equality_requires_a_route_in_both_orders() {
    let sites = scan("fn f() { x.ty == Type::Str; Type::Str != x.ty; self.apparent_type(&x.ty) == Type::Str; Type::Str == self.apparent_type(&x.ty); }");
    assert_eq!(sites.len(), 4);
    assert!(!sites[0].routed && !sites[1].routed);
    assert!(sites[2].routed && sites[3].routed);
}

#[test]
fn a_new_shape_helper_requires_an_apparent_type() {
    let helpers = shape_helpers("impl Type { fn new_shape(&self) -> bool { matches!(self, Type::Str) } } fn free_shape(ignored: bool, ty: &'a Type) -> bool { match ty { Type::Str => true, _ => false } }");
    assert!(helpers.methods.contains("new_shape"));
    assert!(helpers.functions.contains_key("free_shape"));
    let sites = scan_with_helpers("fn consume() { x.ty.new_shape(); free_shape(true, &x.ty); self.apparent_type(&x.ty).new_shape(); free_shape(true, &self.apparent_type(&x.ty)); }", &helpers);
    assert_eq!(sites.len(), 4);
    assert!(!sites[0].routed && !sites[1].routed);
    assert!(sites[2].routed && sites[3].routed);
}
