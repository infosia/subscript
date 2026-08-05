//! The ship-tier AOT driver (`specs/blocks/compiler.md` §8.1).
//!
//! The *same* lowering the dev JIT uses, instantiated with
//! `cranelift-object` instead of `cranelift-jit` and with the ship
//! flag set (`is_pic = true`; the dev tier resolves everything by
//! absolute in-process address and uses `is_pic = false`).
//!
//! Two entry points:
//! - [`emit_object`] produces a relocatable object for any supported
//!   target triple. The device triples (`aarch64-apple-ios`,
//!   `aarch64-linux-android`) go through this; `codegen/device-link.sh`
//!   drives it through the `emit-object` binary and links the results
//!   with the platform toolchains.
//! - [`run_aot`] does the whole host-target cycle in a temporary
//!   directory: emit the object, write the C entry program, link both
//!   against the runtime static library with the host C compiler
//!   (clang, §11), run the binary, and return the stdout bytes it
//!   produced.
//!
//! # Entry program
//!
//! The linked program has no `main` of its own: the lowering exports
//! `subscript_init` (module-global initializer) and `subscript_export_<name>` for
//! every exported script function. The C entry creates a Context
//! through the runtime's host-driver entry points, calls `subscript_init` and
//! `subscript_export_main`, streams each complete printed line to the process
//! stdout, and reports a trap on stderr with a non-zero exit status. Streaming
//! is required so output already produced survives an aborting native call;
//! the run helper still captures and returns the byte-exact stream.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use cranelift_object::object::macho::PLATFORM_IOS;
use cranelift_object::object::write::MachOBuildVersion;
use cranelift_object::{ObjectBuilder, ObjectModule};
use subscript_compiler::{check_program, Pos, SourceFile};
use subscript_runtime::TrapKind;
use target_lexicon::Triple;

use crate::jit::{AbnormalTermination, RunError, TrapReport};
use crate::lower::{aot_flags, internal, lower_module_with, LowerOptions};
use crate::native::missing_symbol;
use crate::NativeLibrary;

/// Mach-O build version `10.0.0`, nibble-packed as `xxxx.yy.zz`. Apple's
/// linker rejects an iOS object with no `LC_BUILD_VERSION`, and
/// `cranelift-object 0.125.4` does not stamp one
/// (`specs/tracking/p0.5-mobile-link.md`).
const MACHO_VERSION_10_0_0: u32 = 10 << 16;

/// Environment variable naming a prebuilt runtime static library for
/// [`run_aot`] to link against. When unset (the normal case, including
/// the differential gate), `run_aot` looks for the archive next to the
/// current executable and builds `subscript-runtime` with the
/// workspace's own cargo if it is missing or stale.
///
/// It exists for a host-target link that must use an archive cargo did
/// not just build. The device-triple links do not use it: they never
/// call `run_aot`, and `codegen/device-link.sh` passes each
/// cross-compiled archive to the platform linker directly.
pub const RUNTIME_STATICLIB_ENV: &str = "SUBSCRIPT_RUNTIME_STATICLIB";

/// A relocatable object emitted for one target triple, with the trap
/// position table the lowering built for it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AotObject {
    /// Target triple the object was emitted for.
    pub triple: String,
    /// Object file bytes, ready to write and link.
    pub bytes: Vec<u8>,
    /// Trap position table: `pos_id` -> TS position.
    pub positions: Vec<Pos>,
    foreign_symbols: Vec<String>,
}

/// Checks `files` and emits a relocatable object for `triple`.
///
/// Pass `None` for the host triple. The object defines `subscript_init` and
/// one `subscript_export_<name>` per exported script function, and imports the
/// runtime's `subscript_rt_*` symbols, which the link resolves from the
/// runtime static library.
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program;
/// [`RunError::Internal`] when the triple is unknown, unsupported by
/// the pinned Cranelift, or the lowering fails.
pub fn emit_object(files: &[SourceFile], triple: Option<&str>) -> Result<AotObject, RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;
    let flags = aot_flags().map_err(RunError::Internal)?;

    let (isa, triple_name, is_macho) = match triple {
        Some(name) => {
            let parsed = Triple::from_str(name)
                .map_err(|e| RunError::Internal(internal(format!("target triple {name}: {e}"))))?;
            let is_macho = parsed.operating_system.to_string().contains("ios")
                || parsed.operating_system.to_string().contains("darwin");
            let isa = cranelift_codegen::isa::lookup(parsed)
                .map_err(|e| RunError::Internal(internal(format!("ISA for {name}: {e}"))))?
                .finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))?;
            (isa, name.to_string(), is_macho)
        }
        None => {
            let builder = cranelift_native::builder()
                .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))?;
            let isa = builder
                .finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))?;
            let name = isa.triple().to_string();
            (isa, name, false)
        }
    };

    let builder = ObjectBuilder::new(isa, "subscript", cranelift_module::default_libcall_names())
        .map_err(|e| RunError::Internal(internal(format!("object builder: {e}"))))?;
    let mut module = ObjectModule::new(builder);
    let lowered = lower_module_with(&mut module, &hir, LowerOptions::default())
        .map_err(RunError::Internal)?;

    let mut product = module.finish();
    if is_macho && triple_name.contains("ios") {
        let mut version = MachOBuildVersion::default();
        version.platform = PLATFORM_IOS;
        version.minos = MACHO_VERSION_10_0_0;
        version.sdk = MACHO_VERSION_10_0_0;
        product.object.set_macho_build_version(version);
    }
    let bytes = product
        .emit()
        .map_err(|e| RunError::Internal(internal(format!("emit object: {e}"))))?;
    Ok(AotObject {
        triple: triple_name,
        bytes,
        positions: lowered.positions,
        foreign_symbols: lowered.foreign_symbols,
    })
}

