//! Tutorial fences check and program output matches adjacent text (compiler.md §91).

use std::path::Path;

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

#[derive(Clone)]
struct Block {
    language: String,
    line: usize,
    source: String,
    closing_line: Option<usize>,
}

fn blocks(markdown: &str) -> Vec<Block> {
    let mut result = Vec::new();
    let mut open: Option<(char, usize, Block)> = None;
    for (index, line) in markdown.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if let Some((marker, width, block)) = &mut open {
            let count = trimmed.chars().take_while(|ch| ch == marker).count();
            if indent <= 3 && count >= *width && trimmed[count..].trim().is_empty() {
                block.closing_line = Some(index + 1);
                result.push(open.take().unwrap().2);
            } else {
                block.source.push_str(line);
            }
        } else if indent <= 3 && (trimmed.starts_with('`') || trimmed.starts_with('~')) {
            let marker = trimmed.chars().next().unwrap();
            let width = trimmed.chars().take_while(|ch| *ch == marker).count();
            if width >= 3 {
                open = Some((
                    marker,
                    width,
                    Block {
                        language: trimmed[width..]
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .into(),
                        line: index + 1,
                        source: String::new(),
                        closing_line: None,
                    },
                ));
            }
        }
    }
    if let Some((_, _, block)) = open {
        result.push(block);
    }
    result
}

fn immediate_output<'a>(
    program: &Block,
    next: Option<&'a Block>,
    lines: &[&str],
) -> Option<&'a Block> {
    let closing = program.closing_line?;
    next.filter(|block| {
        output_text(block).is_some()
            && (block.line == closing + 1
                || (block.line == closing + 2
                    && lines
                        .get(closing)
                        .is_some_and(|line| line.trim().is_empty())))
    })
}

fn output_text(block: &Block) -> Option<&str> {
    match block.language.as_str() {
        "text" => Some(&block.source),
        "sh" => {
            let (command, output) = block.source.split_once('\n')?;
            let file = command
                .trim_end_matches('\r')
                .strip_prefix("$ subscript run ")?;
            (!file.is_empty() && !file.chars().any(char::is_whitespace)).then_some(output)
        }
        _ => None,
    }
}

