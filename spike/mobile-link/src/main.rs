//! P0.5 mobile link spike emitter (`specs/blocks/compiler.md` §3).
//!
//! Emits one relocatable object file per device triple —
//! `aarch64-linux-android` (ELF) and `aarch64-apple-ios` (Mach-O) — via
//! `cranelift-object`. Each object exports a single function
//! `spike_main() -> i64` that computes the loop sum of `1..=10`, multiplies
//! it by 3, adds 7, passes the result to the external runtime function
//! `subscript_rt_print_i64(i64)`, and returns it. The emitted code uses no
//! thread-local storage.
//!
//! Output directory: `out/` next to this crate's manifest by default, or
//! the first command-line argument if given.

use std::error::Error;
use std::path::Path;
use std::str::FromStr;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::object::macho::PLATFORM_IOS;
use cranelift_object::object::write::MachOBuildVersion;
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

/// The two device triples the spike must cover (compiler.md §3).
const TARGET_TRIPLES: [&str; 2] = ["aarch64-linux-android", "aarch64-apple-ios"];

/// Mach-O build-version value for `minos`/`sdk` 10.0.0, encoded as
/// nibble-packed `xxxx.yy.zz`.
const MACHO_VERSION_10_0_0: u32 = (10 << 16) | (0 << 8) | 0;

fn main() {
    if let Err(err) = run() {
        eprintln!("mobile-link-spike: error: {err}");
        std::process::exit(1);
    }
}

/// Emits both target objects into the output directory.
fn run() -> Result<(), Box<dyn Error>> {
    let default_out = concat!(env!("CARGO_MANIFEST_DIR"), "/out").to_string();
    let out_dir = std::env::args().nth(1).unwrap_or(default_out);
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir_all(out_dir)?;

    for triple in TARGET_TRIPLES {
        let bytes = emit_object(triple)?;
        let path = out_dir.join(format!("spike-{triple}.o"));
        std::fs::write(&path, &bytes)?;
        println!("wrote {} ({} bytes)", path.display(), bytes.len());
    }
    Ok(())
}

/// Builds the spike program for `triple_str` and returns the serialized
/// object file bytes.
///
/// For the iOS triple a Mach-O build version (`PLATFORM_IOS`, minos 10.0,
/// sdk 10.0) is stamped on the object before serialization, because
/// cranelift-object 0.125.4 does not set one itself and Apple's linker
/// requires it.
fn emit_object(triple_str: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let triple = Triple::from_str(triple_str)
        .map_err(|e| format!("invalid target triple {triple_str}: {e}"))?;
    let is_macho = triple_str.contains("apple");

    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true")?;
    let isa = isa::lookup(triple)?.finish(settings::Flags::new(flag_builder))?;

    let builder = ObjectBuilder::new(isa, "subscript-mobile-link-spike", default_libcall_names())?;
    let mut module = ObjectModule::new(builder);

    // extern "C" fn subscript_rt_print_i64(i64)
    let mut print_sig = module.make_signature();
    print_sig.params.push(AbiParam::new(types::I64));
    let print_id =
        module.declare_function("subscript_rt_print_i64", Linkage::Import, &print_sig)?;

    // exported fn spike_main() -> i64
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I64));
    let main_id = module.declare_function("spike_main", Linkage::Export, &main_sig)?;

    let mut ctx = module.make_context();
    ctx.func.signature = main_sig;
    let mut fb_ctx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

        let entry = b.create_block();
        let header = b.create_block();
        let body = b.create_block();
        let exit = b.create_block();

        let i_var = b.declare_var(types::I64);
        let sum_var = b.declare_var(types::I64);

        // entry: i = 1; sum = 0
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        b.seal_block(entry);
        let one = b.ins().iconst(types::I64, 1);
        let zero = b.ins().iconst(types::I64, 0);
        b.def_var(i_var, one);
        b.def_var(sum_var, zero);
        b.ins().jump(header, &[]);

        // header: if i <= 10 goto body else goto exit
        b.switch_to_block(header);
        let i = b.use_var(i_var);
        let keep_going = b.ins().icmp_imm(IntCC::SignedLessThanOrEqual, i, 10);
        b.ins().brif(keep_going, body, &[], exit, &[]);

        // body: sum += i; i += 1; goto header
        b.switch_to_block(body);
        b.seal_block(body);
        let i = b.use_var(i_var);
        let sum = b.use_var(sum_var);
        let next_sum = b.ins().iadd(sum, i);
        let next_i = b.ins().iadd_imm(i, 1);
        b.def_var(sum_var, next_sum);
        b.def_var(i_var, next_i);
        b.ins().jump(header, &[]);
        b.seal_block(header);

        // exit: result = sum * 3 + 7; subscript_rt_print_i64(result); return result
        b.switch_to_block(exit);
        b.seal_block(exit);
        let sum = b.use_var(sum_var);
        let tripled = b.ins().imul_imm(sum, 3);
        let result = b.ins().iadd_imm(tripled, 7);
        let print_ref = module.declare_func_in_func(print_id, b.func);
        b.ins().call(print_ref, &[result]);
        b.ins().return_(&[result]);
        b.finalize();
    }

    module.define_function(main_id, &mut ctx)?;
    module.clear_context(&mut ctx);

    let mut product = module.finish();
    if is_macho {
        // cranelift-object 0.125.4 does not stamp an LC_BUILD_VERSION load
        // command; without one Apple's linker rejects the object.
        let mut build_version = MachOBuildVersion::default();
        build_version.platform = PLATFORM_IOS;
        build_version.minos = MACHO_VERSION_10_0_0;
        build_version.sdk = MACHO_VERSION_10_0_0;
        product.object.set_macho_build_version(build_version);
    }
    Ok(product.emit()?)
}

