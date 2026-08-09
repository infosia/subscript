//! Resolves a Unix clang driver that supports x86 `_Float16`.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// An error from capable-clang resolution.
#[derive(Debug)]
pub(crate) struct ResolveClangError {
    message: String,
}

impl fmt::Display for ResolveClangError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ResolveClangError {}

/// Resolves `$CC`, `clang`, or the newest capable `clang-NN` on `PATH`.
pub(crate) fn resolve_capable_clang() -> Result<PathBuf, ResolveClangError> {
    resolve_capable_clang_with(std::env::var_os("CC"), std::env::var_os("PATH").as_deref())
}

fn resolve_capable_clang_with(
    cc: Option<OsString>,
    path: Option<&OsStr>,
) -> Result<PathBuf, ResolveClangError> {
    let probe_directory =
        TempDirectory::create("clang-probe").map_err(|error| ResolveClangError {
            message: format!("create the clang `_Float16` capability probe: {error}"),
        })?;
    let source = probe_directory.path().join("probe.c");
    fs::write(
        &source,
        b"#ifndef __clang__\n#error a clang compiler is required\n#endif\n\
typedef _Float16 half;\n",
    )
    .map_err(|error| ResolveClangError {
        message: format!("write the clang `_Float16` capability probe: {error}"),
    })?;

    if let Some(cc) = cc {
        let candidate = PathBuf::from(cc);
        if compiles_float16(&candidate, &source) {
            return Ok(candidate);
        }
        return Err(ResolveClangError {
            message: format!(
                "$CC is set to `{}`, but that compiler cannot compile x86 `_Float16`; \
                 set $CC to a capable clang (compiler.md §8.3 and §11a)",
                candidate.display()
            ),
        });
    }

    let candidates = clang_candidates(path);
    for candidate in &candidates {
        if compiles_float16(candidate, &source) {
            return Ok(candidate.clone());
        }
    }

    let tried = if candidates.is_empty() {
        "none were found on PATH".to_string()
    } else {
        candidates
            .iter()
            .map(|candidate| format!("`{}`", candidate.display()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(ResolveClangError {
        message: format!(
            "no clang compiler can compile x86 `_Float16`; tried {tried}. \
             Install clang 15 or newer, or set $CC to a capable clang \
             (compiler.md §8.3 and §11a)"
        ),
    })
}

fn clang_candidates(path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(clang) = find_on_path("clang", path) {
        candidates.push(clang);
    }

    let mut numbered = numbered_clangs(path);
    numbered.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    candidates.extend(numbered.into_iter().map(|(_, _, path)| path));

    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.clone()));
    candidates
}

fn find_on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    for directory in std::env::split_paths(path?) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn numbered_clangs(path: Option<&OsStr>) -> Vec<(u32, usize, PathBuf)> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for (directory_index, directory) in std::env::split_paths(path).enumerate() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let Some(version) = file_name.strip_prefix("clang-") else {
                continue;
            };
            if version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let Ok(version) = version.parse::<u32>() else {
                continue;
            };
            let candidate = entry.path();
            if candidate.is_file() {
                candidates.push((version, directory_index, candidate));
            }
        }
    }
    candidates
}

fn compiles_float16(candidate: &Path, source: &Path) -> bool {
    Command::new(candidate)
        .args(["-std=c11", "-fsyntax-only"])
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn create(label: &str) -> io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "subscript-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary directory",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_probe_picks_clang_15_and_rejects_incapable_explicit_cc() {
        let directory =
            TempDirectory::create("clang-resolver-test").expect("create resolver test directory");
        let driver_source = directory.path().join("fake-driver.rs");
        fs::write(
            &driver_source,
            r#"
use std::ffi::OsStr;
use std::path::Path;

fn main() {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    let name = Path::new(&arguments[0]).file_name();
    let flags_match = arguments.get(1).map(OsStr::new) == Some(OsStr::new("-std=c11"))
        && arguments.get(2).map(OsStr::new) == Some(OsStr::new("-fsyntax-only"));
    let source_matches = arguments
        .get(3)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|source| source.contains("typedef _Float16 half;\n"));
    let capable_name = name == Some(OsStr::new("clang-15"))
        || name == Some(OsStr::new("clang-14"));
    if !capable_name || !flags_match || !source_matches {
        std::process::exit(1);
    }
}
"#,
        )
        .expect("write fake clang source");

        let configured_cc = directory.path().join("configured-cc");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        let status = Command::new(rustc)
            .arg(&driver_source)
            .arg("-o")
            .arg(&configured_cc)
            .status()
            .expect("run rustc for the fake clang driver");
        assert!(status.success());
        fs::hard_link(&configured_cc, directory.path().join("clang"))
            .expect("create plain clang fake driver");
        fs::hard_link(&configured_cc, directory.path().join("clang-16"))
            .expect("create clang-16 fake driver");
        fs::hard_link(&configured_cc, directory.path().join("clang-15"))
            .expect("create clang-15 fake driver");
        fs::hard_link(&configured_cc, directory.path().join("clang-14"))
            .expect("create clang-14 fake driver");

        let resolved = resolve_capable_clang_with(None, Some(directory.path().as_os_str()))
            .expect("resolve the capable fake clang");
        assert_eq!(resolved, directory.path().join("clang-15"));

        let error = resolve_capable_clang_with(
            Some(configured_cc.into_os_string()),
            Some(directory.path().as_os_str()),
        )
        .expect_err("an explicitly configured incapable compiler must fail loud");
        assert!(error.to_string().contains("$CC is set"), "{error}");
    }
}