fn check_program_output(
    file: &str,
    markdown: &str,
    fence_line: usize,
    actual: &[u8],
) -> Result<bool, String> {
    let blocks = blocks(markdown);
    let index = blocks
        .iter()
        .position(|block| block.line == fence_line)
        .ok_or_else(|| format!("{file}:{fence_line}: program fence is missing"))?;
    let lines = markdown.lines().collect::<Vec<_>>();
    if let Some(expected) = immediate_output(&blocks[index], blocks.get(index + 1), &lines) {
        compare_output(
            file,
            fence_line,
            actual,
            output_text(expected).unwrap().as_bytes(),
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[test]
fn output_pairing_requires_adjacent_fences() {
    for (gap, paired) in [
        ("", true),
        ("\n", true),
        ("  \n", true),
        ("prose\n", false),
        ("\n\n", false),
        ("\nprose\n\n", false),
    ] {
        let markdown =
            format!("```ts\nexport function main(): void {{}}\n```\n{gap}```text\noutput\n```\n");
        let blocks = blocks(&markdown);
        let lines = markdown.lines().collect::<Vec<_>>();
        assert_eq!(
            immediate_output(&blocks[0], blocks.get(1), &lines).is_some(),
            paired,
            "gap: {gap:?}"
        );
    }
}

fn words(source: &str) -> Vec<&str> {
    source
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '$')
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_program(block: &Block) -> bool {
    let words = words(&block.source);
    words
        .windows(3)
        .any(|part| part == ["export", "function", "main"])
        || words
            .windows(4)
            .any(|part| part == ["export", "async", "function", "main"])
}

fn sources(block: &Block, preceding: Option<&Block>) -> Result<Vec<SourceFile>, String> {
    let ambient = !is_program(block)
        && words(&block.source)
            .iter()
            .any(|word| matches!(*word, "interface" | "declare"));
    let entry = if ambient {
        SourceFile::ambient("snippet.d.ts", &block.source)
    } else {
        SourceFile::new("snippet.ts", &block.source)
    };
    let mut files = vec![entry];
    for line in block
        .source
        .lines()
        .filter(|line| line.trim_start().starts_with("import "))
    {
        let specifier = line
            .split(['\"', '\''])
            .nth(1)
            .ok_or("import has no module string")?;
        if let Some(name) = specifier.strip_prefix("./") {
            let sibling = preceding
                .filter(|block| block.language == "ts")
                .ok_or("relative import has no preceding TypeScript block")?;
            let name = if name.ends_with(".ts") {
                name.to_owned()
            } else {
                format!("{name}.ts")
            };
            files.push(SourceFile::new(name, &sibling.source));
        }
    }
    Ok(files)
}

fn first_diagnostic(diagnostics: &[Diagnostic]) -> String {
    let diagnostic = &diagnostics[0];
    format!("{:?} {}", diagnostic.code, diagnostic.message)
}

fn excerpt_path(source: &str) -> Option<&str> {
    source.lines().next()?.strip_prefix("// excerpt of ")
}

fn check_excerpt_lines(source: &str, path: &str, cited: &str) -> Result<(), String> {
    for line in source.lines().skip(1) {
        let line = line.trim_end();
        if !line.is_empty() && !cited.lines().any(|candidate| candidate.trim_end() == line) {
            return Err(format!("excerpt of {path}: missing line {line:?}"));
        }
    }
    Ok(())
}

fn check_excerpt(root: &Path, source: &str, path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.trim() != path
        || !Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "excerpt of {path:?}: expected a repository-relative file path"
        ));
    }
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(["ls-files", "--error-unmatch", "--", path])
        .output()
        .map_err(|error| format!("excerpt of {path}: check tracked file: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "excerpt of {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let cited = std::fs::read_to_string(root.join(path))
        .map_err(|error| format!("excerpt of {path}: {error}"))?;
    check_excerpt_lines(source, path, &cited)
}

#[test]
fn excerpts_match_only_the_first_line_and_preserve_leading_whitespace() {
    let source = "// excerpt of example.d.ts\n  value: i32;  \n\n}\n";
    assert_eq!(excerpt_path(source), Some("example.d.ts"));
    assert_eq!(excerpt_path(&format!("\n{source}")), None);
    assert_eq!(excerpt_path(&format!(" {source}")), None);
    assert!(check_excerpt_lines(source, "example.d.ts", "}\n  value: i32;\n").is_ok());
    let error = check_excerpt_lines(source, "example.d.ts", "}\nvalue: i32;\n")
        .expect_err("leading whitespace must match");
    assert_eq!(
        error,
        "excerpt of example.d.ts: missing line \"  value: i32;\""
    );
    let altered = source.replace("i32", "i64");
    assert!(check_excerpt_lines(&altered, "example.d.ts", "}\n  value: i32;\n").is_err());
}

fn check_fragment(files: &[SourceFile], mirrors: &[SourceFile]) -> Result<(), String> {
    let diagnostics = match check_program(files) {
        Ok(_) => return Ok(()),
        Err(diagnostics) => diagnostics,
    };
    let unknown = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == RuleCode::S016);
    let mut tried = Vec::new();
    if unknown {
        for mirror in mirrors {
            tried.push(mirror.name.as_str());
            let mut candidate = files.to_vec();
            candidate.push(mirror.clone());
            if check_program(&candidate).is_ok() {
                return Ok(());
            }
        }
    }
    Err(format!(
        "{}; mirrors tried: {tried:?}",
        first_diagnostic(&diagnostics)
    ))
}

fn compare_output(file: &str, line: usize, actual: &[u8], expected: &[u8]) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{file}:{line}: stdout differs\nactual bytes: {actual:?}\nexpected bytes: {expected:?}"
        ))
    }
}

fn committed_mirrors(root: &Path) -> Vec<SourceFile> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z", "--", "examples/", "corpus/interop/"])
        .output()
        .expect("list committed mirrors");
    assert!(
        output.status.success(),
        "git ls-files failed: {:?}",
        output.stderr
    );
    let paths = String::from_utf8(output.stdout).unwrap();
    paths
        .split('\0')
        .filter(|path| path.ends_with(".d.ts"))
        .map(|path| SourceFile::ambient(path, std::fs::read_to_string(root.join(path)).unwrap()))
        .collect()
}

