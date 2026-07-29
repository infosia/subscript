use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use subscript_codegen::runtime_staticlib_name;

pub(crate) const RUNTIME_LIB_ENV: &str = "SUBSCRIPT_RUNTIME_LIB";
pub(crate) const RUNTIME_INCLUDE_ENV: &str = "SUBSCRIPT_RUNTIME_INCLUDE";

#[derive(Debug, Default)]
pub(crate) struct RuntimeOverrides {
    pub(crate) library: Option<PathBuf>,
    pub(crate) include: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeEnvironment {
    library: Option<PathBuf>,
    include: Option<PathBuf>,
}

impl RuntimeEnvironment {
    pub(crate) fn current() -> Self {
        Self {
            library: std::env::var_os(RUNTIME_LIB_ENV).map(PathBuf::from),
            include: std::env::var_os(RUNTIME_INCLUDE_ENV).map(PathBuf::from),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuntimePaths {
    pub(crate) library: PathBuf,
    pub(crate) include: PathBuf,
}

pub(crate) fn resolve_runtime_paths(
    overrides: RuntimeOverrides,
    environment: RuntimeEnvironment,
    start_directory: &Path,
) -> Result<RuntimePaths, String> {
    resolve_runtime_paths_with(overrides, environment, start_directory, build_runtime)
}

fn resolve_runtime_paths_with(
    overrides: RuntimeOverrides,
    environment: RuntimeEnvironment,
    start_directory: &Path,
    build_default: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<RuntimePaths, String> {
    let repository = find_repository(start_directory);
    let library = match overrides.library {
        Some(path) => require_file(absolute(path, start_directory), "--runtime-lib")?,
        None => match environment.library {
            Some(path) => require_file(absolute(path, start_directory), RUNTIME_LIB_ENV)?,
            None => {
                let root = repository.as_ref().ok_or_else(missing_resolution_message)?;
                build_default(root)?;
                let archive = root
                    .join("target")
                    .join("release")
                    .join(runtime_staticlib_name());
                require_file(archive, "in-repo runtime archive")?
            }
        },
    };
    let include = match overrides.include {
        Some(path) => require_directory(absolute(path, start_directory), "--runtime-include")?,
        None => match environment.include {
            Some(path) => require_directory(absolute(path, start_directory), RUNTIME_INCLUDE_ENV)?,
            None => {
                let root = repository.as_ref().ok_or_else(missing_resolution_message)?;
                require_directory(
                    root.join("runtime").join("include"),
                    "in-repo runtime include directory",
                )?
            }
        },
    };
    Ok(RuntimePaths { library, include })
}

fn absolute(path: PathBuf, start_directory: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        start_directory.join(path)
    }
}

fn require_file(path: PathBuf, mechanism: &str) -> Result<PathBuf, String> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{mechanism} runtime archive was not found at {}",
            path.display()
        ))
    }
}

fn require_directory(path: PathBuf, mechanism: &str) -> Result<PathBuf, String> {
    if path.is_dir() {
        Ok(path)
    } else {
        Err(format!(
            "{mechanism} runtime include directory was not found at {}",
            path.display()
        ))
    }
}

fn find_repository(start_directory: &Path) -> Option<PathBuf> {
    start_directory.ancestors().find_map(|candidate| {
        let workspace = candidate.join("Cargo.toml");
        let runtime = candidate.join("runtime").join("Cargo.toml");
        let codegen = candidate.join("codegen").join("Cargo.toml");
        (workspace.is_file() && runtime.is_file() && codegen.is_file())
            .then(|| candidate.to_path_buf())
    })
}

fn missing_resolution_message() -> String {
    format!(
        "runtime paths cannot be resolved outside a subscript repository; \
         pass --runtime-lib and --runtime-include, set {RUNTIME_LIB_ENV} and \
         {RUNTIME_INCLUDE_ENV}, or run from a repository with the in-repo defaults"
    )
}

