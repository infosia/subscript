//! Shared pinned TypeScript configuration and executable lookup.

use std::path::{Path, PathBuf};

fn json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Builds a project that measures `files` with `tsconfig.json`'s
/// options. One text, so every gate in this file measures the
/// configuration the repository ships.
pub fn tsconfig(files: &[PathBuf]) -> String {
    let listed = files
        .iter()
        .map(|path| json_string(path.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        "{{\n  \"compilerOptions\": {{\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\",\n    \"moduleResolution\": \"Bundler\",\n    \"lib\": [\"ES2022\", \"ESNext.Disposable\"],\n    \"types\": [],\n    \"forceConsistentCasingInFileNames\": true\n  }},\n  \"files\": [\n    {listed}\n  ]\n}}\n"
    )
}

/// Answers the pinned TypeScript compiler.
///
/// `node_modules/.bin/tsc` is a POSIX shell script. Windows cannot
/// execute it (`os error 193`); npm writes `tsc.cmd` beside it for that
/// host.
pub fn tsc_binary(root: &Path) -> PathBuf {
    let tsc = root.join(if cfg!(windows) {
        "node_modules/.bin/tsc.cmd"
    } else {
        "node_modules/.bin/tsc"
    });
    assert!(
        tsc.is_file(),
        "the pinned TypeScript compiler is absent at {}",
        tsc.display()
    );
    tsc
}