#[test]
fn documentation_blocks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut documents = vec![root.join("README.md")];
    let mut tutorials = std::fs::read_dir(root.join("docs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    tutorials.sort();
    documents.extend(tutorials);
    let mirrors = committed_mirrors(root);
    let mut failures = Vec::new();
    let mut text_control = false;
    let mut shell_control = false;
    let mut fragment_control = false;
    for document in documents {
        let file = document.strip_prefix(root).unwrap().to_str().unwrap();
        let markdown = std::fs::read_to_string(&document).unwrap();
        let lines = markdown.lines().collect::<Vec<_>>();
        let blocks = blocks(&markdown);
        let expected_ts = match file {
            "README.md" => 1,
            "docs/tutorial-c-cpp.md" => 11,
            "docs/tutorial-rust.md" => 3,
            "docs/tutorial-typescript.md" => 17,
            _ => panic!("{file}: add the measured TypeScript fence count to the scope table"),
        };
        assert_eq!(
            blocks.iter().filter(|block| block.language == "ts").count(),
            expected_ts,
            "{file}: TypeScript fence count differs from the scope table"
        );
        let (mut programs, mut compared, mut fragments, mut excerpts) = (0, 0, 0, 0);
        for (index, block) in blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.language == "ts")
        {
            if let Some(path) = excerpt_path(&block.source) {
                excerpts += 1;
                if let Err(error) = check_excerpt(root, &block.source, path) {
                    failures.push(format!("{file}:{}: {error}", block.line));
                }
                continue;
            }
            let program = is_program(block);
            if program {
                programs += 1;
            } else {
                fragments += 1;
            }
            let expected = immediate_output(block, blocks.get(index + 1), &lines);
            if program && expected.is_some() {
                compared += 1;
            }
            let result = (|| {
                let preceding = index.checked_sub(1).and_then(|index| blocks.get(index));
                let files = sources(block, preceding)?;
                if program {
                    check_program(&files).map_err(|diagnostics| first_diagnostic(&diagnostics))?;
                    let actual =
                        subscript_codegen::run_jit(&files).map_err(|error| error.to_string())?;
                    check_program_output(file, &markdown, block.line, &actual)?;
                    if let Some(expected) = expected {
                        let control = if expected.language == "sh" {
                            &mut shell_control
                        } else {
                            &mut text_control
                        };
                        let expected_text = output_text(expected).unwrap();
                        if !*control && !expected_text.is_empty() {
                            let offset = markdown
                                .split_inclusive('\n')
                                .take(expected.line)
                                .map(str::len)
                                .sum::<usize>()
                                + expected.source.len()
                                - expected_text.len();
                            let mut altered = markdown.as_bytes().to_vec();
                            altered[offset] ^= 1;
                            let altered = String::from_utf8(altered).unwrap();
                            assert!(
                                check_program_output(file, &altered, block.line, &actual).is_err()
                            );
                            println!("negative control: {file}:{} changed {} output byte rejected through extraction", block.line, expected.language);
                            *control = true;
                        }
                    }
                } else {
                    check_fragment(&files, &mirrors)?;
                    if !fragment_control && !files[0].dts {
                        let mut altered = block.clone();
                        altered
                            .source
                            .push_str("\nconst docsNegativeControl: i32 = docsUndeclaredName;\n");
                        let altered_files = sources(&altered, preceding)?;
                        assert!(check_fragment(&altered_files, &mirrors).is_err());
                        println!(
                            "negative control: {file}:{} undeclared fragment name rejected",
                            block.line
                        );
                        fragment_control = true;
                    }
                }
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                failures.push(format!("{file}:{}: {error}", block.line));
            }
        }
        println!("{file}: programs={programs}, compared outputs={compared}, fragments={fragments}, excerpts={excerpts}");
    }
    assert!(text_control, "no text output negative control ran");
    assert!(shell_control, "no shell output negative control ran");
    assert!(fragment_control, "no fragment negative control ran");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

struct TestRepository(std::path::PathBuf);

impl TestRepository {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "subscript-docs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let status = std::process::Command::new("git")
            .current_dir(&root)
            .args(["init", "-q"])
            .status()
            .unwrap();
        assert!(status.success());
        Self(root)
    }

    fn track(&self, path: &str, contents: &str) {
        let file = self.0.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
        assert!(std::process::Command::new("git")
            .current_dir(&self.0)
            .args(["add", "--", path])
            .status()
            .unwrap()
            .success());
    }
}

impl Drop for TestRepository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn excerpt_tracks_working_tree_changes() {
    let repo = TestRepository::new();
    repo.track("mirror.d.ts", "type Value = i32;\n");
    let source = "// excerpt of mirror.d.ts\ntype Value = i32;\n";
    assert!(check_excerpt(&repo.0, source, "mirror.d.ts").is_ok());
    std::fs::write(repo.0.join("mirror.d.ts"), "type Value = i64;\n").unwrap();
    assert!(check_excerpt(&repo.0, source, "mirror.d.ts").is_err());
    assert!(check_excerpt(&repo.0, &source.replace("i32", "i64"), "mirror.d.ts").is_ok());
}

#[test]
fn mirror_inventory_includes_tracked_declaration_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mirrors = committed_mirrors(root);
    assert!(mirrors
        .iter()
        .any(|mirror| mirror.name == "corpus/interop/wire-enum-aliases.d.ts"));
}

#[test]
fn shell_transcript_is_program_output() {
    let markdown = "```ts\nexport function main(): void {}\n```\n\n```sh\n$ subscript run hello.ts\nhello\n```\n";
    let blocks = blocks(markdown);
    assert!(immediate_output(
        &blocks[0],
        blocks.get(1),
        &markdown.lines().collect::<Vec<_>>()
    )
    .is_some());
    assert_eq!(
        check_program_output("example.md", markdown, 1, b"hello\n"),
        Ok(true)
    );
    assert!(check_program_output("example.md", markdown, 1, b"wrong\n").is_err());
    let separated = markdown.replace("\n\n```sh", "\nprose\n```sh");
    assert_eq!(
        check_program_output("example.md", &separated, 1, b"hello\n"),
        Ok(false)
    );
    let other_command = markdown.replace("$ subscript run hello.ts", "$ subscript build hello.ts");
    assert_eq!(
        check_program_output("example.md", &other_command, 1, b"hello\n"),
        Ok(false)
    );
}
