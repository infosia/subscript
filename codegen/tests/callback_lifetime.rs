//! The callback lifetime reaches LIR (`specs/blocks/compiler.md` §111
//! rule 2).
//!
//! The lowering copies the checked value, the LIR text prints the field
//! only when the lifetime is explicit, and each tier reads that field to
//! pick its crossing and its trampoline (§111 rule 4).

use subscript_codegen::emit_c;
use subscript_codegen::lir::lower_module;
use subscript_compiler::lir_text::print_module;
use subscript_compiler::types::CallbackLifetime;
use subscript_compiler::{check_program, SourceFile};

/// A mirror with one callback-carrying boundary class. `directive` holds
/// the `@subscript-c-callback-lifetime` record the case needs, so the
/// control and the selected case differ in that text only.
fn sink_mirror(directive: &str) -> String {
    format!(
        "\
// @subscript-c-header include=\"engine.h\"
// @subscript-c-callback typedef=\"EngineCallback\"
{directive}type EngineCallback = (engineMessage: string, engineUserdata1: object | null, engineUserdata2: object | null) => void;
declare class EngineSink {{
  engineCallback: EngineCallback;
  engineUserdata1: object | null;
  engineUserdata2: object | null;
  constructor(engineCallback: EngineCallback, engineUserdata1: object | null, engineUserdata2: object | null);
}}
declare function engineUse(engineSink: EngineSink): void;
"
    )
}

const PROGRAM: &str = "export function main(): void {}\n";

/// A program that crosses the boundary with the mirrored aggregate, so
/// the marshaling of §111 rule 4 is emitted.
const CROSSING: &str = "\
class EngineState {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
export function main(): void {
  const state: EngineState = new EngineState(1);
  const sink: EngineSink = new EngineSink(
    (engineMessage, engineUserdata1, engineUserdata2) => {
      print(`${engineMessage.length}`);
    },
    state,
    null,
  );
  engineUse(sink);
}
";

fn checked(directive: &str, program: &str) -> subscript_compiler::hir::Module {
    let files = [
        SourceFile::ambient("engine.generated.d.ts", sink_mirror(directive)),
        SourceFile::new("prog.ts", program.to_string()),
    ];
    check_program(&files).expect("the mirror and the program check clean")
}

fn lower(directive: &str) -> subscript_compiler::lir::Module {
    lower_module(&checked(directive, PROGRAM)).expect("the module lowers to LIR")
}

fn emitted_c(directive: &str) -> String {
    emit_c(&checked(directive, CROSSING))
        .expect("the module emits C")
        .source
}

fn class_lifetime(module: &subscript_compiler::lir::Module, name: &str) -> CallbackLifetime {
    module
        .classes
        .iter()
        .find(|class| class.source_name == name)
        .unwrap_or_else(|| panic!("class `{name}`"))
        .callback_lifetime
}

#[test]
fn the_selected_aggregate_carries_the_explicit_lifetime_in_lir() {
    let selected = lower("// @subscript-c-callback-lifetime aggregate=\"EngineSink\"\n");
    assert_eq!(
        class_lifetime(&selected, "EngineSink"),
        CallbackLifetime::Explicit
    );

    // Firing control: the same mirror without the record.
    let control = lower("");
    assert_eq!(
        class_lifetime(&control, "EngineSink"),
        CallbackLifetime::Context
    );
}

#[test]
fn the_lir_text_prints_the_lifetime_only_when_it_is_explicit() {
    let selected = print_module(&lower(
        "// @subscript-c-callback-lifetime aggregate=\"EngineSink\"\n",
    ));
    let control = print_module(&lower(""));
    let marked: Vec<&str> = selected
        .lines()
        .filter(|line| line.contains("callback-lifetime=explicit"))
        .collect();
    assert_eq!(marked.len(), 1, "{selected}");
    assert!(marked[0].contains("\"EngineSink\""), "{selected}");
    assert!(
        !control.contains("callback-lifetime"),
        "the Context lifetime prints nothing:\n{control}"
    );
}

/// §111 rule 4: the ship tier reads the class field and emits the
/// registration crossing with its own trampoline.
#[test]
fn the_ship_tier_emits_the_registration_crossing_for_the_selected_aggregate() {
    let selected = emitted_c("// @subscript-c-callback-lifetime aggregate=\"EngineSink\"\n");
    assert!(
        selected.contains("subscript_rt_cb_register(ctx,"),
        "{selected}"
    );
    assert!(
        selected.contains("&subscript_rt_cb_registration_trampoline"),
        "{selected}"
    );
    // The address is taken, so no call declares the symbol; the emitter
    // declares it where it is used.
    assert!(
        selected.contains(
            "extern void subscript_rt_cb_registration_trampoline(subscript_callback_string_view, \
             void*, void*);"
        ),
        "{selected}"
    );
    assert!(
        !selected.contains("subscript_rt_cb_bind(ctx,"),
        "the selected aggregate takes no Context-lifetime binding:\n{selected}"
    );

    // The firing control: the same program without the record keeps the
    // Context-lifetime crossing and declares no registration trampoline.
    let control = emitted_c("");
    assert!(control.contains("subscript_rt_cb_bind(ctx,"), "{control}");
    assert!(control.contains("&subscript_rt_cb_trampoline"), "{control}");
    // The generated runtime header names `subscript_rt_cb_registration_context`
    // in every emitted program, so these two read the call and the
    // address the marshaling writes, not the bare name.
    assert!(
        !control.contains("subscript_rt_cb_register(ctx,"),
        "{control}"
    );
    assert!(
        !control.contains("&subscript_rt_cb_registration_trampoline"),
        "{control}"
    );
}