fn build_runtime(repository: &Path) -> Result<(), String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let output = Command::new(cargo)
        .args(["build", "--offline", "--release", "-p", "subscript-runtime"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("build the in-repo runtime archive: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "building the in-repo runtime archive failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "subscript-runtime-paths-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path)
                .map_err(|error| format!("create {}: {error}", path.display()))?;
            Ok(Self(path))
        }

        fn file(&self, relative: &str) -> Result<PathBuf, String> {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            std::fs::write(&path, b"archive")
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            Ok(path)
        }

        fn directory(&self, relative: &str) -> Result<PathBuf, String> {
            let path = self.0.join(relative);
            std::fs::create_dir_all(&path)
                .map_err(|error| format!("create {}: {error}", path.display()))?;
            Ok(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fake_repository(directory: &TestDir) -> Result<RuntimePaths, String> {
        directory.file("Cargo.toml")?;
        directory.file("runtime/Cargo.toml")?;
        directory.file("codegen/Cargo.toml")?;
        let include = directory.directory("runtime/include")?;
        let library = directory.file(
            Path::new("target")
                .join("release")
                .join(runtime_staticlib_name())
                .to_string_lossy()
                .as_ref(),
        )?;
        Ok(RuntimePaths { library, include })
    }

    #[test]
    fn runtime_resolution_flags_beat_environment_and_default() -> Result<(), String> {
        let directory = TestDir::new()?;
        let _default = fake_repository(&directory)?;
        let flag_library = directory.file("flag/runtime.a")?;
        let flag_include = directory.directory("flag/include")?;
        let env_library = directory.file("env/runtime.a")?;
        let env_include = directory.directory("env/include")?;

        let resolved = resolve_runtime_paths_with(
            RuntimeOverrides {
                library: Some(flag_library.clone()),
                include: Some(flag_include.clone()),
            },
            RuntimeEnvironment {
                library: Some(env_library),
                include: Some(env_include),
            },
            &directory.0,
            |_| Err("default builder must not run".to_string()),
        )?;
        assert_eq!(
            resolved,
            RuntimePaths {
                library: flag_library,
                include: flag_include,
            }
        );
        Ok(())
    }

    #[test]
    fn runtime_resolution_environment_beats_default() -> Result<(), String> {
        let directory = TestDir::new()?;
        let _default = fake_repository(&directory)?;
        let env_library = directory.file("env/runtime.a")?;
        let env_include = directory.directory("env/include")?;

        let resolved = resolve_runtime_paths_with(
            RuntimeOverrides::default(),
            RuntimeEnvironment {
                library: Some(env_library.clone()),
                include: Some(env_include.clone()),
            },
            &directory.0,
            |_| Err("default builder must not run".to_string()),
        )?;
        assert_eq!(
            resolved,
            RuntimePaths {
                library: env_library,
                include: env_include,
            }
        );
        Ok(())
    }

    #[test]
    fn runtime_resolution_uses_in_repo_default() -> Result<(), String> {
        let directory = TestDir::new()?;
        let expected = fake_repository(&directory)?;
        let nested = directory.directory("some/nested/directory")?;
        let mut built = false;
        let resolved = resolve_runtime_paths_with(
            RuntimeOverrides::default(),
            RuntimeEnvironment::default(),
            &nested,
            |_| {
                built = true;
                Ok(())
            },
        )?;
        assert!(built);
        assert_eq!(resolved, expected);
        Ok(())
    }

    #[test]
    fn runtime_resolution_outside_repo_names_all_mechanisms() -> Result<(), String> {
        let directory = TestDir::new()?;
        let error = resolve_runtime_paths_with(
            RuntimeOverrides::default(),
            RuntimeEnvironment::default(),
            &directory.0,
            |_| Err("default builder must not run".to_string()),
        )
        .expect_err("outside-repository resolution must fail");
        for mechanism in [
            "--runtime-lib",
            "--runtime-include",
            RUNTIME_LIB_ENV,
            RUNTIME_INCLUDE_ENV,
            "in-repo defaults",
        ] {
            assert!(error.contains(mechanism), "{error}");
        }
        Ok(())
    }
}