#[cfg(test)]
mod tests {
    use super::emit_object;
    use object::{Architecture, BinaryFormat, Object, ObjectSymbol};

    /// Emits `triple`, round-trips the bytes through a temp file (the same
    /// bytes `run` writes), and returns them for parsing.
    fn emit_via_temp_file(triple: &str) -> Vec<u8> {
        let bytes = emit_object(triple).expect("object emission must succeed");
        let dir = std::env::temp_dir().join("mobile-link-spike-tests");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(format!("spike-{triple}.o"));
        std::fs::write(&path, &bytes).expect("write temp object");
        std::fs::read(&path).expect("read temp object back")
    }

    /// Asserts format and architecture, that `spike_main` is a defined
    /// global symbol, and that `subscript_rt_print_i64` is an undefined
    /// (imported) global symbol. Mach-O global names carry a `_` prefix.
    fn assert_spike_object(bytes: &[u8], format: BinaryFormat) {
        let file = object::File::parse(bytes).expect("object file must parse");
        assert_eq!(file.format(), format);
        assert_eq!(file.architecture(), Architecture::Aarch64);

        let prefix = if format == BinaryFormat::MachO {
            "_"
        } else {
            ""
        };
        let main_name = format!("{prefix}spike_main");
        let print_name = format!("{prefix}subscript_rt_print_i64");

        let main_sym = file
            .symbols()
            .find(|s| s.name() == Ok(main_name.as_str()))
            .expect("spike_main symbol must be present");
        assert!(main_sym.is_definition(), "spike_main must be defined");
        assert!(main_sym.is_global(), "spike_main must be global");

        let print_sym = file
            .symbols()
            .find(|s| s.name() == Ok(print_name.as_str()))
            .expect("subscript_rt_print_i64 symbol must be present");
        assert!(
            print_sym.is_undefined(),
            "subscript_rt_print_i64 must be an undefined import"
        );
        assert!(
            print_sym.is_global(),
            "subscript_rt_print_i64 must be global"
        );
    }

    #[test]
    fn android_object_is_valid_elf_aarch64() {
        let bytes = emit_via_temp_file("aarch64-linux-android");
        assert_spike_object(&bytes, BinaryFormat::Elf);
    }

    #[test]
    fn ios_object_is_valid_macho_aarch64() {
        let bytes = emit_via_temp_file("aarch64-apple-ios");
        assert_spike_object(&bytes, BinaryFormat::MachO);
    }
}
