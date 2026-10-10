//! `subscript boundary` (`specs/blocks/compiler.md` §189 rule 1 and
//! acceptances 1 and 2): golden tests of the command output, the exit
//! codes, the coverage of the closed word set, and a compare of the plan
//! with the emitted C.
//!
//! The corpus goldens run the command from the workspace root on a corpus
//! entry with its interop mirrors, so each line starts with the relative
//! entry path. No accept entry can hold the two remaining words: a fixed
//! array of structs in a scratch struct stops code generation (`not
//! lowered`, 187.3 item 6), and no accept entry returns a struct with a
//! pointer member (`read by members`). A bound header in a scratch
//! directory gives both.
//!
//! Cost, measured on Apple arm64, debug: the goldens run 8 command
//! processes and 1 `bind` process (0.84 s alone); the exit-code test runs
//! 4 command processes (0.58 s alone); the C compare checks, lowers, and
//! emits C for the 7 corpus programs in the test process, with no C
//! compiler (0.30 s alone), and its negative control emits one program
//! (0.04 s). The file runs in 0.8 to 0.95 s.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "../../codegen/tests/corpus/mod.rs"]
#[allow(dead_code)]
mod corpus;

use subscript_boundary::PointerPass;
use subscript_compiler::crossing::{
    call_sites, CallPlan, MemberCrossing, ParameterPlan, PointerPlan, StructPlan, Word,
};

/// The corpus entries of the goldens, each with the words that it covers.
const ENTRIES: [(&str, &[&str]); 7] = [
    (
        "a100-interop-texture-descriptor-read",
        &[
            "script memory",
            "scratch written-back",
            "embedded bytes",
            "pair",
            "string view",
        ],
    ),
    (
        "a106-interop-recursive-struct-pointer-members",
        &["scratch read-only", "per element"],
    ),
    (
        "a103-interop-recursive-compute-pipeline",
        &["embedded scratch"],
    ),
    (
        "a348-boundary-values-have-c-form",
        &["by-value bytes", "read bytes", "value"],
    ),
    ("a27-interop-string-view", &["callback binding"]),
    (
        "a248-registration-script-reference-kept",
        &["callback registration", "by-value scratch"],
    ),
    ("a354-completion-string-bytes", &["completion endpoint"]),
];

/// A header whose program reaches the words that no accept entry holds.
const BOUND_HEADER: &str = "#include <stdint.h>\n\
typedef struct SPt { int32_t x; int32_t y; } SPt;\n\
typedef struct SLeaf { int32_t v; } SLeaf;\n\
typedef struct SShape { SPt pts[2]; SLeaf *leaf; } SShape;\n\
typedef struct SLink { float m[4]; SLeaf *leaf; } SLink;\n\
int32_t sDraw(const SShape *shape);\n\
SLink sLink(void);\n";

const BOUND_PROGRAM: &str = "export function main(): void {\n\
  const pts: FixedArray<SPt, 2> = [new SPt(1, 2), new SPt(3, 4)];\n\
  const shape = new SShape(pts, new SLeaf(5));\n\
  print(`${sDraw(shape)}`);\n\
  const link = sLink();\n\
  print(`${link.m[0]}`);\n\
}\n";

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-boundary-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        std::fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/boundary/{name}.txt"))
}

