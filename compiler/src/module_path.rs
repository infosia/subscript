//! The key of a module and the resolution of a specifier (compiler.md §354 rules 1–3 and 6).

/// The key of the module that the source file `name` holds
/// (compiler.md §354 rule 1).
///
/// The key is `name` normalized, without the suffix `.ts`. The separator is
/// `/`. The key has no empty segment and no `.` segment. A `..` segment
/// removes the segment before it; a `..` segment with no segment before it
/// stays. `a/./x.ts`, `a//x.ts`, and `a/b/../x.ts` have the key `a/x`.
#[must_use]
pub fn module_key(name: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in name.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|last| *last != "..") => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    let key = segments.join("/");
    match key.strip_suffix(".ts") {
        Some(stem) => stem.to_owned(),
        None => key,
    }
}

/// The key of the module that `specifier` names in the source file
/// `importer` (compiler.md §354 rules 2 and 3).
///
/// `importer` is the `name` of the importing file. A specifier that starts
/// with `./` or `../` is joined to the directory of the key of `importer`,
/// and [`module_key`] normalizes the result. Any other specifier, such as a
/// bare name or a standard module, names no file of the program: the result
/// is `None`.
#[must_use]
pub fn resolve_module_specifier(importer: &str, specifier: &str) -> Option<String> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }
    let importer = module_key(importer);
    Some(match importer.rfind('/') {
        Some(end) => module_key(&format!("{}/{specifier}", &importer[..end])),
        None => module_key(specifier),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cost (debug): below 0.01 s; string operations only.
    #[test]
    fn keys_are_normalized_paths_without_the_suffix() {
        for (name, key) in [
            ("main.ts", "main"),
            ("a/x.ts", "a/x"),
            ("a/./x.ts", "a/x"),
            ("a//x.ts", "a/x"),
            ("a/b/../x.ts", "a/x"),
            ("./a/x.ts", "a/x"),
            ("../x.ts", "../x"),
            ("a/../../x.ts", "../x"),
            ("../../x.ts", "../../x"),
            ("p.typegpu", "p.typegpu"),
            ("./p.typegpu", "p.typegpu"),
        ] {
            assert_eq!(module_key(name), key, "{name}");
        }
    }

    // Cost (debug): below 0.01 s; string operations only.
    #[test]
    fn relative_specifiers_resolve_against_the_importer() {
        for (importer, specifier, key) in [
            ("main.ts", "./x", Some("x")),
            ("main.ts", "../x", Some("../x")),
            ("main.ts", "./a/../b", Some("b")),
            ("main.ts", "./a/./b", Some("a/b")),
            ("main.ts", "./x.ts", Some("x")),
            ("sub/a.ts", "./x", Some("sub/x")),
            ("sub/a.ts", "../x", Some("x")),
            ("sub/deep/a.ts", "../../b/util", Some("b/util")),
            ("sub/./a.ts", "./x", Some("sub/x")),
            ("./sub/a.ts", "./x", Some("sub/x")),
            ("sub/../a.ts", "./x", Some("x")),
            ("main.ts", "x", None),
            ("sub/a.ts", "x", None),
            ("main.ts", "node:fs/promises", None),
            ("main.ts", "subscript:text", None),
            ("main.ts", ".", None),
            ("main.ts", "/x", None),
        ] {
            assert_eq!(
                resolve_module_specifier(importer, specifier).as_deref(),
                key,
                "{importer} {specifier}"
            );
        }
    }
}
