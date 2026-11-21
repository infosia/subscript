//! The dev JIT form of a collection poll (`compiler.md` §342 rule 9).
//!
//! Cost: one checker and JIT lowering of a small module.

use crate::lower::func::take_defined_function_texts;
use crate::lower::{dev_flags, lower_lir_module_with, LowerOptions};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncOrDataId, Module};
use subscript_compiler::{check_program, SourceFile};

const SOURCE: &str = r#"
class P { x: i32 = 0; }
function fill(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const p: P = new P(); s += p.x; }
  return s;
}
export function main(): void { print(`${fill(3)}`); }
"#;

#[test]
fn a_poll_is_one_load_and_one_branch_to_a_cold_call() {
    let hir = check_program(&[SourceFile::new("poll.ts", SOURCE)]).expect("checks");
    let lir = crate::lir::lower_module(&hir).expect("LIR");
    let isa = cranelift_native::builder()
        .expect("host ISA")
        .finish(dev_flags().expect("dev flags"))
        .expect("ISA flags");
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let _ = take_defined_function_texts();
    lower_lir_module_with(&mut module, &lir, LowerOptions::default()).expect("JIT lowering");
    let functions = take_defined_function_texts();
    let Some(FuncOrDataId::Func(id)) = module.declarations().get_name("subscript_rt_collect_poll")
    else {
        panic!("the poll runtime function is not declared");
    };
    // SAFETY: no finalized function address escapes this test.
    unsafe { module.free_memory() };
    let offset = subscript_runtime::Context::collection_pending_offset();
    let polled = functions
        .iter()
        .filter(|(_, text)| text.contains(&format!("load.i8 notrap aligned v0+{offset}")))
        .collect::<Vec<_>>();
    // Only `fill` has a loop that allocates.
    assert_eq!(polled.len(), 1, "{functions:#?}");
    let text = &polled[0].1;
    println!("{text}");
    // A test build prefixes each instruction with its source location.
    let lines = text
        .lines()
        .map(|line| {
            let line = line.trim();
            match line.strip_prefix('@') {
                Some(rest) => rest.split_once(' ').map_or(line, |(_, rest)| rest.trim()),
                None => line,
            }
        })
        .collect::<Vec<_>>();
    let load = lines
        .iter()
        .position(|line| line.contains("load.i8") && line.ends_with(&format!("+{offset}")))
        .expect("one load of the pending flag");
    let flag = lines[load].split(" = ").next().expect("load result");
    let branch = lines[load + 1];
    assert!(
        branch.starts_with(&format!("brif {flag}, ")),
        "the branch follows the load: {branch}"
    );
    let cold = branch
        .trim_start_matches(&format!("brif {flag}, "))
        .split(',')
        .next()
        .expect("cold target")
        .trim()
        .to_string();
    let cold_line = lines
        .iter()
        .position(|line| line.starts_with(&format!("{cold} cold:")))
        .expect("the poll block is cold");
    let reference = lines
        .iter()
        .find_map(|line| {
            let (reference, declaration) = line.split_once(" = ")?;
            declaration
                .starts_with(&format!("u0:{} ", id.as_u32()))
                .then(|| reference.to_string())
        })
        .expect("the poll function reference");
    assert!(
        lines[cold_line + 1].starts_with(&format!("call {reference}(")),
        "the cold block calls the runtime first: {}",
        lines[cold_line + 1]
    );
}