fn run(directory: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_subscript"))
        .current_dir(directory)
        .args(args)
        .output()
        .map_err(|error| format!("run subscript: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The command arguments of one corpus entry: the entry, its interop
/// mirrors, and its enabled modules.
fn entry_arguments(id: &str) -> Result<Vec<String>, String> {
    let relative = format!("corpus/accept/{id}.ts");
    let source = std::fs::read_to_string(workspace_root().join(&relative))
        .map_err(|error| format!("read {relative}: {error}"))?;
    let mut args = vec!["boundary".to_string(), relative];
    for name in corpus::interop::mirrors_for(&source, |name, _| name) {
        args.push("--mirror".into());
        args.push(format!("corpus/interop/{name}"));
    }
    for line in source.lines() {
        if let Some(module) = line.strip_prefix("// enable-module: ") {
            args.push("--enable-module".into());
            args.push(module.into());
        }
    }
    Ok(args)
}

/// The command output of each golden, after a compare with the committed
/// golden.
fn goldens() -> Result<Vec<String>, String> {
    let root = workspace_root();
    let mut outputs = Vec::new();
    for (id, _) in ENTRIES {
        let args = entry_arguments(id)?;
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = run(&root, &args)?;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{id}: {}",
            text(&output.stderr)
        );
        outputs.push(compare(id, &output.stdout)?);
    }
    let directory = TestDir::new()?;
    directory.write("shapes.h", BOUND_HEADER)?;
    directory.write("main.ts", BOUND_PROGRAM)?;
    let bound = run(
        &directory.0,
        &["bind", "shapes.h", "-o", "shapes.generated.d.ts"],
    )?;
    assert_eq!(bound.status.code(), Some(0), "{}", text(&bound.stderr));
    let output = run(
        &directory.0,
        &["boundary", "main.ts", "--mirror", "shapes.generated.d.ts"],
    )?;
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    outputs.push(compare("bound-shapes", &output.stdout)?);
    Ok(outputs)
}

fn compare(name: &str, stdout: &[u8]) -> Result<String, String> {
    let path = golden_path(name);
    let expected = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let found = text(stdout);
    if found != expected {
        return Err(format!(
            "{name}: the command output differs from {}:\n{found}",
            path.display()
        ));
    }
    Ok(found)
}

/// Acceptance 1: the goldens are byte-identical, and together they print
/// every word of the closed set.
#[test]
fn the_goldens_cover_every_word() -> Result<(), String> {
    let outputs = goldens()?;
    for ((id, words), output) in ENTRIES.iter().zip(&outputs) {
        for word in *words {
            assert!(output.contains(word), "{id} does not print `{word}`");
        }
    }
    let all = outputs.join("");
    for word in Word::ALL {
        let text = word.text();
        assert!(
            all.lines().any(|line| line.contains(&format!(": {text}"))),
            "no golden prints `{text}`"
        );
    }
    let bound = outputs.last().ok_or("no bound golden")?;
    assert!(bound.contains("not lowered: a fixed array of structs"));
    assert!(bound.contains("result: read by members [SLink.leaf]"));
    assert!(all.contains(" writes-back "));
    Ok(())
}

/// Rule 1: a program that `check` rejects exits 1 with the `check`
/// diagnostics; a missing source is a usage error.
#[test]
fn a_rejected_program_gives_the_check_status_and_diagnostics() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("bad.ts", "const value: number = 1;\n")?;
    let check = run(&directory.0, &["check", "bad.ts"])?;
    let boundary = run(&directory.0, &["boundary", "bad.ts"])?;
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(boundary.status.code(), Some(1));
    assert_eq!(boundary.stderr, check.stderr);
    assert!(boundary.stdout.is_empty());

    // A clean program with no foreign call prints nothing.
    directory.write("clean.ts", "export function main(): void {}\n")?;
    let clean = run(&directory.0, &["boundary", "clean.ts"])?;
    assert_eq!(clean.status.code(), Some(0), "{}", text(&clean.stderr));
    assert!(clean.stdout.is_empty());

    let usage = run(&directory.0, &["boundary"])?;
    assert_eq!(usage.status.code(), Some(2));
    assert!(text(&usage.stderr).contains("boundary requires <file.ts>"));
    Ok(())
}

/// What the plan of one callee says about each of its calls: the call
/// opens a scratch scope, and the call writes back.
type Expected = HashMap<String, (bool, bool)>;

/// True when a pointer node at any depth of `plan` is written back.
fn plan_writes_back(plan: &CallPlan) -> bool {
    fn structure(plan: &StructPlan) -> bool {
        plan.members.iter().any(|member| match &member.crossing {
            MemberCrossing::Embedded(nested) => structure(nested),
            MemberCrossing::Pointer(pointer) => pointer_node(pointer),
            MemberCrossing::Pair(pair) => pair
                .elements
                .as_ref()
                .and_then(|elements| elements.element.as_ref())
                .is_some_and(structure),
            _ => false,
        })
    }
    fn pointer_node(plan: &PointerPlan) -> bool {
        plan.pass == PointerPass::ScratchWrittenBack || plan.target.as_ref().is_some_and(structure)
    }
    plan.parameters.iter().any(|parameter| match parameter {
        ParameterPlan::ByValue(plan) => structure(plan),
        ParameterPlan::Pointer(plan) => pointer_node(plan),
        ParameterPlan::Pair(pair) => pair
            .elements
            .as_ref()
            .and_then(|elements| elements.element.as_ref())
            .is_some_and(structure),
        _ => false,
    })
}

/// The scratch mark and the write-back of each call of `callee` in the
/// emitted C. A mark is in the build that comes after the previous call
/// or the function start; a write-back is the guarded block that follows
/// the call statement and its result copy.
fn emitted_calls(c: &str, callee: &str) -> Vec<(bool, bool)> {
    let lines = c.lines().collect::<Vec<_>>();
    let call = format!("{callee}(");
    let is_call = |line: &str| {
        line.match_indices(&call).any(|(at, _)| {
            line[..at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
        }) && line.starts_with("    ")
    };
    let mut calls = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !is_call(line) {
            continue;
        }
        let mut mark = false;
        for before in lines[..index].iter().rev() {
            if !before.starts_with(' ')
                || before.contains("subscript_rt_boundary_scratch_release(")
                || before
                    .trim_start()
                    .starts_with("if (*(const uint32_t*)ctx == 0u) {")
            {
                break;
            }
            if before.contains("subscript_rt_boundary_scratch_mark(") {
                mark = true;
                break;
            }
        }
        let mut after = lines[index + 1..].iter();
        let mut next = after.next();
        if next.is_some_and(|line| line.starts_with("    memcpy(&")) {
            next = after.next();
        }
        let write_back = next.is_some_and(|line| *line == "    if (*(const uint32_t*)ctx == 0u) {");
        calls.push((mark, write_back));
    }
    calls
}

/// Compares the emitted C with the plans: each call of each callee has a
/// scratch mark exactly when the plan opens a scratch scope, and a
/// write-back exactly when the plan writes back. The number of calls is
/// the number of sites that the command prints.
fn check_emitted(
    c: &str,
    expected: &Expected,
    sites: &HashMap<String, usize>,
) -> Result<Vec<(bool, bool)>, String> {
    let mut seen = Vec::new();
    for (callee, plan) in expected {
        let calls = emitted_calls(c, callee);
        let count = sites.get(callee).copied().unwrap_or(0);
        if calls.len() != count {
            return Err(format!(
                "`{callee}`: {} calls in the C, {count} sites in the plan",
                calls.len()
            ));
        }
        for call in calls {
            if call != *plan {
                return Err(format!(
                    "`{callee}`: the C has (scratch mark, write-back) {call:?}, the plan {plan:?}"
                ));
            }
            seen.push(call);
        }
    }
    Ok(seen)
}

/// The emitted C, the plan of each callee, and the site count of each
/// callee of one corpus entry.
fn emitted_entry(id: &str) -> Result<(String, Expected, HashMap<String, usize>), String> {
    let accept = corpus::corpus_accept();
    let files = corpus::entry_sources(&accept, id);
    let hir = corpus::check_program(&files).map_err(|d| format!("{id}: rejected: {d:?}"))?;
    let lowered = subscript_codegen::lir::lower_module(&hir).map_err(|e| format!("{id}: {e}"))?;
    let c = subscript_codegen::emit_c(&hir)?.source;
    let mut expected = Expected::new();
    let mut sites = HashMap::new();
    for site in call_sites(&lowered)? {
        let name = lowered.foreign_functions[site.callee.0 as usize]
            .source_name
            .clone();
        expected.insert(
            name.clone(),
            (site.plan.scratch_scope, plan_writes_back(&site.plan)),
        );
        *sites.entry(name).or_insert(0) += 1;
    }
    Ok((c, expected, sites))
}

/// Acceptance 2: for each call site of the corpus programs of acceptance
/// 1, the emitted C holds a scratch mark exactly when the plan opens a
/// scratch scope, and a write-back exactly when the plan writes back. The
/// bound program has no C: its fixed array of structs stops emission.
#[test]
fn the_emitted_c_marks_and_writes_back_as_the_plan_says() -> Result<(), String> {
    let mut seen = Vec::new();
    for (id, _) in ENTRIES {
        let (c, expected, sites) = emitted_entry(id)?;
        seen.extend(check_emitted(&c, &expected, &sites).map_err(|e| format!("{id}: {e}"))?);
    }
    // Each fact occurs in both forms, so the compare can fail both ways.
    for mark in [true, false] {
        assert!(
            seen.iter().any(|call| call.0 == mark),
            "no call with mark {mark}"
        );
    }
    for write_back in [true, false] {
        assert!(
            seen.iter().any(|call| call.1 == write_back),
            "no call with write-back {write_back}"
        );
    }
    Ok(())
}

/// Core principle 9: a C text that drops a write-back or adds a scratch
/// mark, and a plan that differs from the C, each fail the compare. The
/// test builds each violating form; it changes no record that a check
/// reads.
#[test]
fn a_c_text_or_a_plan_that_disagrees_fails_the_compare() -> Result<(), String> {
    let (c, expected, sites) = emitted_entry("a100-interop-texture-descriptor-read")?;
    check_emitted(&c, &expected, &sites)?;
    let callee = "subProbeTextureDescriptorFill";
    assert_eq!(expected.get(callee), Some(&(false, true)));

    let guard = format!("{callee}(t5);\n    if (*(const uint32_t*)ctx == 0u) {{");
    assert!(c.contains(&guard), "the write-back follows the call");
    let dropped = c.replacen(&guard, &format!("{callee}(t5);\n    {{"), 1);
    assert!(check_emitted(&dropped, &expected, &sites).is_err());

    let marked = c.replacen(
        &format!("    {callee}(t5);"),
        &format!("    uint64_t t99 = subscript_rt_boundary_scratch_mark(ctx);\n    {callee}(t5);"),
        1,
    );
    assert!(check_emitted(&marked, &expected, &sites).is_err());

    let mut other = expected.clone();
    other.insert(callee.into(), (true, true));
    assert!(check_emitted(&c, &other, &sites).is_err());
    let mut fewer = sites.clone();
    fewer.insert(callee.into(), 2);
    assert!(check_emitted(&c, &expected, &fewer).is_err());
    Ok(())
}