/// The C entry program linked with every AOT build, host or device.
///
/// It is the single definition of the entry: [`run_aot`] writes it into
/// its temporary directory, and the device-triple link script writes it
/// through the `emit-object` binary. It is generated output, never
/// hand-edited in place.
pub const AOT_ENTRY_C: &str = concat!(
    include_str!("../../runtime/include/subscript_runtime.h"),
    r#"

/* Host entry for a subscript AOT build (compiler.md 8.1).
 * Generated; never hand-edited.
 */
#include <stdio.h>
#include <stddef.h>
#if defined(_WIN32)
#include <io.h>
#include <fcntl.h>
#endif

/* Generated by both object lowering and the C ship tier. */
extern void subscript_kick_async_exports(subscript_rt_context *ctx);

static void call_script_entry(subscript_rt_context *ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

static void write_stdout_line(void *userdata, const uint8_t *line, uint64_t line_len) {
    FILE *stream = (FILE *)userdata;
    if (line_len > 0) {
        fwrite(line, 1, (size_t)line_len, stream);
    }
    fputc('\n', stream);
    fflush(stream);
}

int main(void) {
#if defined(_WIN32)
    /* The sink bytes are compared byte-for-byte against the goldens; the
     * MSVCRT opens stdout in text mode and would translate '\n' to
     * '\r\n'. Binary mode writes the sink through unchanged. No-op on
     * every other platform, which has no text-mode translation. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    subscript_rt_context *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    subscript_rt_ctx_set_print_observer(ctx, write_stdout_line, stdout);
    call_script_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) {
        call_script_entry(ctx, subscript_export_main);
    }
    if (subscript_rt_ctx_trap_kind(ctx) == 0) {
        call_script_entry(ctx, subscript_kick_async_exports);
    }
    while (subscript_rt_ctx_trap_kind(ctx) == 0 &&
           subscript_rt_ctx_async_pending(ctx) != 0) {
        (void)subscript_rt_ctx_async_step(ctx);
    }
    uint64_t len = 0;
    const unsigned char *out = subscript_rt_ctx_stdout(ctx, &len);
    if (len > 0) {
        fwrite(out, 1, (size_t)len, stdout);
    }
    fflush(stdout);
    int status = 0;
    uint32_t kind = subscript_rt_ctx_trap_kind(ctx);
    if (kind != 0) {
        uint64_t mlen = 0;
        const unsigned char *msg = subscript_rt_ctx_trap_message(ctx, &mlen);
        fprintf(stderr, "trap %u %u ", kind, subscript_rt_ctx_trap_pos_id(ctx));
        if (mlen > 0) {
            fwrite(msg, 1, (size_t)mlen, stderr);
        }
        fputc('\n', stderr);
        status = 3;
    }
    subscript_rt_ctx_release(ctx);
    return status;
}
"#
);

/// Generated host header consumed by the standard and test AOT entries.
pub const HOST_HEADER_C: &str = include_str!("../../runtime/include/subscript_runtime.h");

/// A temporary directory removed when the guard is dropped.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Result<TempDir, RunError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "subscript-aot-{}-{}-{}",
            std::process::id(),
            tag,
            n
        ));
        std::fs::create_dir_all(&path)
            .map_err(|e| RunError::Internal(internal(format!("temp dir: {e}"))))?;
        Ok(TempDir { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort: a leftover temp directory is not a failure of
        // the run that produced it.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The directory holding build outputs for the running profile
/// (`.../target/<profile>`), derived from the current executable.
fn build_dir() -> Result<PathBuf, RunError> {
    let exe = std::env::current_exe()
        .map_err(|e| RunError::Internal(internal(format!("current executable: {e}"))))?;
    // Test and example binaries live in `<profile>/deps/`; plain
    // binaries live directly in `<profile>/`.
    let dir = exe
        .parent()
        .ok_or_else(|| RunError::Internal(internal("executable has no directory")))?;
    let dir = if dir.file_name().is_some_and(|n| n == "deps") {
        dir.parent()
            .ok_or_else(|| RunError::Internal(internal("deps directory has no parent")))?
    } else {
        dir
    };
    Ok(dir.to_path_buf())
}

/// Newest modification time under `dir`, recursively. `None` when the
/// tree cannot be read; the caller then treats the archive as stale.
fn newest_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let meta = std::fs::metadata(&p).ok()?;
        if meta.is_dir() {
            for e in std::fs::read_dir(&p).ok()? {
                stack.push(e.ok()?.path());
            }
        } else if let Ok(t) = meta.modified() {
            newest = Some(newest.map_or(t, |n| n.max(t)));
        }
    }
    newest
}

/// True when `archive` exists and is newer than every runtime source.
fn staticlib_is_fresh(archive: &Path, runtime_dir: &Path) -> bool {
    let Ok(built) = std::fs::metadata(archive).and_then(|m| m.modified()) else {
        return false;
    };
    match newest_mtime(runtime_dir) {
        Some(source) => built >= source,
        None => false,
    }
}

/// Returns the host runtime static-library filename produced by Cargo.
///
/// Windows MSVC uses `subscript_runtime.lib`; every other host uses
/// `libsubscript_runtime.a`.
#[must_use]
pub fn runtime_staticlib_name() -> &'static str {
    if cfg!(all(windows, target_env = "msvc")) {
        "subscript_runtime.lib"
    } else {
        "libsubscript_runtime.a"
    }
}

/// Builds the runtime static library for the running profile and
/// returns its path.
fn build_runtime_staticlib() -> Result<PathBuf, String> {
    let dir = build_dir().map_err(|e| e.to_string())?;
    let archive = dir.join(runtime_staticlib_name());
    let runtime_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    if staticlib_is_fresh(&archive, &runtime_dir) {
        return Ok(archive);
    }
    let profile = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("build")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(runtime_dir.join("Cargo.toml"));
    if profile == "release" {
        cmd.arg("--release");
    }
    let out = cmd
        .output()
        .map_err(|e| internal(format!("build runtime staticlib: {e}")))?;
    if !out.status.success() {
        return Err(internal(format!(
            "building the runtime static library failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    if !archive.is_file() {
        return Err(internal(format!(
            "runtime static library not found at {}; set {RUNTIME_STATICLIB_ENV}",
            archive.display()
        )));
    }
    Ok(archive)
}

/// Locates the runtime static library, building it if necessary.
///
/// The runtime crate declares both `rlib` and `staticlib`; `cargo test`
/// builds only the `rlib` it links against, so the archive is produced
/// on demand with the same cargo and the same profile. Resolution is
/// cached per process and the archive is reused when it is newer than
/// every runtime source, so a test binary that links many programs
/// spawns cargo at most once, and not at all when the archive is
/// already current. Set [`RUNTIME_STATICLIB_ENV`] to bypass all of it.
fn runtime_staticlib() -> Result<PathBuf, RunError> {
    if let Some(p) = std::env::var_os(RUNTIME_STATICLIB_ENV) {
        let path = PathBuf::from(p);
        if !path.is_file() {
            return Err(RunError::Internal(internal(format!(
                "{RUNTIME_STATICLIB_ENV} points at {}, which is not a file",
                path.display()
            ))));
        }
        return Ok(path);
    }
    static RESOLVED: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    RESOLVED
        .get_or_init(build_runtime_staticlib)
        .clone()
        .map_err(RunError::Internal)
}

/// Path to the runtime static library for the running build profile,
/// building it with the workspace's own cargo when it is missing or
/// older than the runtime sources.
///
/// [`run_aot`] uses it for its own link; it is public so that a host
/// program which links an [`emit_object`] result with an entry of its
/// own — the P4 benchmark harness is the one in-tree case — resolves
/// the same archive the differential gate links.
/// [`RUNTIME_STATICLIB_ENV`] overrides it.
///
/// # Errors
///
/// [`RunError::Internal`] when the archive cannot be located or built.
pub fn runtime_staticlib_path() -> Result<PathBuf, RunError> {
    runtime_staticlib()
}

/// Finds `name` on `PATH`, returning its full path when an executable
/// file exists. On Windows the `.exe` extension is tried as well.
#[cfg(not(all(windows, target_env = "msvc")))]
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &["", ".exe"] } else { &[""] };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let cand = dir.join(format!("{name}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Command-line spelling used by a host C compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CCompilerStyle {
    /// Clang/GCC-compatible command-line spelling.
    Unix,
    /// Microsoft `cl` command-line spelling.
    Msvc,
}

impl CCompilerStyle {
    /// Returns whether this is Microsoft `cl` spelling.
    #[must_use]
    pub fn is_msvc(self) -> bool {
        matches!(self, Self::Msvc)
    }
}

/// The resolved host C compiler for a ship-tier compile/link.
///
/// It carries the program, the environment needed to run it, and the
/// command-line style selected for the host.
#[derive(Debug)]
#[non_exhaustive]
pub struct HostCCompiler {
    program: OsString,
    env: Vec<(OsString, OsString)>,
    style: CCompilerStyle,
}

impl HostCCompiler {
    /// A [`Command`] for the resolved compiler with its toolchain
    /// environment applied. MSVC `cl` cannot find its own headers and
    /// import libraries without the `INCLUDE`/`LIB`/`PATH` the registry
    /// lookup supplies, so that environment travels with the program.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.envs(self.env.iter().cloned());
        command
    }

    /// Returns the resolved compiler executable.
    #[must_use]
    pub fn program(&self) -> &std::ffi::OsStr {
        &self.program
    }

    /// Returns the compiler's command-line style.
    #[must_use]
    pub fn style(&self) -> CCompilerStyle {
        self.style
    }
}

/// Resolves the platform C compiler used for ship-tier compilation and
/// linking. `$CC` is honored verbatim on every target. The default is
/// MSVC `cl` with its discovered toolchain environment on windows-msvc
/// (compiler.md §11c), and clang on every other host (§11/§11b).
///
/// # Errors
///
/// [`RunError::Internal`] on windows-msvc when neither `$CC` is set nor
/// the MSVC toolchain can be located; a missing toolchain is a failure,
/// never a skip — the gate machine is the development machine (§8.3).
pub fn host_c_compiler() -> Result<HostCCompiler, RunError> {
    if let Some(cc) = std::env::var_os("CC") {
        return Ok(HostCCompiler {
            program: cc,
            env: Vec::new(),
            style: if cfg!(all(windows, target_env = "msvc")) {
                CCompilerStyle::Msvc
            } else {
                CCompilerStyle::Unix
            },
        });
    }

    #[cfg(all(windows, target_env = "msvc"))]
    {
        let target = target_lexicon::HOST.to_string();
        let tool = cc::windows_registry::find_tool(&target, "cl.exe").ok_or_else(|| {
            RunError::Internal(internal(format!(
                "MSVC `cl.exe` was not found for target {target}; \
                 install the Visual C++ build tools or set $CC \
                 (compiler.md §11c)"
            )))
        })?;
        Ok(HostCCompiler {
            program: tool.path().as_os_str().to_owned(),
            env: tool.env().to_vec(),
            style: CCompilerStyle::Msvc,
        })
    }

    #[cfg(not(all(windows, target_env = "msvc")))]
    {
        Ok(HostCCompiler {
            program: find_on_path("clang")
                .map(PathBuf::into_os_string)
                .unwrap_or_else(|| "clang".into()),
            env: Vec::new(),
            style: CCompilerStyle::Unix,
        })
    }
}

/// Concatenates a flag `prefix` with a path, as MSVC's `/Fo:` / `/Fe:`
/// output flags require (no separating space between flag and value).
fn prefixed_path_arg(prefix: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(prefix);
    arg.push(path.as_os_str());
    arg
}

/// The MSVC `/Fo:` object-output flag pointing at a directory: a
/// trailing path separator tells `cl` to place each object there under
/// the source basename, which is required when more than one source is
/// compiled in a single invocation.
fn msvc_object_directory_arg(directory: &Path) -> OsString {
    let mut arg = prefixed_path_arg("/Fo:", directory);
    arg.push(std::path::MAIN_SEPARATOR.to_string());
    arg
}

/// Adds the pinned C11 optimization flags for the resolved driver. The
/// emitted ship C targets C11 (compiler.md §11); pinning the dialect
/// keeps the ship tier independent of the platform compiler's default
/// `-std`. On MSVC (§11c) `-fwrapv` is dropped — `cl` wraps signed
/// overflow two's-complement — and `/utf-8` reads the UTF-8 sources
/// without warning C4819. `/fp:strict` replaces `-ffp-contract=off`: it
/// forbids contraction and reassociation as `-ffp-contract=off` does, and
/// additionally stops `cl` from constant-folding an emitted IEEE
/// `1.0 / 0.0` (infinity) at compile time — which `/fp:precise` rejects
/// with error C2124 — deferring it to a runtime infinity instead. It is
/// at least as conservative as `/fp:precise`, so the byte-exact
/// differential is preserved.
pub fn add_c11_optimized_flags(command: &mut Command, style: CCompilerStyle) {
    if style.is_msvc() {
        command.args(["/nologo", "/std:c11", "/O2", "/utf-8", "/fp:strict"]);
    } else {
        command.args(["-std=c11", "-O2", "-fwrapv", "-ffp-contract=off"]);
    }
}

/// Adds the AddressSanitizer flags for the resolved driver.
fn add_address_sanitizer_flags(command: &mut Command, style: CCompilerStyle) {
    if style.is_msvc() {
        command.args(["/fsanitize=address", "/Oy-"]);
    } else {
        command.args(["-fsanitize=address", "-fno-omit-frame-pointer"]);
    }
}

/// Renders the output of a failed toolchain command for an error
/// message, with both streams and a label for each.
///
/// MSVC `cl` and `link.exe` write their diagnostics to stdout. Unix
/// compilers write them to stderr. A report of one stream only is
/// therefore empty on one host family, and the failure arrives with no
/// cause (compiler.md §11c.4). An empty stream is normal, so this
/// function drops it and keeps the label of the stream that has content.
#[must_use]
pub fn tool_output_report(output: &std::process::Output) -> String {
    let mut report = String::new();
    for (label, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
        let stream = String::from_utf8_lossy(bytes);
        if stream.trim().is_empty() {
            continue;
        }
        report.push_str("--- ");
        report.push_str(label);
        report.push_str(" ---\n");
        report.push_str(&stream);
        if !stream.ends_with('\n') {
            report.push('\n');
        }
    }
    if report.is_empty() {
        report.push_str("(the command printed nothing)\n");
    }
    report
}

/// Adds the executable-output arguments for the resolved driver: MSVC's
/// `/Fe:` naming plus the `-link` linker-arguments marker, or the
/// GNU-style `-o` form.
pub fn add_executable_output(command: &mut Command, executable: &Path, style: CCompilerStyle) {
    if style.is_msvc() {
        command
            .arg(prefixed_path_arg("/Fe:", executable))
            .arg("-link");
    } else {
        command.arg("-o").arg(executable);
    }
}

/// The object-file path for `stem` in `directory`, with the platform's
/// object extension (`.obj` on windows-msvc, `.o` elsewhere).
fn object_path(directory: &Path, stem: &str) -> PathBuf {
    let extension = if cfg!(all(windows, target_env = "msvc")) {
        "obj"
    } else {
        "o"
    };
    directory.join(format!("{stem}.{extension}"))
}

/// Windows system libraries required when linking the Rust runtime archive.
pub const WINDOWS_SYSTEM_LIBRARIES: &[&str] =
    &["kernel32", "ntdll", "userenv", "ws2_32", "dbghelp"];

/// Returns the host system-library arguments for `style`.
///
/// The list is empty off Windows. On Windows it uses `name.lib` for
/// Microsoft `cl` and `-lname` for a Unix-style driver. The runtime embeds
/// Rust `std`, whose imports require the five libraries that `rustc` would
/// otherwise pass automatically.
#[must_use]
pub fn runtime_system_libraries(style: CCompilerStyle) -> Vec<OsString> {
    system_library_arguments(cfg!(windows), style)
}

fn system_library_arguments(windows: bool, style: CCompilerStyle) -> Vec<OsString> {
    if !windows {
        return Vec::new();
    }
    WINDOWS_SYSTEM_LIBRARIES
        .iter()
        .map(|name| {
            if style.is_msvc() {
                OsString::from(format!("{name}.lib"))
            } else {
                OsString::from(format!("-l{name}"))
            }
        })
        .collect()
}

/// Returns one joined include-directory argument in the selected spelling.
#[must_use]
pub fn include_directory_arg(style: CCompilerStyle, directory: &Path) -> OsString {
    let prefix = if style.is_msvc() { "/I" } else { "-I" };
    prefixed_path_arg(prefix, directory)
}

/// Directs compiler-created object files into `directory` when required.
///
/// This adds MSVC's joined `/Fo:<dir>\` argument and is a no-op for a
/// Unix-style compiler, which writes no intermediate object in a one-shot
/// compile-and-link command.
pub fn add_object_directory(command: &mut Command, directory: &Path, style: CCompilerStyle) {
    if style.is_msvc() {
        command.arg(msvc_object_directory_arg(directory));
    }
}

fn require_native_symbols(
    required: &[String],
    libraries: &[NativeLibrary],
) -> Result<(), RunError> {
    match missing_symbol(required, libraries) {
        Some(name) => Err(RunError::UnresolvedForeignSymbol(name.to_string())),
        None => Ok(()),
    }
}

fn add_native_compile_inputs(
    command: &mut Command,
    libraries: &[NativeLibrary],
    style: CCompilerStyle,
) {
    for library in libraries {
        for directory in library.include_directories() {
            command.arg(include_directory_arg(style, directory));
        }
    }
    for library in libraries {
        for source in library.c_sources() {
            command.arg(source);
        }
    }
}

/// Writes `bytes` to `path`.
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), RunError> {
    let mut f = std::fs::File::create(path)
        .map_err(|e| RunError::Internal(internal(format!("create {}: {e}", path.display()))))?;
    f.write_all(bytes)
        .map_err(|e| RunError::Internal(internal(format!("write {}: {e}", path.display()))))
}

/// Compiles, links, and runs `files` through the ship tier on the host
/// target, returning the exact stdout bytes the program produced.
///
/// The whole cycle happens in a temporary directory that is removed
/// before returning. Linking uses clang (resolved by
/// [`host_c_compiler`], or `$CC`); its absence is a failure, never a
/// skip — the differential gate machine is the development machine
/// (`specs/blocks/compiler.md` §8.3).
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the linked program trapped (the trap is
/// reported by the entry program and mapped back through the position
/// table), [`RunError::UnresolvedForeignSymbol`] when the program calls a
/// symbol but no native library was supplied,
/// [`RunError::AbnormalTermination`] when the linked program ends outside
/// the trap protocol, and [`RunError::Internal`] on emission, toolchain,
/// link, or execution failures.
pub fn run_aot(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    run_aot_with_native_libraries(files, &[])
}

/// Compiles, links, and runs `files` through the retained Cranelift-object
/// AOT cross-check with caller-supplied native libraries.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_aot`], including
/// [`RunError::UnresolvedForeignSymbol`] when a called foreign symbol is
/// absent from `libraries`.
pub fn run_aot_with_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    let object = emit_object(files, None)?;
    require_native_symbols(&object.foreign_symbols, libraries)?;
    let staticlib = runtime_staticlib()?;
    let dir = TempDir::new("run")?;

    let obj_path = object_path(&dir.path, "program");
    let entry_path = dir.path.join("entry.c");
    let exe_path = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&obj_path, &object.bytes)?;
    write_file(&entry_path, AOT_ENTRY_C.as_bytes())?;

    let cc = host_c_compiler()?;
    let mut command = cc.command();
    if cc.style().is_msvc() {
        // The C entry is the only source compiled here; direct its
        // object into the temp dir so `cl` does not litter the cwd.
        add_c11_optimized_flags(&mut command, cc.style());
        add_object_directory(&mut command, &dir.path, cc.style());
    }
    add_native_compile_inputs(&mut command, libraries, cc.style());
    command
        .arg(&entry_path)
        .arg(&obj_path)
        .arg(&staticlib)
        .args(runtime_system_libraries(cc.style()));
    add_executable_output(&mut command, &exe_path, cc.style());
    let link = command.output().map_err(|e| {
        RunError::Internal(internal(format!(
            "the platform C compiler `{}` could not be run: {e}; \
                 install the host C toolchain or set $CC (compiler.md §11c)",
            cc.program.to_string_lossy()
        )))
    })?;
    if !link.status.success() {
        return Err(RunError::Internal(internal(format!(
            "link failed:\n{}",
            tool_output_report(&link)
        ))));
    }

    let run = Command::new(&exe_path)
        .output()
        .map_err(|e| RunError::Internal(internal(format!("run linked program: {e}"))))?;
    if run.status.success() {
        return Ok(run.stdout);
    }
    match parse_trap(&run.stderr, &object.positions, &run.stdout) {
        Some(report) => Err(RunError::Trap(report)),
        None => Err(RunError::AbnormalTermination(AbnormalTermination {
            status: format!("linked program exited with {}", run.status),
            stdout: run.stdout,
            stderr: run.stderr,
        })),
    }
}

/// Compiles, links, and runs `files` through the **ship tier** (§11) on
/// the host target, returning the exact stdout bytes the program
/// produced.
///
/// The ship tier emits C ([`crate::emit_c`]), which the platform C
/// compiler compiles at `-O2 -ffp-contract=off` and links with the
/// runtime static library and the same host entry [`AOT_ENTRY_C`] the
/// Cranelift AOT path uses — the emitted C exports the identical
/// `subscript_init` / `subscript_export_main` surface, so it is a drop-in subject.
/// The whole cycle happens in a temporary directory removed before
/// returning. The C compiler's absence is a failure, never a skip (the
/// gate machine is the development machine, §8.3).
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the linked program trapped (mapped back
/// through the emitted position table, with pre-trap stdout),
/// [`RunError::UnresolvedForeignSymbol`] when the program calls a symbol
/// but no native library was supplied, [`RunError::AbnormalTermination`]
/// when the linked program ends outside the trap protocol, and
/// [`RunError::Internal`] on emission, toolchain, compile, link, or
/// execution failures.
pub fn run_c_aot(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, None, false, &[], None, None)
}

/// Compiles, links, and runs `files` through the emitted-C ship tier with
/// caller-supplied native libraries.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_c_aot`], including
/// [`RunError::UnresolvedForeignSymbol`] when a called foreign symbol is
/// absent from `libraries`.
pub fn run_c_aot_with_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, None, false, libraries, None, None)
}

/// Compiles, links, and runs `files` through the emitted-C ship tier with
/// caller-supplied native libraries and optional host lifecycle hooks.
///
/// `pre_entry_hook`, when present, names a C function called after
/// `subscript_init` and before the exported script entry. `post_run_hook`,
/// when present, names a C function called after the async pump and before
/// the Context is released. Each hook has the C signature
/// `void hook(subscript_rt_context *)`; hook names must be C identifiers.
/// Neither hook is entered as script code. Both calls are unconditional:
/// the pre-entry hook runs even when initialization has trapped, and the
/// post-run hook runs regardless of the Context's trap state.
/// The generated entry declares and calls only the hooks supplied here, so
/// no weak-symbol or link-time discovery mechanism is involved.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as
/// [`run_c_aot_with_native_libraries`]. [`RunError::Internal`] is returned
/// when a supplied hook name is not a C identifier.
pub fn run_c_aot_with_native_libraries_and_host_hooks(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
    pre_entry_hook: Option<&str>,
    post_run_hook: Option<&str>,
) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(
        files,
        None,
        false,
        libraries,
        pre_entry_hook,
        post_run_hook,
    )
}

/// Runs the emitted-C ship tier with freed-handle diagnostics enabled and
/// the caller-supplied native libraries.
///
/// The host establishes threshold 0 and the recommended 1 GiB retention
/// budget before `subscript_init`, so retained-dead diagnostics have the
/// same coverage as the dev-JIT diagnostic runner.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as
/// [`run_c_aot_with_native_libraries`].
pub fn run_c_aot_with_freed_handle_diagnostics_and_native_libraries(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, None, true, libraries, None, None)
}

/// Runs the emitted-C ship tier while refusing the `n`-th object-level
/// Context allocation after Context creation.
///
/// The injected fault is armed before `subscript_init`, so module-initializer
/// allocations are part of the count. When
/// `SUBSCRIPT_C_AOT_ASAN` is set, the generated C and host entry are
/// compiled and linked with AddressSanitizer.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_c_aot`].
pub fn run_c_aot_with_alloc_failure(files: &[SourceFile], n: u64) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, Some(n), false, &[], None, None)
}

fn validate_host_hook_name(name: &str) -> Result<(), RunError> {
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic());
    if !valid_start || !bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric()) {
        return Err(RunError::Internal(internal(format!(
            "host hook name `{name}` is not a C identifier"
        ))));
    }
    Ok(())
}

fn aot_entry_with_host_hooks(
    pre_entry_hook: Option<&str>,
    post_run_hook: Option<&str>,
) -> Result<String, RunError> {
    for hook in [pre_entry_hook, post_run_hook].into_iter().flatten() {
        validate_host_hook_name(hook)?;
    }
    if pre_entry_hook.is_none() && post_run_hook.is_none() {
        return Ok(AOT_ENTRY_C.to_string());
    }

    const DECLARATION_ANCHOR: &str =
        "extern void subscript_kick_async_exports(subscript_rt_context *ctx);";
    const PRE_ENTRY_ANCHOR: &str = "    call_script_entry(ctx, subscript_init);";
    const POST_RUN_ANCHOR: &str = "    uint64_t len = 0;";
    for anchor in [DECLARATION_ANCHOR, PRE_ENTRY_ANCHOR, POST_RUN_ANCHOR] {
        if !AOT_ENTRY_C.contains(anchor) {
            return Err(RunError::Internal(internal(
                "AOT entry host-hook anchor moved",
            )));
        }
    }

    let mut declarations = String::new();
    if let Some(hook) = pre_entry_hook {
        declarations.push_str(&format!(
            "\nextern void {hook}(subscript_rt_context *ctx);"
        ));
    }
    if let Some(hook) = post_run_hook.filter(|hook| Some(*hook) != pre_entry_hook) {
        declarations.push_str(&format!(
            "\nextern void {hook}(subscript_rt_context *ctx);"
        ));
    }

    let mut entry = AOT_ENTRY_C.replacen(
        DECLARATION_ANCHOR,
        &format!("{DECLARATION_ANCHOR}{declarations}"),
        1,
    );
    if let Some(hook) = pre_entry_hook {
        entry = entry.replacen(
            PRE_ENTRY_ANCHOR,
            &format!("{PRE_ENTRY_ANCHOR}\n    {hook}(ctx);"),
            1,
        );
    }
    if let Some(hook) = post_run_hook {
        entry = entry.replacen(
            POST_RUN_ANCHOR,
            &format!("    {hook}(ctx);\n{POST_RUN_ANCHOR}"),
            1,
        );
    }
    Ok(entry)
}

fn run_c_aot_configured(
    files: &[SourceFile],
    fail_alloc_after: Option<u64>,
    freed_handle_diagnostics: bool,
    libraries: &[NativeLibrary],
    pre_entry_hook: Option<&str>,
    post_run_hook: Option<&str>,
) -> Result<Vec<u8>, RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;
    let program = crate::emit_c(&hir).map_err(|e| RunError::Internal(internal(e)))?;
    require_native_symbols(&program.foreign_symbols, libraries)?;
    let staticlib = runtime_staticlib()?;
    let dir = TempDir::new("crun")?;

    let src_path = dir.path.join("program.c");
    let entry_path = dir.path.join("entry.c");
    let exe_path = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&src_path, program.source.as_bytes())?;
    let anchor = "    call_script_entry(ctx, subscript_init);";
    let mut entry = aot_entry_with_host_hooks(pre_entry_hook, post_run_hook)?;
    if !entry.contains(anchor) {
        return Err(RunError::Internal(internal(
            "AOT entry Context-configuration anchor moved",
        )));
    }
    let mut setup = String::new();
    if freed_handle_diagnostics {
        setup.push_str(concat!(
            "    if (subscript_rt_ctx_set_freed_handle_diagnostics(\n",
            "            ctx, 1u, 0u,\n",
            "            SUBSCRIPT_RT_FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES) == 0) {\n",
            "        subscript_rt_ctx_release(ctx);\n",
            "        return 2;\n",
            "    }\n",
        ));
    }
    if let Some(n) = fail_alloc_after {
        setup.push_str(&format!(
            "    subscript_rt_ctx_fail_alloc_after(ctx, {n}u);\n"
        ));
    }
    if !setup.is_empty() {
        setup.push_str(anchor);
        entry = entry.replacen(anchor, &setup, 1);
    }
    write_file(&entry_path, entry.as_bytes())?;

    let cc = host_c_compiler()?;
    let address_sanitizer = std::env::var_os("SUBSCRIPT_C_AOT_ASAN").is_some();
    let mut command = cc.command();
    // The emitted ship C targets C11 (compiler block §11); the language's
    // C-ABI layout, compound literals, `_Alignof`, and
    // `<stdbool.h>`/`<stdint.h>` types are all C11. Pinning the dialect
    // keeps the ship tier independent of the platform compiler's default
    // `-std`. Signed overflow must wrap two's-complement (the language's
    // semantics); `-fwrapv` establishes that off MSVC, `cl` wraps by
    // default (§11c).
    add_c11_optimized_flags(&mut command, cc.style());
    if address_sanitizer {
        add_address_sanitizer_flags(&mut command, cc.style());
    }
    if cc.style().is_msvc() {
        // Two sources (the program and the entry) plus any native
        // sources are compiled in one invocation; direct their objects
        // into the temp dir under their basenames.
        add_object_directory(&mut command, &dir.path, cc.style());
    }
    add_native_compile_inputs(&mut command, libraries, cc.style());
    command
        .arg(&src_path)
        .arg(&entry_path)
        .arg(&staticlib)
        .args(runtime_system_libraries(cc.style()));
    add_executable_output(&mut command, &exe_path, cc.style());
    let compile = command.output().map_err(|e| {
        RunError::Internal(internal(format!(
            "the platform C compiler `{}` could not be run: {e}; \
                 install the host C toolchain or set $CC (compiler.md §11c)",
            cc.program.to_string_lossy()
        )))
    })?;
    if !compile.status.success() {
        return Err(RunError::Internal(internal(format!(
            "compiling/linking the emitted C failed:\n{}",
            tool_output_report(&compile)
        ))));
    }

    let run = Command::new(&exe_path)
        .output()
        .map_err(|e| RunError::Internal(internal(format!("run linked program: {e}"))))?;
    if run.status.success() {
        return Ok(run.stdout);
    }
    match parse_trap(&run.stderr, &program.positions, &run.stdout) {
        Some(report) => Err(RunError::Trap(report)),
        None => Err(RunError::AbnormalTermination(AbnormalTermination {
            status: format!("linked C program exited with {}", run.status),
            stdout: run.stdout,
            stderr: run.stderr,
        })),
    }
}

/// Parses the entry program's `trap <kind> <pos_id> <message>` line
/// back into a report, resolving the position through the table the
/// lowering produced for this object.
fn parse_trap(stderr: &[u8], positions: &[Pos], stdout: &[u8]) -> Option<TrapReport> {
    let text = std::str::from_utf8(stderr).ok()?;
    let line = text.lines().find(|l| l.starts_with("trap "))?;
    let mut parts = line.splitn(4, ' ');
    parts.next()?;
    let kind = TrapKind::from_u32(parts.next()?.parse().ok()?)?;
    let pos_id: usize = parts.next()?.parse().ok()?;
    let message = parts.next().unwrap_or("").to_string();
    Some(TrapReport {
        rule: kind,
        message,
        pos: positions
            .get(pos_id)
            .cloned()
            .unwrap_or_else(|| Pos::new(String::new(), 0, 0)),
        stdout: stdout.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::{Architecture, BinaryFormat, Object, ObjectSymbol};

    fn sources(src: &str) -> Vec<SourceFile> {
        vec![SourceFile::new("test.ts", src)]
    }

    #[test]
    fn aot_entry_without_host_hooks_is_byte_identical_to_the_standing_entry() {
        let generated = aot_entry_with_host_hooks(None, None).expect("generate entry");
        assert_eq!(generated.as_bytes(), AOT_ENTRY_C.as_bytes());
    }

    #[test]
    fn aot_entry_host_hooks_are_optional_and_independent() {
        const PRE: &str = "fixture_pre_entry";
        const POST: &str = "fixture_post_run";
        for (pre, post) in [
            (None, None),
            (Some(PRE), None),
            (None, Some(POST)),
            (Some(PRE), Some(POST)),
        ] {
            let entry = aot_entry_with_host_hooks(pre, post).expect("generate entry");
            assert_eq!(entry.contains(&format!("extern void {PRE}(")), pre.is_some());
            assert_eq!(entry.contains(&format!("    {PRE}(ctx);")), pre.is_some());
            assert_eq!(
                entry.contains(&format!("extern void {POST}(")),
                post.is_some()
            );
            assert_eq!(
                entry.contains(&format!("    {POST}(ctx);")),
                post.is_some()
            );
        }

        let entry = aot_entry_with_host_hooks(Some(PRE), Some(POST)).expect("generate entry");
        let init = entry
            .find("    call_script_entry(ctx, subscript_init);")
            .expect("initializer call");
        let pre = entry.find("    fixture_pre_entry(ctx);").expect("pre hook");
        let main_guard = entry
            .find("    if (subscript_rt_ctx_trap_kind(ctx) == 0) {")
            .expect("main trap guard");
        assert!(init < pre && pre < main_guard);

        let pump = entry
            .find("    while (subscript_rt_ctx_trap_kind(ctx) == 0 &&")
            .expect("async pump");
        let post = entry.find("    fixture_post_run(ctx);").expect("post hook");
        let output = entry.find("    uint64_t len = 0;").expect("output capture");
        let release = entry
            .find("    subscript_rt_ctx_release(ctx);")
            .expect("Context release");
        assert!(pump < post && post < output && output < release);
    }

    #[test]
    fn aot_entry_rejects_non_identifier_host_hook_names() {
        assert!(matches!(
            aot_entry_with_host_hooks(Some("bad-hook"), None),
            Err(RunError::Internal(message)) if message.contains("not a C identifier")
        ));
    }

    /// Builds an `Output` with the two streams. [`tool_output_report`]
    /// reads no status, so the default status is enough.
    fn tool_output(stdout: &str, stderr: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn tool_output_report_keeps_the_stream_that_spoke() {
        // MSVC `cl` reports on stdout; a stderr-only report loses this.
        let msvc = tool_output("host.h(12): error C2146: syntax error\n", "");
        assert_eq!(
            tool_output_report(&msvc),
            "--- stdout ---\nhost.h(12): error C2146: syntax error\n"
        );

        let unix = tool_output("", "host.h:12:5: error: expected ';'\n");
        assert_eq!(
            tool_output_report(&unix),
            "--- stderr ---\nhost.h:12:5: error: expected ';'\n"
        );
    }

    #[test]
    fn tool_output_report_labels_both_streams_and_ends_every_line() {
        let both = tool_output("out line", "err line");
        assert_eq!(
            tool_output_report(&both),
            "--- stdout ---\nout line\n--- stderr ---\nerr line\n"
        );
    }

    #[test]
    fn tool_output_report_names_a_silent_command() {
        let silent = tool_output("", "   \n");
        assert_eq!(
            tool_output_report(&silent),
            "(the command printed nothing)\n"
        );
    }

    #[test]
    fn public_toolchain_api_carries_the_ship_contract() -> Result<(), String> {
        assert!(!CCompilerStyle::Unix.is_msvc());
        assert!(CCompilerStyle::Msvc.is_msvc());

        let mut unix = Command::new("cc");
        add_c11_optimized_flags(&mut unix, CCompilerStyle::Unix);
        add_object_directory(&mut unix, Path::new("objects"), CCompilerStyle::Unix);
        add_executable_output(&mut unix, Path::new("program"), CCompilerStyle::Unix);
        assert_eq!(
            unix.get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            [
                "-std=c11",
                "-O2",
                "-fwrapv",
                "-ffp-contract=off",
                "-o",
                "program",
            ]
        );
        assert_eq!(
            include_directory_arg(CCompilerStyle::Unix, Path::new("include")),
            OsString::from("-Iinclude")
        );

        let mut msvc = Command::new("cl");
        add_c11_optimized_flags(&mut msvc, CCompilerStyle::Msvc);
        add_object_directory(&mut msvc, Path::new("objects"), CCompilerStyle::Msvc);
        add_executable_output(&mut msvc, Path::new("program.exe"), CCompilerStyle::Msvc);
        let msvc_args = msvc
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            &msvc_args[..5],
            ["/nologo", "/std:c11", "/O2", "/utf-8", "/fp:strict"]
        );
        assert!(msvc_args[5].starts_with("/Fo:objects"));
        assert_eq!(&msvc_args[6..], ["/Fe:program.exe", "-link"]);
        assert_eq!(
            include_directory_arg(CCompilerStyle::Msvc, Path::new("include")),
            OsString::from("/Iinclude")
        );

        assert!(matches!(
            runtime_staticlib_name(),
            "libsubscript_runtime.a" | "subscript_runtime.lib"
        ));
        assert_eq!(
            WINDOWS_SYSTEM_LIBRARIES,
            ["kernel32", "ntdll", "userenv", "ws2_32", "dbghelp"]
        );
        assert_eq!(
            system_library_arguments(true, CCompilerStyle::Msvc),
            [
                "kernel32.lib",
                "ntdll.lib",
                "userenv.lib",
                "ws2_32.lib",
                "dbghelp.lib",
            ]
            .map(OsString::from)
        );
        assert_eq!(
            system_library_arguments(true, CCompilerStyle::Unix),
            [
                "-lkernel32",
                "-lntdll",
                "-luserenv",
                "-lws2_32",
                "-ldbghelp",
            ]
            .map(OsString::from)
        );
        assert_eq!(
            runtime_system_libraries(CCompilerStyle::Unix),
            system_library_arguments(cfg!(windows), CCompilerStyle::Unix)
        );

        let compiler = host_c_compiler().map_err(|error| error.to_string())?;
        assert!(!compiler.program().is_empty());
        assert_eq!(
            compiler.style(),
            if cfg!(all(windows, target_env = "msvc")) {
                CCompilerStyle::Msvc
            } else {
                CCompilerStyle::Unix
            }
        );
        assert_eq!(compiler.command().get_program(), compiler.program());
        Ok(())
    }

    /// Drives the emitted-C ship tier with a test-specific C host entry.
    /// The compile/link flags and runtime inputs are exactly the ones used
    /// by `run_c_aot`; only the host driver source differs.
    fn run_c_aot_with_entry(files: &[SourceFile], entry: &str) -> std::process::Output {
        let hir = check_program(files).expect("test program checks");
        let program = crate::emit_c(&hir).expect("emit ship C");
        let staticlib = runtime_staticlib().expect("runtime staticlib");
        let dir = TempDir::new("host-api-test").expect("temp dir");
        let src_path = dir.path.join("program.c");
        let entry_path = dir.path.join("entry.c");
        let metadata_path = dir.path.join("allocation-metadata.h");
        let exe_path = dir
            .path
            .join(format!("program{}", std::env::consts::EXE_SUFFIX));
        write_file(&src_path, program.source.as_bytes()).expect("write program.c");
        write_file(&entry_path, entry.as_bytes()).expect("write entry.c");
        write_file(
            &metadata_path,
            program.allocation_metadata_header.as_bytes(),
        )
        .expect("write allocation-metadata.h");

        let cc = host_c_compiler().expect("resolve C compiler");
        let mut command = cc.command();
        add_c11_optimized_flags(&mut command, cc.style());
        if cc.style().is_msvc() {
            command
                .arg(msvc_object_directory_arg(&dir.path))
                .arg("/I")
                .arg(&dir.path);
        } else {
            command.arg("-I").arg(&dir.path);
        }
        command
            .arg(&src_path)
            .arg(&entry_path)
            .arg(&staticlib)
            .args(runtime_system_libraries(cc.style()));
        if !cfg!(any(windows, target_os = "macos")) {
            command.arg("-pthread");
        }
        add_executable_output(&mut command, &exe_path, cc.style());
        let compile = command.output().expect("run C compiler");
        assert!(
            compile.status.success(),
            "compiling/linking test host failed:\n{}",
            tool_output_report(&compile)
        );
        Command::new(&exe_path).output().expect("run test host")
    }

    const MODULE_STATE_ISOLATION_SOURCE: &str = "let counter: i32 = 0;\n\
         export function advance(): void {\n\
         \x20 counter += 1;\n\
         \x20 print(`${counter}`);\n\
         }\n\
         export function main(): void {}\n";

    fn run_dev_module_state_reference() -> Vec<u8> {
        let program = sources(MODULE_STATE_ISOLATION_SOURCE);
        let mut session = crate::ReloadSession::new(&program).expect("create dev session");
        session.call_export("advance").expect("first dev call");
        session.call_export("advance").expect("second dev call");
        session.take_output()
    }

    #[test]
    fn module_state_is_isolated_between_concurrent_contexts_in_both_tiers() {
        let reference = run_dev_module_state_reference();
        assert_eq!(reference, b"1\n2\n", "single-Context reference");

        let threads: Vec<_> = (0..2)
            .map(|_| std::thread::spawn(run_dev_module_state_reference))
            .collect();
        let dev_outputs: Vec<Vec<u8>> = threads
            .into_iter()
            .map(|thread| thread.join().expect("join dev Context thread"))
            .collect();
        for (index, output) in dev_outputs.iter().enumerate() {
            assert_eq!(output, &reference, "dev Context {index} output");
        }

        let program = sources(MODULE_STATE_ISOLATION_SOURCE);
        let entry = host_entry(
            r#"
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

extern void subscript_export_advance(subscript_rt_context* ctx);

struct worker_result {
    uint8_t stdout_bytes[32];
    uint64_t stdout_len;
    int error;
};

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

#ifdef _WIN32
struct worker_args {
    int id;
    HANDLE init_done;
    HANDLE all_initialized;
    HANDLE turns[4];
    volatile LONG* ready;
    struct worker_result* result;
};

static DWORD WINAPI run_worker(LPVOID raw) {
    struct worker_args* args = (struct worker_args*)raw;
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        args->result->error = 1;
        return 0;
    }
    if (args->id == 1) WaitForSingleObject(args->init_done, INFINITE);
    call_entry(ctx, subscript_init);
    if (args->id == 0) SetEvent(args->init_done);
    if (InterlockedIncrement(args->ready) == 2) SetEvent(args->all_initialized);
    WaitForSingleObject(args->all_initialized, INFINITE);

    for (int round = 0; round < 2; ++round) {
        const int turn = round * 2 + args->id;
        WaitForSingleObject(args->turns[turn], INFINITE);
        call_entry(ctx, subscript_export_advance);
        if (turn + 1 < 4) SetEvent(args->turns[turn + 1]);
    }

    args->result->stdout_len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &args->result->stdout_len);
    if (args->result->stdout_len > sizeof args->result->stdout_bytes) {
        args->result->error = 2;
    } else if (args->result->stdout_len > 0) {
        memcpy(args->result->stdout_bytes, bytes, (size_t)args->result->stdout_len);
    }
    if (subscript_rt_ctx_trap_kind(ctx) != 0) args->result->error = 3;
    subscript_rt_ctx_release(ctx);
    return 0;
}
#else
struct coordinator {
    pthread_mutex_t mutex;
    pthread_cond_t condition;
    int init_done;
    int ready;
    int turn;
};

struct worker_args {
    int id;
    struct coordinator* coordinator;
    struct worker_result* result;
};

static void* run_worker(void* raw) {
    struct worker_args* args = (struct worker_args*)raw;
    struct coordinator* coordinator = args->coordinator;
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        args->result->error = 1;
        return NULL;
    }

    pthread_mutex_lock(&coordinator->mutex);
    while (args->id == 1 && !coordinator->init_done) {
        pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
    }
    pthread_mutex_unlock(&coordinator->mutex);
    call_entry(ctx, subscript_init);
    pthread_mutex_lock(&coordinator->mutex);
    if (args->id == 0) coordinator->init_done = 1;
    coordinator->ready += 1;
    pthread_cond_broadcast(&coordinator->condition);
    while (coordinator->ready != 2) {
        pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
    }
    pthread_mutex_unlock(&coordinator->mutex);

    for (int round = 0; round < 2; ++round) {
        const int expected_turn = round * 2 + args->id;
        pthread_mutex_lock(&coordinator->mutex);
        while (coordinator->turn != expected_turn) {
            pthread_cond_wait(&coordinator->condition, &coordinator->mutex);
        }
        pthread_mutex_unlock(&coordinator->mutex);
        call_entry(ctx, subscript_export_advance);
        pthread_mutex_lock(&coordinator->mutex);
        coordinator->turn += 1;
        pthread_cond_broadcast(&coordinator->condition);
        pthread_mutex_unlock(&coordinator->mutex);
    }

    args->result->stdout_len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &args->result->stdout_len);
    if (args->result->stdout_len > sizeof args->result->stdout_bytes) {
        args->result->error = 2;
    } else if (args->result->stdout_len > 0) {
        memcpy(args->result->stdout_bytes, bytes, (size_t)args->result->stdout_len);
    }
    if (subscript_rt_ctx_trap_kind(ctx) != 0) args->result->error = 3;
    subscript_rt_ctx_release(ctx);
    return NULL;
}
#endif

static int compare_result(
    const struct worker_result* result,
    const uint8_t* reference,
    uint64_t reference_len,
    int index) {
    if (result->error != 0 || result->stdout_len != reference_len ||
        memcmp(result->stdout_bytes, reference, (size_t)reference_len) != 0) {
        fprintf(stderr, "Context %d stdout: ", index);
        fwrite(result->stdout_bytes, 1, (size_t)result->stdout_len, stderr);
        fprintf(stderr, "single-Context reference: ");
        fwrite(reference, 1, (size_t)reference_len, stderr);
        return 30 + index;
    }
    return 0;
}

int main(void) {
    uint8_t reference[32];
    uint64_t reference_len = 0;
    subscript_rt_context* reference_ctx = subscript_rt_ctx_new();
    if (reference_ctx == NULL) return 2;
    call_entry(reference_ctx, subscript_init);
    call_entry(reference_ctx, subscript_export_advance);
    call_entry(reference_ctx, subscript_export_advance);
    const uint8_t* reference_bytes = subscript_rt_ctx_stdout(reference_ctx, &reference_len);
    if (reference_len > sizeof reference) return 3;
    memcpy(reference, reference_bytes, (size_t)reference_len);
    subscript_rt_ctx_release(reference_ctx);

    struct worker_result results[2] = {0};
#ifdef _WIN32
    volatile LONG ready = 0;
    HANDLE init_done = CreateEvent(NULL, TRUE, FALSE, NULL);
    HANDLE all_initialized = CreateEvent(NULL, TRUE, FALSE, NULL);
    HANDLE turns[4];
    for (int i = 0; i < 4; ++i) turns[i] = CreateEvent(NULL, FALSE, i == 0, NULL);
    struct worker_args args[2] = {
        {0, init_done, all_initialized, {turns[0], turns[1], turns[2], turns[3]}, &ready, &results[0]},
        {1, init_done, all_initialized, {turns[0], turns[1], turns[2], turns[3]}, &ready, &results[1]},
    };
    HANDLE threads[2] = {
        CreateThread(NULL, 0, run_worker, &args[0], 0, NULL),
        CreateThread(NULL, 0, run_worker, &args[1], 0, NULL),
    };
    WaitForMultipleObjects(2, threads, TRUE, INFINITE);
    for (int i = 0; i < 2; ++i) CloseHandle(threads[i]);
    CloseHandle(init_done);
    CloseHandle(all_initialized);
    for (int i = 0; i < 4; ++i) CloseHandle(turns[i]);
#else
    struct coordinator coordinator = {
        PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, 0, 0
    };
    struct worker_args args[2] = {
        {0, &coordinator, &results[0]},
        {1, &coordinator, &results[1]},
    };
    pthread_t threads[2];
    if (pthread_create(&threads[0], NULL, run_worker, &args[0]) != 0) return 4;
    if (pthread_create(&threads[1], NULL, run_worker, &args[1]) != 0) return 5;
    pthread_join(threads[0], NULL);
    pthread_join(threads[1], NULL);
    pthread_cond_destroy(&coordinator.condition);
    pthread_mutex_destroy(&coordinator.mutex);
#endif

    int comparison = compare_result(&results[0], reference, reference_len, 0);
    if (comparison != 0) return comparison;
    comparison = compare_result(&results[1], reference, reference_len, 1);
    if (comparison != 0) return comparison;
    return 0;
}
"#,
        );
        let run = run_c_aot_with_entry(&program, &entry);
        assert!(
            run.status.success(),
            "ship concurrent host exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        );
    }

    /// Prefixes a test host body with the generated runtime header, so
    /// host tests exercise the committed ABI artifact.
    ///
    /// A test host defines its own `int main(void)`; on Windows the
    /// MSVCRT opens stdout in text mode, which would translate the sink's
    /// `\n` to `\r\n` and break the byte-exact compare. This injects the
    /// same `_setmode(_fileno(stdout), _O_BINARY)` (and its `<io.h>` /
    /// `<fcntl.h>` includes) that the production `AOT_ENTRY_C` uses, at
    /// the top of `main`, `_WIN32`-guarded so it is a no-op elsewhere.
    fn host_entry(body: &str) -> String {
        const MAIN: &str = "int main(void) {";
        assert!(
            body.contains(MAIN),
            "test host body must define `int main(void)`"
        );
        let body = body.replacen(
            MAIN,
            "int main(void) {\n\
             #ifdef _WIN32\n\
             \x20   (void)_setmode(_fileno(stdout), _O_BINARY);\n\
             #endif",
            1,
        );
        format!(
            "{HOST_HEADER_C}\n\
             #ifdef _WIN32\n\
             #include <fcntl.h>\n\
             #include <io.h>\n\
             #endif\n\
             {body}"
        )
    }

    /// Asserts the shape every emitted object must have: the right
    /// format and architecture, `subscript_export_main` and `subscript_init` defined
    /// and global, and the runtime reached only as undefined imports.
    /// Mach-O global names carry a `_` prefix.
    fn assert_object_shape(bytes: &[u8], format: BinaryFormat, arch: Architecture) {
        let file = object::File::parse(bytes).expect("object file must parse");
        assert_eq!(file.format(), format);
        assert_eq!(file.architecture(), arch);
        let prefix = if format == BinaryFormat::MachO {
            "_"
        } else {
            ""
        };
        for name in ["subscript_export_main", "subscript_init"] {
            let sym = file
                .symbols()
                .find(|s| s.name() == Ok(format!("{prefix}{name}").as_str()))
                .unwrap_or_else(|| panic!("{name} must be present"));
            assert!(sym.is_definition(), "{name} must be defined");
            assert!(sym.is_global(), "{name} must be global");
        }
        let print = file
            .symbols()
            .find(|s| s.name() == Ok(format!("{prefix}subscript_rt_print").as_str()))
            .expect("subscript_rt_print must be referenced");
        assert!(print.is_undefined(), "the runtime is resolved at link time");
    }

    /// The object format the host target must produce.
    const HOST_FORMAT: BinaryFormat = if cfg!(target_os = "macos") {
        BinaryFormat::MachO
    } else if cfg!(target_os = "windows") {
        BinaryFormat::Coff
    } else {
        BinaryFormat::Elf
    };

    /// The architecture the host target must produce.
    const HOST_ARCH: Architecture = if cfg!(target_arch = "aarch64") {
        Architecture::Aarch64
    } else {
        Architecture::X86_64
    };

    #[test]
    fn host_object_defines_the_exported_entries_and_imports_the_runtime() {
        let obj = emit_object(
            &sources("export function main(): void {\n  print(\"hi\");\n}\n"),
            None,
        )
        .expect("emit host object");
        assert!(!obj.triple.is_empty());
        // Expected format and architecture are stated, not read back
        // out of the object under test.
        assert_object_shape(&obj.bytes, HOST_FORMAT, HOST_ARCH);
    }

    #[test]
    fn device_triples_emit_objects_for_the_real_lowering() {
        let src = "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  let total: i32 = 0;\n  for (let i: i32 = 0; i < xs.length; i += 1) {\n    total += xs[i];\n  }\n  print(`${total}`);\n}\n";
        for (triple, format) in [
            ("aarch64-apple-ios", BinaryFormat::MachO),
            ("aarch64-linux-android", BinaryFormat::Elf),
        ] {
            let obj = emit_object(&sources(src), Some(triple)).expect("emit device object");
            assert_eq!(obj.triple, triple);
            assert_object_shape(&obj.bytes, format, Architecture::Aarch64);
        }
    }

    #[test]
    fn unknown_triple_is_an_internal_error_not_a_panic() {
        let err = emit_object(
            &sources("export function main(): void {}\n"),
            Some("nonsense"),
        );
        assert!(matches!(err, Err(RunError::Internal(_))));
    }

    #[test]
    fn rejected_program_never_reaches_the_backend() {
        let err = emit_object(&sources("const x: number = 1;\n"), None);
        assert!(matches!(err, Err(RunError::Rejected(_))));
    }

    #[test]
    fn aot_runs_the_program_and_captures_the_sink_bytes() {
        let out = run_aot(&sources(
            "export function main(): void {\n  const a: i32 = 6;\n  print(`${a * 7}`);\n}\n",
        ))
        .expect("aot run");
        assert_eq!(out, b"42\n");
    }

    #[test]
    fn ship_c_host_trap_observer_and_clear_api_preserve_unwind_semantics() {
        let program = sources(
            "let calls: i32 = 0;\n\
             export function main(): void {\n\
               calls += 1;\n\
               print(`start:${calls}`);\n\
               if (calls === 1) {\n\
                 const failed: JsonResult<i32> = JSON.parse<i32>(\"nope\");\n\
                 print(`${failed.value}`);\n\
               }\n\
               print(\"done\");\n\
             }\n",
        );
        let entry = host_entry(
            r#"
#include <stdio.h>
#include <string.h>

struct observed_trap {
    uint32_t calls;
    uint32_t kind;
    uint32_t pos_id;
    const uint8_t* message;
    uint64_t message_len;
};

static void observe(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len) {
    struct observed_trap* observed = (struct observed_trap*)userdata;
    observed->calls += 1;
    observed->kind = kind;
    observed->pos_id = pos_id;
    observed->message = message;
    observed->message_len = message_len;
}

static int fail(subscript_rt_context* ctx, int code) {
    subscript_rt_ctx_release(ctx);
    return code;
}

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    struct observed_trap observed = {0};
    subscript_rt_ctx_set_trap_observer(ctx, observe, &observed);
    call_entry(ctx, subscript_init);
    call_entry(ctx, subscript_export_main);

    if (observed.calls != 1) return fail(ctx, 10);
    uint32_t kind = subscript_rt_ctx_trap_kind(ctx);
    uint32_t pos_id = subscript_rt_ctx_trap_pos_id(ctx);
    uint64_t message_len = 0;
    const uint8_t* message = subscript_rt_ctx_trap_message(ctx, &message_len);
    if (kind == 0) return fail(ctx, 11);
    if (observed.kind != kind || observed.pos_id != pos_id) return fail(ctx, 12);
    if (observed.message != message || observed.message_len != message_len) return fail(ctx, 13);
    if (memcmp(observed.message, message, (size_t)message_len) != 0) return fail(ctx, 14);

    const uint64_t live_before = subscript_rt_ctx_live_allocations(ctx);
    const uint64_t bytes_before = subscript_rt_ctx_live_bytes(ctx);
    const uint64_t reserved_before = subscript_rt_ctx_reserved_bytes(ctx);
    subscript_rt_ctx_enter_script(ctx);
    const int cleared_while_live = subscript_rt_ctx_clear_trap(ctx);
    subscript_rt_ctx_exit_script(ctx);
    if (cleared_while_live != 0) return fail(ctx, 15);
    if (subscript_rt_ctx_clear_trap(ctx) != 1) return fail(ctx, 16);
    if (subscript_rt_ctx_live_allocations(ctx) != live_before ||
        subscript_rt_ctx_live_bytes(ctx) != bytes_before ||
        subscript_rt_ctx_reserved_bytes(ctx) != reserved_before) return fail(ctx, 17);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 18);
    call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 19);
    if (observed.calls != 1) return fail(ctx, 20);

    uint64_t stdout_len = 0;
    const uint8_t* stdout_bytes = subscript_rt_ctx_stdout(ctx, &stdout_len);
    if (stdout_len > 0) fwrite(stdout_bytes, 1, (size_t)stdout_len, stdout);
    subscript_rt_ctx_release(ctx);

    subscript_rt_context* cleared_ctx = subscript_rt_ctx_new();
    if (cleared_ctx == NULL) return 3;
    struct observed_trap cleared = {0};
    subscript_rt_ctx_set_trap_observer(cleared_ctx, observe, &cleared);
    subscript_rt_ctx_set_trap_observer(cleared_ctx, NULL, NULL);
    call_entry(cleared_ctx, subscript_init);
    call_entry(cleared_ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(cleared_ctx) == 0) return fail(cleared_ctx, 21);
    if (cleared.calls != 0) return fail(cleared_ctx, 22);
    subscript_rt_ctx_release(cleared_ctx);
    return 0;
}
"#,
        );
        let run = run_c_aot_with_entry(&program, &entry);
        assert!(
            run.status.success(),
            "ship host exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            run.stdout, b"start:1\nstart:2\ndone\n",
            "the first call must unwind and the cleared second call must finish"
        );
    }

    #[test]
    fn ship_c_corpus_output_is_byte_identical_with_an_observer_registered() {
        let source = include_str!("../../corpus/accept/a01-hello.ts");
        let program = [SourceFile::new("a01-hello.ts", source)];
        let without = run_c_aot(&program).expect("a01 without observer");
        let entry = host_entry(
            r#"
#include <stdio.h>

static void observe(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len) {
    (void)kind;
    (void)pos_id;
    (void)message;
    (void)message_len;
    *(uint32_t*)userdata += 1;
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    uint32_t calls = 0;
    subscript_rt_ctx_set_trap_observer(ctx, observe, &calls);
    subscript_rt_ctx_enter_script(ctx);
    subscript_init(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_enter_script(ctx);
    subscript_export_main(ctx);
    subscript_rt_ctx_exit_script(ctx);
    if (subscript_rt_ctx_trap_kind(ctx) != 0 || calls != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    uint64_t len = 0;
    const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &len);
    if (len > 0) fwrite(bytes, 1, (size_t)len, stdout);
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        );
        let with = run_c_aot_with_entry(&program, &entry);
        assert!(
            with.status.success(),
            "ship observer host exited with {}: {}",
            with.status,
            String::from_utf8_lossy(&with.stderr)
        );
        assert_eq!(with.stdout, without, "observer changed a01 stdout bytes");
        assert_eq!(with.stdout, b"hello\n");
    }

    #[test]
    fn host_memory_accounting_agrees_on_count_and_measures_tier_bytes() {
        let program = sources(
            "class Cell {\n\
               value: i32;\n\
               constructor(value: i32) { this.value = value; }\n\
             }\n\
             export function main(): void {\n\
               const first: Cell = new Cell(1);\n\
               const deleted: Cell = new Cell(2);\n\
               const last: Cell = new Cell(first.value + 2);\n\
               Context.free(deleted);\n\
               if (last.value === 0) { print(\"unreachable\"); }\n\
             }\n",
        );
        let dev = crate::jit::memory_accounting_after_run(&program).expect("dev accounting");
        let entry = host_entry(
            r#"
#include <stdio.h>

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu %llu %llu\n",
        (unsigned long long)subscript_rt_ctx_live_allocations(ctx),
        (unsigned long long)subscript_rt_ctx_live_bytes(ctx),
        (unsigned long long)subscript_rt_ctx_reserved_bytes(ctx));
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        );
        let run = run_c_aot_with_entry(&program, &entry);
        assert!(
            run.status.success(),
            "ship accounting host exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        );
        let values: Vec<u64> = String::from_utf8(run.stdout)
            .expect("ship accounting output is UTF-8")
            .split_whitespace()
            .map(|value| value.parse().expect("ship accounting integer"))
            .collect();
        assert_eq!(values.len(), 3);
        let ship = (values[0], values[1], values[2]);

        assert_eq!(dev.0, 2, "three allocations minus one delete");
        assert_eq!(ship.0, dev.0, "tiers disagree on live allocation count");
        assert_ne!(ship.1, dev.1, "live bytes unexpectedly agree across tiers");
        assert_ne!(
            ship.2, dev.2,
            "reserved bytes unexpectedly agree across tiers"
        );
        eprintln!(
            "host memory accounting: dev=({}, {}, {}), ship=({}, {}, {})",
            dev.0, dev.1, dev.2, ship.0, ship.1, ship.2
        );
    }

    #[test]
    fn host_allocation_attribution_reports_known_sites_on_both_tiers() {
        let program = sources(
            "class Cell {\n\
               value: i32;\n\
               constructor(value: i32) { this.value = value; }\n\
             }\n\
             export function main(): void {\n\
               const cell: Cell = new Cell(7);\n\
               const values: i32[] = [];\n\
               values.push(cell.value);\n\
             }\n",
        );
        let (dev, dev_positions) =
            crate::jit::allocation_attribution_after_run(&program).expect("dev attribution");

        let entry = host_entry(
            r#"
#include "allocation-metadata.h"
#include <stdio.h>
#include <string.h>

struct triple {
    uint32_t class_id;
    uint32_t pos_id;
    uint64_t bytes;
};

struct observed {
    struct triple triples[8];
    uint64_t count;
};

static void visit(
    void* userdata, uint32_t class_id, uint32_t pos_id,
    uint64_t payload_bytes) {
    struct observed* observed = (struct observed*)userdata;
    if (observed->count < 8) {
        struct triple* triple = &observed->triples[observed->count];
        triple->class_id = class_id;
        triple->pos_id = pos_id;
        triple->bytes = payload_bytes;
    }
    observed->count += 1;
}

static const char* class_name(uint32_t class_id) {
    for (uint64_t i = 0; i < subscript_alloc_class_count; ++i) {
        if (subscript_alloc_classes[i].class_id == class_id) {
            return subscript_alloc_classes[i].name;
        }
    }
    return NULL;
}

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }

    struct observed observed = {0};
    uint64_t visited =
        subscript_rt_ctx_visit_live_allocations(ctx, visit, &observed);
    if (visited != 3 || observed.count != 3 ||
        subscript_rt_ctx_live_allocations(ctx) != 3) {
        subscript_rt_ctx_release(ctx);
        return 4;
    }

    for (uint64_t i = 0; i < observed.count; ++i) {
        const struct triple* triple = &observed.triples[i];
        if (triple->pos_id >= subscript_alloc_position_count) {
            subscript_rt_ctx_release(ctx);
            return 5;
        }
        const char* name = class_name(triple->class_id);
        const subscript_alloc_position_info* pos =
            &subscript_alloc_positions[triple->pos_id];
        uint32_t expected_line = 0;
        if (triple->class_id == 0 && name != NULL &&
            strcmp(name, "Cell") == 0) {
            expected_line = 6;
        } else if (triple->class_id == 4294967042u && name != NULL &&
                   strcmp(name, "Array") == 0) {
            expected_line = 7;
        } else if (triple->class_id == 4294967043u && name != NULL &&
                   strcmp(name, "ArrayData") == 0) {
            expected_line = 8;
        } else {
            subscript_rt_ctx_release(ctx);
            return 6;
        }
        if (strcmp(pos->file, "test.ts") != 0 ||
            pos->line != expected_line) {
            subscript_rt_ctx_release(ctx);
            return 7;
        }
        printf("%u %u %llu\n", triple->class_id, triple->pos_id,
               (unsigned long long)triple->bytes);
    }
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        );
        let run = run_c_aot_with_entry(&program, &entry);
        assert!(
            run.status.success(),
            "ship attribution host exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        );
        let values: Vec<u64> = String::from_utf8(run.stdout)
            .expect("ship attribution output is UTF-8")
            .split_whitespace()
            .map(|value| value.parse().expect("ship attribution integer"))
            .collect();
        assert_eq!(values.len(), 9);
        let mut ship: Vec<(u32, u32, u64)> = values
            .chunks_exact(3)
            .map(|v| (v[0] as u32, v[1] as u32, v[2]))
            .collect();
        ship.sort_unstable();

        assert_eq!(
            dev,
            vec![(0, 0, 4), (0xFFFF_FF02, 1, 32), (0xFFFF_FF03, 4, 16),],
            "dev attribution triples changed"
        );
        assert_eq!(
            ship,
            vec![(0, 0, 16), (0xFFFF_FF02, 1, 48), (0xFFFF_FF03, 2, 16),],
            "ship attribution triples changed"
        );
        let dev_sites: Vec<(u32, &str, u32)> = dev
            .iter()
            .map(|&(class_id, pos_id, _)| {
                let pos = &dev_positions[pos_id as usize];
                (class_id, pos.file.as_str(), pos.line)
            })
            .collect();
        assert_eq!(
            dev_sites,
            vec![
                (0, "test.ts", 6),
                (0xFFFF_FF02, "test.ts", 7),
                (0xFFFF_FF03, "test.ts", 8),
            ],
            "dev position table did not resolve the known sites"
        );
        eprintln!("allocation attribution: dev={dev:?}, ship={ship:?}");
    }

    #[test]
    fn allocation_corpus_object_request_counts_match_across_tiers() {
        let entry = host_entry(
            r#"
#include <stdio.h>

static void call_entry(subscript_rt_context* ctx, subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, subscript_init);
    if (subscript_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, subscript_export_main);
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        subscript_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu\n",
        (unsigned long long)subscript_rt_ctx_live_allocations(ctx));
    subscript_rt_ctx_release(ctx);
    return 0;
}
"#,
        );
        let cases = [
            (
                "t26-allocation-failure-new",
                include_str!("../../corpus/trap/t26-allocation-failure-new.ts"),
                3,
            ),
            (
                "t28-allocation-failure-array-literal",
                include_str!("../../corpus/trap/t28-allocation-failure-array-literal.ts"),
                4,
            ),
            (
                "t29-allocation-failure-push-grow",
                include_str!("../../corpus/trap/t29-allocation-failure-push-grow.ts"),
                4,
            ),
            (
                "t30-allocation-failure-string-concat",
                include_str!("../../corpus/trap/t30-allocation-failure-string-concat.ts"),
                4,
            ),
            (
                "t31-allocation-failure-template",
                include_str!("../../corpus/trap/t31-allocation-failure-template.ts"),
                6,
            ),
            (
                "t32-allocation-failure-generator-frame",
                include_str!("../../corpus/trap/t32-allocation-failure-generator-frame.ts"),
                3,
            ),
            (
                "t33-allocation-failure-json-raw-new",
                include_str!("../../corpus/trap/t33-allocation-failure-json-raw-new.ts"),
                6,
            ),
        ];

        for (id, source, expected) in cases {
            let files = vec![SourceFile::new(format!("{id}.ts"), source)];
            let dev = crate::jit::memory_accounting_after_run(&files)
                .expect("dev allocation count")
                .0;
            let run = run_c_aot_with_entry(&files, &entry);
            assert!(
                run.status.success(),
                "{id}: ship allocation-count host exited with {}: {}",
                run.status,
                String::from_utf8_lossy(&run.stderr)
            );
            let ship: u64 = String::from_utf8(run.stdout)
                .expect("ship count output is UTF-8")
                .trim()
                .parse()
                .expect("ship allocation count");
            eprintln!("{id}: object allocation requests dev={dev}, ship={ship}");
            assert_eq!(dev, expected, "{id}: dev exact allocation count changed");
            assert_eq!(ship, expected, "{id}: ship exact allocation count changed");
        }
    }

    #[test]
    fn aot_reports_a_trap_with_its_rule_and_position() {
        let err = run_aot(&sources(
            "export function main(): void {\n  const xs: i32[] = [1];\n  print(`${xs[4]}`);\n}\n",
        ));
        match err {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::IndexOutOfBounds);
                assert_eq!(t.pos.file, "test.ts");
                assert_eq!(t.pos.line, 3);
            }
            other => panic!("expected a trap, got {other:?}"),
        }
    }

    #[test]
    fn the_runtime_staticlib_resolves_to_a_file() {
        let path = runtime_staticlib_path().expect("runtime static library");
        assert!(path.is_file(), "{} must exist", path.display());
    }

    #[test]
    fn trap_line_parsing_is_total() {
        assert!(parse_trap(b"", &[], b"").is_none());
        assert!(parse_trap(b"something else\n", &[], b"").is_none());
        assert!(parse_trap(b"trap 999 0 unknown\n", &[], b"").is_none());
        let r =
            parse_trap(b"trap 2 0 pop() on an empty array\n", &[], b"before\n").expect("parsed");
        assert_eq!(r.rule, TrapKind::EmptyPop);
        assert!(r.message.contains("empty"));
        assert_eq!(r.stdout, b"before\n");
    }
}
