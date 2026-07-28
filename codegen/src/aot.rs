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
//! `ss_init` (module-global initializer) and `ss_export_<name>` for
//! every exported script function. The C entry creates a Context
//! through the runtime's host-driver entry points, calls `ss_init` and
//! `ss_export_main`, writes the Context's stdout sink to the process
//! stdout, and reports a trap on stderr with a non-zero exit status.
//! `print` still never writes to the process stdout itself: the bytes
//! compared by the differential gate are the sink's.

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

use crate::jit::{RunError, TrapReport};
use crate::lower::{aot_flags, internal, lower_module_with, LowerOptions};

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
}

/// Checks `files` and emits a relocatable object for `triple`.
///
/// Pass `None` for the host triple. The object defines `ss_init` and
/// one `ss_export_<name>` per exported script function, and imports the
/// runtime's `sub_rt_*` symbols, which the link resolves from the
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

static void call_script_entry(Context *ctx, sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
}

int main(void) {
#if defined(_WIN32)
    /* The sink bytes are compared byte-for-byte against the goldens; the
     * MSVCRT opens stdout in text mode and would translate '\n' to
     * '\r\n'. Binary mode writes the sink through unchanged. No-op on
     * every other platform, which has no text-mode translation. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    Context *ctx = sub_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    call_script_entry(ctx, ss_init);
    if (sub_rt_ctx_trap_kind(ctx) == 0) {
        call_script_entry(ctx, ss_export_main);
    }
    uint64_t len = 0;
    const unsigned char *out = sub_rt_ctx_stdout(ctx, &len);
    if (len > 0) {
        fwrite(out, 1, (size_t)len, stdout);
    }
    fflush(stdout);
    int status = 0;
    uint32_t kind = sub_rt_ctx_trap_kind(ctx);
    if (kind != 0) {
        uint64_t mlen = 0;
        const unsigned char *msg = sub_rt_ctx_trap_message(ctx, &mlen);
        fprintf(stderr, "trap %u %u ", kind, sub_rt_ctx_trap_pos_id(ctx));
        if (mlen > 0) {
            fwrite(msg, 1, (size_t)mlen, stderr);
        }
        fputc('\n', stderr);
        status = 3;
    }
    sub_rt_ctx_release(ctx);
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

/// The runtime static-library filename cargo produces for the host
/// target: `subscript_runtime.lib` on windows-msvc, the GNU-style
/// `libsubscript_runtime.a` everywhere else (Unix and windows-gnu).
fn runtime_staticlib_name() -> &'static str {
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

/// Resolves the C compiler used for linking. The compiler is clang
/// (compiler.md §11): `$CC` verbatim when set, else `clang` on `PATH`,
/// else — on Windows only — the standard LLVM install
/// (`%ProgramFiles%\LLVM\bin\clang.exe`). Falls back to the bare name
/// `clang`, so a missing toolchain surfaces as a clear run error.
fn host_c_compiler() -> std::ffi::OsString {
    if let Some(cc) = std::env::var_os("CC") {
        return cc;
    }
    if let Some(p) = find_on_path("clang") {
        return p.into_os_string();
    }
    #[cfg(windows)]
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        let llvm = PathBuf::from(pf).join("LLVM").join("bin").join("clang.exe");
        if llvm.is_file() {
            return llvm.into_os_string();
        }
    }
    "clang".into()
}

/// System import libraries the linked program needs on windows-msvc that
/// clang's own defaults do not supply. The runtime static library embeds
/// Rust `std`, which references these; `rustc` passes them automatically
/// when it links, so a manual clang link of the staticlib must add them
/// (`rustc --print native-static-libs` for this host: kernel32, ntdll,
/// userenv, ws2_32, dbghelp). Empty on every other target.
fn runtime_system_libs() -> &'static [&'static str] {
    if cfg!(all(windows, target_env = "msvc")) {
        &[
            "-lkernel32",
            "-lntdll",
            "-luserenv",
            "-lws2_32",
            "-ldbghelp",
        ]
    } else {
        &[]
    }
}

/// The committed synthetic-fixture directory (`corpus/interop`). Both AOT
/// link paths compile its C implementation and add this as an include
/// directory, so a foreign call resolves to the same implementation the
/// dev-JIT tier links (compiler.md §12.4). Repo-relative, resolved from the
/// crate.
fn interop_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop")
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
/// table), [`RunError::Internal`] on emission, toolchain, link, or
/// execution failures.
pub fn run_aot(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    let object = emit_object(files, None)?;
    let staticlib = runtime_staticlib()?;
    let dir = TempDir::new("run")?;

    let obj_path = dir.path.join("program.o");
    let entry_path = dir.path.join("entry.c");
    let exe_path = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&obj_path, &object.bytes)?;
    write_file(&entry_path, AOT_ENTRY_C.as_bytes())?;

    let interop = interop_dir();
    let cc = host_c_compiler();
    let link = Command::new(&cc)
        .arg("-I")
        .arg(&interop)
        .arg(&entry_path)
        .arg(&obj_path)
        .arg(interop.join("interop.c"))
        .arg(&staticlib)
        .args(runtime_system_libs())
        .arg("-o")
        .arg(&exe_path)
        .output()
        .map_err(|e| {
            RunError::Internal(internal(format!(
                "the C compiler `{}` (clang) could not be run: {e}. \
                 The ship-C AOT path requires clang (§11); \
                 install LLVM or set $CC.",
                cc.to_string_lossy()
            )))
        })?;
    if !link.status.success() {
        return Err(RunError::Internal(internal(format!(
            "link failed:\n{}",
            String::from_utf8_lossy(&link.stderr)
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
        None => Err(RunError::Internal(internal(format!(
            "linked program exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        )))),
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
/// `ss_init` / `ss_export_main` surface, so it is a drop-in subject.
/// The whole cycle happens in a temporary directory removed before
/// returning. The C compiler's absence is a failure, never a skip (the
/// gate machine is the development machine, §8.3).
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the linked program trapped (mapped back
/// through the emitted position table, with pre-trap stdout),
/// [`RunError::Internal`] on emission, toolchain, compile, link, or
/// execution failures.
pub fn run_c_aot(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, None)
}

/// Runs the emitted-C ship tier while refusing the `n`-th object-level
/// Context allocation after Context creation.
///
/// The injected fault is armed before `ss_init`, so module-initializer
/// allocations are part of the count. When
/// `SUBSCRIPT_C_AOT_ASAN` is set, the generated C and host entry are
/// compiled and linked with AddressSanitizer.
///
/// # Errors
///
/// Returns the same [`RunError`] variants as [`run_c_aot`].
pub fn run_c_aot_with_alloc_failure(
    files: &[SourceFile],
    n: u64,
) -> Result<Vec<u8>, RunError> {
    run_c_aot_configured(files, Some(n))
}

fn run_c_aot_configured(
    files: &[SourceFile],
    fail_alloc_after: Option<u64>,
) -> Result<Vec<u8>, RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;
    let program = crate::emit_c(&hir).map_err(|e| RunError::Internal(internal(e)))?;
    let staticlib = runtime_staticlib()?;
    let dir = TempDir::new("crun")?;

    let src_path = dir.path.join("program.c");
    let entry_path = dir.path.join("entry.c");
    let exe_path = dir
        .path
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    write_file(&src_path, program.source.as_bytes())?;
    let entry = if let Some(n) = fail_alloc_after {
        let anchor = "    call_script_entry(ctx, ss_init);";
        if !AOT_ENTRY_C.contains(anchor) {
            return Err(RunError::Internal(internal(
                "AOT entry allocation-fault anchor moved",
            )));
        }
        AOT_ENTRY_C.replace(
            anchor,
            &format!(
                "    sub_rt_ctx_fail_alloc_after(ctx, {n}u);\n\
                 {anchor}"
            ),
        )
    } else {
        AOT_ENTRY_C.to_string()
    };
    write_file(&entry_path, entry.as_bytes())?;

    let interop = interop_dir();
    let cc = host_c_compiler();
    let address_sanitizer = std::env::var_os("SUBSCRIPT_C_AOT_ASAN").is_some();
    let mut command = Command::new(&cc);
    command
        // The emitted ship C targets C11 (compiler block §11); the
        // language's C-ABI layout, compound literals, `_Alignof`, and
        // `<stdbool.h>`/`<stdint.h>` types are all C11. Pinning the
        // dialect keeps the ship tier independent of the platform
        // compiler's default `-std`.
        .arg("-std=c11")
        .arg("-O2")
        // Signed integer arithmetic in the emitted C must wrap
        // two's-complement (the language's semantics); `-fwrapv` makes
        // signed overflow defined rather than C undefined behaviour.
        .arg("-fwrapv")
        .arg("-ffp-contract=off")
        .args(
            address_sanitizer
                .then_some(["-fsanitize=address", "-fno-omit-frame-pointer"])
                .into_iter()
                .flatten(),
        )
        .arg("-I")
        .arg(&interop)
        .arg(&src_path)
        .arg(&entry_path)
        .arg(interop.join("interop.c"))
        .arg(&staticlib)
        .args(runtime_system_libs())
        .arg("-o")
        .arg(&exe_path);
    let compile = command
        .output()
        .map_err(|e| {
            RunError::Internal(internal(format!(
                "the C compiler `{}` (clang) could not be run: {e}. \
                 The ship-C AOT path requires clang (§11); \
                 install LLVM or set $CC.",
                cc.to_string_lossy()
            )))
        })?;
    if !compile.status.success() {
        return Err(RunError::Internal(internal(format!(
            "compiling/linking the emitted C failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
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
        None => Err(RunError::Internal(internal(format!(
            "linked C program exited with {}: {}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        )))),
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

    /// Drives the emitted-C ship tier with a test-specific C host entry.
    /// The compile/link flags and runtime/interop inputs are exactly the
    /// ones used by `run_c_aot`; only the host driver source differs.
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

        let interop = interop_dir();
        let cc = host_c_compiler();
        let compile = Command::new(&cc)
            .arg("-std=c11")
            .arg("-O2")
            .arg("-fwrapv")
            .arg("-ffp-contract=off")
            .arg("-I")
            .arg(&dir.path)
            .arg("-I")
            .arg(&interop)
            .arg(&src_path)
            .arg(&entry_path)
            .arg(interop.join("interop.c"))
            .arg(&staticlib)
            .args(runtime_system_libs())
            .arg("-o")
            .arg(&exe_path)
            .output()
            .expect("run C compiler");
        assert!(
            compile.status.success(),
            "compiling/linking test host failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        Command::new(&exe_path).output().expect("run test host")
    }

    /// Prefixes a test host body with the generated runtime header, so
    /// host tests exercise the committed ABI artifact.
    fn host_entry(body: &str) -> String {
        format!("{HOST_HEADER_C}\n{body}")
    }

    /// Asserts the shape every emitted object must have: the right
    /// format and architecture, `ss_export_main` and `ss_init` defined
    /// and global, and the runtime reached only as undefined imports.
    /// Mach-O global names carry a `_` prefix.
    fn assert_object_shape(bytes: &[u8], format: BinaryFormat, arch: Architecture) {
        let file = object::File::parse(bytes).expect("object file must parse");
        assert_eq!(file.format(), format);
        assert_eq!(file.architecture(), arch);
        let prefix = if format == BinaryFormat::MachO { "_" } else { "" };
        for name in ["ss_export_main", "ss_init"] {
            let sym = file
                .symbols()
                .find(|s| s.name() == Ok(format!("{prefix}{name}").as_str()))
                .unwrap_or_else(|| panic!("{name} must be present"));
            assert!(sym.is_definition(), "{name} must be defined");
            assert!(sym.is_global(), "{name} must be global");
        }
        let print = file
            .symbols()
            .find(|s| s.name() == Ok(format!("{prefix}sub_rt_print").as_str()))
            .expect("sub_rt_print must be referenced");
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
        let err = emit_object(&sources("export function main(): void {}\n"), Some("nonsense"));
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

static int fail(Context* ctx, int code) {
    sub_rt_ctx_release(ctx);
    return code;
}

static void call_entry(Context* ctx, sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
}

int main(void) {
    Context* ctx = sub_rt_ctx_new();
    if (ctx == NULL) return 2;
    struct observed_trap observed = {0};
    sub_rt_ctx_set_trap_observer(ctx, observe, &observed);
    call_entry(ctx, ss_init);
    call_entry(ctx, ss_export_main);

    if (observed.calls != 1) return fail(ctx, 10);
    uint32_t kind = sub_rt_ctx_trap_kind(ctx);
    uint32_t pos_id = sub_rt_ctx_trap_pos_id(ctx);
    uint64_t message_len = 0;
    const uint8_t* message = sub_rt_ctx_trap_message(ctx, &message_len);
    if (kind == 0) return fail(ctx, 11);
    if (observed.kind != kind || observed.pos_id != pos_id) return fail(ctx, 12);
    if (observed.message != message || observed.message_len != message_len) return fail(ctx, 13);
    if (memcmp(observed.message, message, (size_t)message_len) != 0) return fail(ctx, 14);

    const uint64_t live_before = sub_rt_ctx_live_allocations(ctx);
    const uint64_t bytes_before = sub_rt_ctx_live_bytes(ctx);
    const uint64_t reserved_before = sub_rt_ctx_reserved_bytes(ctx);
    sub_rt_ctx_enter_script(ctx);
    const int cleared_while_live = sub_rt_ctx_clear_trap(ctx);
    sub_rt_ctx_exit_script(ctx);
    if (cleared_while_live != 0) return fail(ctx, 15);
    if (sub_rt_ctx_clear_trap(ctx) != 1) return fail(ctx, 16);
    if (sub_rt_ctx_live_allocations(ctx) != live_before ||
        sub_rt_ctx_live_bytes(ctx) != bytes_before ||
        sub_rt_ctx_reserved_bytes(ctx) != reserved_before) return fail(ctx, 17);
    if (sub_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 18);
    call_entry(ctx, ss_export_main);
    if (sub_rt_ctx_trap_kind(ctx) != 0) return fail(ctx, 19);
    if (observed.calls != 1) return fail(ctx, 20);

    uint64_t stdout_len = 0;
    const uint8_t* stdout_bytes = sub_rt_ctx_stdout(ctx, &stdout_len);
    if (stdout_len > 0) fwrite(stdout_bytes, 1, (size_t)stdout_len, stdout);
    sub_rt_ctx_release(ctx);

    Context* cleared_ctx = sub_rt_ctx_new();
    if (cleared_ctx == NULL) return 3;
    struct observed_trap cleared = {0};
    sub_rt_ctx_set_trap_observer(cleared_ctx, observe, &cleared);
    sub_rt_ctx_set_trap_observer(cleared_ctx, NULL, NULL);
    call_entry(cleared_ctx, ss_init);
    call_entry(cleared_ctx, ss_export_main);
    if (sub_rt_ctx_trap_kind(cleared_ctx) == 0) return fail(cleared_ctx, 21);
    if (cleared.calls != 0) return fail(cleared_ctx, 22);
    sub_rt_ctx_release(cleared_ctx);
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
    Context* ctx = sub_rt_ctx_new();
    if (ctx == NULL) return 2;
    uint32_t calls = 0;
    sub_rt_ctx_set_trap_observer(ctx, observe, &calls);
    sub_rt_ctx_enter_script(ctx);
    ss_init(ctx);
    sub_rt_ctx_exit_script(ctx);
    sub_rt_ctx_enter_script(ctx);
    ss_export_main(ctx);
    sub_rt_ctx_exit_script(ctx);
    if (sub_rt_ctx_trap_kind(ctx) != 0 || calls != 0) {
        sub_rt_ctx_release(ctx);
        return 3;
    }
    uint64_t len = 0;
    const uint8_t* bytes = sub_rt_ctx_stdout(ctx, &len);
    if (len > 0) fwrite(bytes, 1, (size_t)len, stdout);
    sub_rt_ctx_release(ctx);
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
               unsafeDelete(deleted);\n\
               if (last.value === 0) { print(\"unreachable\"); }\n\
             }\n",
        );
        let dev = crate::jit::memory_accounting_after_run(&program).expect("dev accounting");
        let entry = host_entry(
            r#"
#include <stdio.h>

static void call_entry(Context* ctx, sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
}

int main(void) {
    Context* ctx = sub_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, ss_init);
    if (sub_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, ss_export_main);
    if (sub_rt_ctx_trap_kind(ctx) != 0) {
        sub_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu %llu %llu\n",
        (unsigned long long)sub_rt_ctx_live_allocations(ctx),
        (unsigned long long)sub_rt_ctx_live_bytes(ctx),
        (unsigned long long)sub_rt_ctx_reserved_bytes(ctx));
    sub_rt_ctx_release(ctx);
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
            crate::jit::allocation_attribution_after_run(&program)
                .expect("dev attribution");

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
    for (uint64_t i = 0; i < ss_alloc_class_count; ++i) {
        if (ss_alloc_classes[i].class_id == class_id) {
            return ss_alloc_classes[i].name;
        }
    }
    return NULL;
}

static void call_entry(Context* ctx, sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
}

int main(void) {
    Context* ctx = sub_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, ss_init);
    if (sub_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, ss_export_main);
    if (sub_rt_ctx_trap_kind(ctx) != 0) {
        sub_rt_ctx_release(ctx);
        return 3;
    }

    struct observed observed = {0};
    uint64_t visited =
        sub_rt_ctx_visit_live_allocations(ctx, visit, &observed);
    if (visited != 3 || observed.count != 3 ||
        sub_rt_ctx_live_allocations(ctx) != 3) {
        sub_rt_ctx_release(ctx);
        return 4;
    }

    for (uint64_t i = 0; i < observed.count; ++i) {
        const struct triple* triple = &observed.triples[i];
        if (triple->pos_id >= ss_alloc_position_count) {
            sub_rt_ctx_release(ctx);
            return 5;
        }
        const char* name = class_name(triple->class_id);
        const sub_alloc_position_info* pos =
            &ss_alloc_positions[triple->pos_id];
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
            sub_rt_ctx_release(ctx);
            return 6;
        }
        if (strcmp(pos->file, "test.ts") != 0 ||
            pos->line != expected_line) {
            sub_rt_ctx_release(ctx);
            return 7;
        }
        printf("%u %u %llu\n", triple->class_id, triple->pos_id,
               (unsigned long long)triple->bytes);
    }
    sub_rt_ctx_release(ctx);
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
            vec![
                (0, 0, 4),
                (0xFFFF_FF02, 1, 32),
                (0xFFFF_FF03, 4, 16),
            ],
            "dev attribution triples changed"
        );
        assert_eq!(
            ship,
            vec![
                (0, 0, 16),
                (0xFFFF_FF02, 1, 48),
                (0xFFFF_FF03, 2, 16),
            ],
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

static void call_entry(Context* ctx, sub_script_main_entry entry) {
    sub_rt_ctx_enter_script(ctx);
    entry(ctx);
    sub_rt_ctx_exit_script(ctx);
}

int main(void) {
    Context* ctx = sub_rt_ctx_new();
    if (ctx == NULL) return 2;
    call_entry(ctx, ss_init);
    if (sub_rt_ctx_trap_kind(ctx) == 0) call_entry(ctx, ss_export_main);
    if (sub_rt_ctx_trap_kind(ctx) != 0) {
        sub_rt_ctx_release(ctx);
        return 3;
    }
    printf("%llu\n",
        (unsigned long long)sub_rt_ctx_live_allocations(ctx));
    sub_rt_ctx_release(ctx);
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
                include_str!(
                    "../../corpus/trap/t28-allocation-failure-array-literal.ts"
                ),
                4,
            ),
            (
                "t29-allocation-failure-push-grow",
                include_str!(
                    "../../corpus/trap/t29-allocation-failure-push-grow.ts"
                ),
                4,
            ),
            (
                "t30-allocation-failure-string-concat",
                include_str!(
                    "../../corpus/trap/t30-allocation-failure-string-concat.ts"
                ),
                4,
            ),
            (
                "t31-allocation-failure-template",
                include_str!(
                    "../../corpus/trap/t31-allocation-failure-template.ts"
                ),
                6,
            ),
            (
                "t32-allocation-failure-generator-frame",
                include_str!(
                    "../../corpus/trap/t32-allocation-failure-generator-frame.ts"
                ),
                3,
            ),
            (
                "t33-allocation-failure-json-raw-new",
                include_str!(
                    "../../corpus/trap/t33-allocation-failure-json-raw-new.ts"
                ),
                6,
            ),
        ];

        for (id, source, expected) in cases {
            let files = vec![SourceFile::new(format!("{id}.ts"), source)];
            let dev =
                crate::jit::memory_accounting_after_run(&files)
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
            assert_eq!(
                ship, expected,
                "{id}: ship exact allocation count changed"
            );
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
