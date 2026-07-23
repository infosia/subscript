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
pub const AOT_ENTRY_C: &str = r#"/* Host entry for a subscript AOT build (compiler.md 8.1).
 * Generated; never hand-edited.
 */
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#if defined(_WIN32)
#include <io.h>
#include <fcntl.h>
#endif

extern void *sub_rt_ctx_new(void);
extern void sub_rt_ctx_release(void *ctx);
extern const unsigned char *sub_rt_ctx_stdout(const void *ctx, uint64_t *len);
extern uint32_t sub_rt_ctx_trap_kind(const void *ctx);
extern uint32_t sub_rt_ctx_trap_pos_id(const void *ctx);
extern const unsigned char *sub_rt_ctx_trap_message(const void *ctx, uint64_t *len);

extern void ss_init(void *ctx);
extern void ss_export_main(void *ctx);

int main(void) {
#if defined(_WIN32)
    /* The sink bytes are compared byte-for-byte against the goldens; the
     * MSVCRT opens stdout in text mode and would translate '\n' to
     * '\r\n'. Binary mode writes the sink through unchanged. No-op on
     * every other platform, which has no text-mode translation. */
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    void *ctx = sub_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    ss_init(ctx);
    if (sub_rt_ctx_trap_kind(ctx) == 0) {
        ss_export_main(ctx);
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
"#;

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

/// The committed synthetic-header directory (`corpus/interop`), holding
/// `interop.h` and its implementation `interop.c` (P5.2b). Both AOT link
/// paths compile `interop.c` in and add this as an include directory, so
/// a foreign call resolves to the same implementation the dev-JIT tier
/// links (compiler.md §12.4). Repo-relative, resolved from the crate.
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
    match parse_trap(&run.stderr, &object.positions) {
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
/// through the emitted position table), [`RunError::Internal`] on
/// emission, toolchain, compile, link, or execution failures.
pub fn run_c_aot(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
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
    write_file(&entry_path, AOT_ENTRY_C.as_bytes())?;

    let interop = interop_dir();
    let cc = host_c_compiler();
    let compile = Command::new(&cc)
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
    match parse_trap(&run.stderr, &program.positions) {
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
fn parse_trap(stderr: &[u8], positions: &[Pos]) -> Option<TrapReport> {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::{Architecture, BinaryFormat, Object, ObjectSymbol};

    fn sources(src: &str) -> Vec<SourceFile> {
        vec![SourceFile::new("test.ts", src)]
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
        assert!(parse_trap(b"", &[]).is_none());
        assert!(parse_trap(b"something else\n", &[]).is_none());
        assert!(parse_trap(b"trap 999 0 unknown\n", &[]).is_none());
        let r = parse_trap(b"trap 2 0 pop() on an empty array\n", &[]).expect("parsed");
        assert_eq!(r.rule, TrapKind::EmptyPop);
        assert!(r.message.contains("empty"));
    }
}
