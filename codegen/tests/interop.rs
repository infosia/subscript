//! Foreign C-header binding: cross-tier differential tests.
//!
//! Each program exercises one Q13 boundary pattern through a real foreign
//! call against the committed synthetic header (`corpus/interop/`), whose
//! implementation (`interop.c`) is linked into both tiers. The assertion
//! is the one the ship=C decision rests on: **dev-JIT bytes == ship-C-AOT
//! bytes**, plus a check that the observable effect is the expected one.
//! The two tiers are each other's oracle here; this target commits no
//! golden.
//!
//! interop.c's behaviour: setLabel stores a label; setLogger fires the
//! callback once with the stored label as the message; submit sums the
//! (ptr,count) commands and fires the callback with a message of length
//! (sum + chain depth). The language callback accumulates
//! `message.length` into a userdata sink, so a program surfaces every
//! effect by printing the sink's count.

// Every test here binds the synthetic interop header, whose fixture compiles
// `corpus/interop/interop.c` (`_Float16`), unbuildable by MSVC `cl`. The whole
// target is excluded on windows-msvc (compiler.md §11c).
#![cfg(not(all(windows, target_env = "msvc")))]

#[path = "support/native_fixture.rs"]
mod native_fixture;

#[path = "../../compiler/tests/corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_codegen::{
    run_c_aot_with_freed_handle_diagnostics_and_native_libraries, run_c_aot_with_native_libraries,
    run_jit_with_freed_handle_diagnostics_and_native_libraries, run_jit_with_native_libraries,
    RunError,
};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

/// Runs `program` under both tiers, asserts byte-identical output, and
/// returns those bytes. A divergence is a hard failure (the cross-tier
/// equivalence the ship=C decision rests on), never papered over.
fn both_tiers(program: &str) -> Vec<u8> {
    let files = || {
        vec![
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("prog.ts", program),
        ]
    };
    let libraries = [native_fixture::fixture()
        .expect("this target requires the fixture")
        .library()];
    let jit = run_jit_with_native_libraries(&files(), &libraries)
        .unwrap_or_else(|e| panic!("dev-JIT run failed: {e}"));
    let ship = run_c_aot_with_native_libraries(&files(), &libraries)
        .unwrap_or_else(|e| panic!("ship-C-AOT run failed: {e}"));
    assert_eq!(
        jit,
        ship,
        "dev-JIT output {:?} != ship-C-AOT output {:?}",
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&ship)
    );
    jit
}

const SINK: &str = "\
class LogSink {
  count: i32;
  constructor() { this.count = 0; }
}
";

/// Handle lifecycle: create returns a handle, retain/release take it as an
/// opaque pointer. No callback, so no output beyond the marker.
fn handle_create_retain_release() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const chain: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null);
  const device: SubDevice = subDeviceCreate(chain);
  subDeviceRetain(device);
  subDeviceRelease(device);
  print(`ok`);
}}
"
    );
    assert_eq!(both_tiers(&prog), b"ok\n");
}

/// String label marshaling and the callback: setLogger fires the callback
/// with the stored label, so the sink accumulates the label's length.
fn string_label_round_trips_through_the_callback() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
    null,
  );
  subDeviceSetLabel(device, \"device-label\");
  subDeviceSetLogger(device, info);
  print(`${{sink.count}}`);
}}
"
    );
    // "device-label" is 12 bytes; setLogger fires the callback once with it.
    assert_eq!(both_tiers(&prog), b"12\n");
}

/// `(ptr, count)` array descriptor: submit sums the commands and fires the
/// callback with a message of length (sum + chain depth = 0). Also proves
/// the callback userdata (`object | null` narrowed with `as`).
fn buffer_view_sum_through_the_callback() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
    null,
  );
  subDeviceSetLogger(device, info);
  const commands: u32[] = [10, 20, 30];
  subDeviceSubmit(device, commands);
  print(`${{sink.count}}`);
}}
"
    );
    // No label (setLogger fires with length 0), then submit sum = 60,
    // chain depth = 0 (null chain): 0 + 60 = 60.
    assert_eq!(both_tiers(&prog), b"60\n");
}

/// Chain-slot address-of via the constructor: `new SubChainHeader(_, tail)`
/// stores the address of `tail`'s storage into the `Struct | null` slot.
/// create walks the chain (depth 2), surfaced through submit.
fn chain_slot_address_of_via_constructor() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const tail: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null);
  const head: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, tail);
  const device: SubDevice = subDeviceCreate(head);
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
    null,
  );
  subDeviceSetLogger(device, info);
  const commands: u32[] = [5];
  subDeviceSubmit(device, commands);
  print(`${{sink.count}}`);
}}
"
    );
    // chain depth 2, submit sum 5: 5 + 2 = 7.
    assert_eq!(both_tiers(&prog), b"7\n");
}

/// Chain-slot address-of via an explicit `next` assignment.
fn chain_slot_address_of_via_assignment() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const tail: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null);
  const head: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null);
  head.next = tail;
  const device: SubDevice = subDeviceCreate(head);
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
    null,
  );
  subDeviceSetLogger(device, info);
  const commands: u32[] = [5];
  subDeviceSubmit(device, commands);
  print(`${{sink.count}}`);
}}
"
    );
    assert_eq!(both_tiers(&prog), b"7\n");
}

/// An extension's embedded header crosses the chain slot by address, so
/// the callee can recover and fold the payload fields that follow it.
fn chain_extension_payload_is_read_through_its_embedded_header() {
    let prog = include_str!("../../corpus/accept/a89-interop-chain-payload.ts");
    let golden = include_bytes!("../../corpus/accept/a89-interop-chain-payload.expected");
    let output = both_tiers(prog);
    assert_eq!(golden, b"222\n");
    assert_eq!(output, golden);
    println!(
        "a89-interop-chain-payload: {:?}",
        String::from_utf8_lossy(&output)
    );
}

/// The §13.3 async model: a completion callback is registered
/// (subDeviceOnComplete) but not fired; intervening work runs; a
/// host-driven pump (subDevicePump) fires it after the registering call
/// returned. The sink is 0 at registration and after the intervening
/// submit, then nonzero after the pump. This proves the deferred fire.
/// It also proves that the userdata, and the Context-held callback
/// binding behind it, outlived the registration.
fn deferred_completion_callback_fires_on_pump() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: LogSink = new LogSink();
  const info: SubCompletionInfo = new SubCompletionInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
  );
  subDeviceOnComplete(device, info);
  print(`${{sink.count}}`);
  const commands: u32[] = [10, 20, 30];
  subDeviceSubmit(device, commands);
  print(`${{sink.count}}`);
  subDevicePump(device);
  print(`${{sink.count}}`);
}}
"
    );
    // Registered but not fired (0), still not fired after submit (0), fired by
    // the pump with message length = submit sum (60) + chain depth (0) = 60.
    assert_eq!(both_tiers(&prog), b"0\n0\n60\n");
}

fn callback_userdata_rooting_corpus_survives_collect_on_both_tiers() {
    let program = include_str!("../../corpus/accept/a90-callback-userdata-rooted.ts");
    let output = both_tiers(program);
    assert_eq!(output, b"41:7:0\n");
    println!(
        "a90-callback-userdata-rooted: {:?}",
        String::from_utf8_lossy(&output)
    );
}

fn callback_userdata_fire_check_traps_identically_on_both_tiers() {
    let files = || {
        vec![
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new(
                "t46-callback-userdata-freed.ts",
                include_str!("../../corpus/trap/t46-callback-userdata-freed.ts"),
            ),
        ]
    };
    let libraries = [native_fixture::fixture()
        .expect("this target requires the fixture")
        .library()];
    let jit = run_jit_with_freed_handle_diagnostics_and_native_libraries(&files(), &libraries);
    let ship = run_c_aot_with_freed_handle_diagnostics_and_native_libraries(&files(), &libraries);

    let report = match (jit, ship) {
        (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
            assert_eq!(jit, ship, "diagnostic trap tuple differs by tier");
            jit
        }
        (jit, ship) => panic!("expected both tiers to trap; jit={jit:?}, ship={ship:?}"),
    };
    assert_eq!(report.rule, TrapKind::CallbackUserdataFreed);
    assert_eq!(
        report.message,
        "callback userdata points to a freed allocation"
    );
    assert_eq!(report.pos.file, "t46-callback-userdata-freed.ts");
    assert_eq!(report.pos.line, 31);
    assert_eq!(report.stdout, b"registered\nfreed\n");
    println!(
        "t46-callback-userdata-freed: kind={}, message={:?}, position={}, stdout={:?}",
        report.rule,
        report.message,
        report.pos,
        String::from_utf8_lossy(&report.stdout)
    );
}

/// All five patterns composed in one program: chain + handle + string +
/// (ptr,count) + callback with userdata.
fn all_patterns_composed() {
    let prog = format!(
        "{SINK}export function main(): void {{
  const chain: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null);
  const device: SubDevice = subDeviceCreate(chain);
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }}
    }},
    sink,
    null,
  );
  subDeviceSetLabel(device, \"device-label\");
  subDeviceSetLogger(device, info);
  const commands: u32[] = [1, 2, 3];
  subDeviceSubmit(device, commands);
  subDeviceRetain(device);
  subDeviceRelease(device);
  print(`${{sink.count}}`);
}}
"
    );
    // setLogger fires with label length 12; submit sum 6 + depth 1 = 7;
    // total 19.
    assert_eq!(both_tiers(&prog), b"19\n");
}

/// Foreign-call arguments are evaluated left to right, like every other
/// operand list: the receiver-shaped first argument runs before the
/// second, whose ternary lowers to statements of its own. The dev tier
/// marshals in argument order, so the ship tier must not let the
/// marshaling of a later argument overtake an earlier one.
fn foreign_call_arguments_run_left_to_right() {
    let prog = "let log: string = \"\";\nlet pick: boolean = true;\nfunction note(tag: string): void {\n  log = log + tag;\n}\nfunction mkDevice(): SubDevice {\n  note(\"D\");\n  return subDeviceCreate(null);\n}\nfunction mkLabel(): string {\n  note(\"L\");\n  return \"abc\";\n}\nexport function main(): void {\n  subDeviceSetLabel(mkDevice(), pick ? mkLabel() : \"\");\n  print(log);\n}\n";
    assert_eq!(both_tiers(prog), b"DL\n");
}

fn foreign_call_operands_survive_a_later_suspension() {
    let program = "async function av(value: i32): Promise<i32> {\n  await Context.suspend();\n  return value;\n}\nexport async function main(): Promise<void> {\n  const queue: SubDevice = subDeviceCreate(null);\n  const first: SubDevice = subDeviceCreate(null);\n  const second: SubDevice = subDeviceCreate(null);\n  const commands: SubDevice[] = [first, second];\n  const probe: u64 = subProbeQueueSubmitCheck(queue, commands, (await av(2)) as u32);\n  print(`probe=${probe}`);\n  subDeviceRelease(queue);\n  subDeviceRelease(first);\n  subDeviceRelease(second);\n}\n";
    assert_eq!(both_tiers(program), b"probe=2\n");
}

fn foreign_call_without_suspension_uses_call_time_view() {
    let program = "function grow(commands: SubDevice[], device: SubDevice): u32 {\n  commands.push(device);\n  return 0;\n}\nexport function main(): void {\n  const queue: SubDevice = subDeviceCreate(null);\n  const first: SubDevice = subDeviceCreate(null);\n  const second: SubDevice = subDeviceCreate(null);\n  const third: SubDevice = subDeviceCreate(null);\n  const commands: SubDevice[] = [first, second];\n  const probe: u64 = subProbeQueueSubmitCheck(queue, commands, grow(commands, third));\n  print(`f2=${probe} len=${commands.length}`);\n  subDeviceRelease(queue);\n  subDeviceRelease(first);\n  subDeviceRelease(second);\n  subDeviceRelease(third);\n}\n";
    assert_eq!(both_tiers(program), b"f2=3 len=3\n");
}

fn foreign_call_rule_7a_plain_twin_uses_call_time_view() {
    let program = "async function av(value: u32): Promise<u32> {\n  await Context.suspend();\n  return value;\n}\nfunction grow(commands: SubDevice[], device: SubDevice, value: u32): u32 {\n  commands.push(device);\n  return value;\n}\nexport async function main(): Promise<void> {\n  const queue: SubDevice = subDeviceCreate(null);\n  const first: SubDevice = subDeviceCreate(null);\n  const second: SubDevice = subDeviceCreate(null);\n  const third: SubDevice = subDeviceCreate(null);\n  const commands: SubDevice[] = [first, second];\n  const probe: u64 = subProbeQueueSubmitCheck(queue, commands, grow(commands, third, 0));\n  print(`f2sync=${probe} len=${commands.length}`);\n  subDeviceRelease(queue);\n  subDeviceRelease(first);\n  subDeviceRelease(second);\n  subDeviceRelease(third);\n}\n";
    assert_eq!(both_tiers(program), b"f2sync=3 len=3\n");
}

fn foreign_call_rule_7a_later_suspension_uses_call_time_view() {
    // §68.7.3 applies the same call-time view after a later suspension.
    let program = "async function av(value: u32): Promise<u32> {\n  await Context.suspend();\n  return value;\n}\nfunction grow(commands: SubDevice[], device: SubDevice, value: u32): u32 {\n  commands.push(device);\n  return value;\n}\nexport async function main(): Promise<void> {\n  const queue: SubDevice = subDeviceCreate(null);\n  const first: SubDevice = subDeviceCreate(null);\n  const second: SubDevice = subDeviceCreate(null);\n  const third: SubDevice = subDeviceCreate(null);\n  const commands: SubDevice[] = [first, second];\n  const probe: u64 = subProbeQueueSubmitCheck(queue, commands, grow(commands, third, await av(0)));\n  print(`f2suspend=${probe} len=${commands.length}`);\n  subDeviceRelease(queue);\n  subDeviceRelease(first);\n  subDeviceRelease(second);\n  subDeviceRelease(third);\n}\n";
    assert_eq!(both_tiers(program), b"f2suspend=3 len=3\n");
}

/// A mirror boundary struct's `new` is a positional field initializer
/// list; its fields are evaluated left to right too.
fn boundary_struct_field_initializers_run_left_to_right() {
    let prog = "let log: string = \"\";\nlet pick: boolean = true;\nfunction note(tag: string): void {\n  log = log + tag;\n}\nfunction mkKind(): SubChainKind {\n  note(\"H\");\n  return SubChainKind.SUB_CHAIN_KIND_BASE;\n}\nfunction mkIntensity(): f32 {\n  note(\"I\");\n  return 1.5;\n}\nexport function main(): void {\n  const ext: SubChainExtA = new SubChainExtA(new SubChainHeader(mkKind(), null), pick ? mkIntensity() : 0.0, 2);\n  print(`${log}:${ext.intensity}`);\n}\n";
    assert_eq!(both_tiers(prog), b"HI:1.5\n");
}

fn boundary_struct_new_arguments_survive_suspension() {
    let program = "async function av(label: string, value: i32): Promise<i32> {\n  print(`av:${label}`);\n  await Context.suspend();\n  return value;\n}\nexport async function main(): Promise<void> {\n  const rect: SubRect = new SubRect(1, await av(\"rect-y\", 2), 3 as u32, 4 as u32);\n  print(`rect=${rect.x},${rect.y},${rect.width},${rect.height}`);\n}\n";
    assert_eq!(both_tiers(program), b"av:rect-y\nrect=1,2,3,4\n");
}

// ----- §111.2 the host adapter of the fixture -----
//
// The adapter must end every registration it receives exactly one time,
// at the point where no later fire can occur. Each test below drives one
// shape the adapter must survive and reads the release count, which
// counts the releases the runtime answered with 1.

/// The userdata every adapter test registers. The class is a reference
/// class, so each instance is one Context allocation the registration
/// roots.
const REQUEST_SINK: &str = "\
class RequestSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}
";

/// §111.2: two deferred one-shots live at the same time. The adapter
/// holds both, fires both at the pump, and ends each one time.
fn two_deferred_one_shots_each_end_one_time() {
    let program = format!(
        "{REQUEST_SINK}\
function start(device: SubDevice, sink: RequestSink, payload: u32): i32 {{
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const state = userdata1 as RequestSink;
        state.count = state.count + message.length;
        print(`fired ${{state.count}}`);
      }}
    }},
    sink,
    null,
  );
  return subRequestStart(device, payload, 0, info);
}}
export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: RequestSink = new RequestSink();
  print(`first ${{start(device, sink, 3)}}`);
  print(`second ${{start(device, sink, 5)}}`);
  subRequestPump(device);
  print(`released ${{subRequestReleaseCount(device)}}`);
  subDeviceRelease(device);
}}
"
    );
    assert_eq!(
        both_tiers(&program),
        b"first 1\nsecond 2\nfired 3\nfired 8\nreleased 2\n"
    );
}

/// §111.2: a start that meets a full table is refused. The refused
/// crossing still received a registration, so the adapter ends it at
/// once and answers -1.
fn a_start_over_the_table_capacity_is_refused_and_released_at_once() {
    let program = format!(
        "{REQUEST_SINK}\
function start(device: SubDevice, sink: RequestSink): i32 {{
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const state = userdata1 as RequestSink;
        state.count = state.count + 1;
      }}
    }},
    sink,
    null,
  );
  return subRequestStart(device, 1, 0, info);
}}
export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: RequestSink = new RequestSink();
  // The table holds four pending one-shots; the fifth is refused.
  for (let index: i32 = 0; index < 5; index = index + 1) {{
    print(`start ${{start(device, sink)}}`);
  }}
  // The refused crossing ended at once, and no other did.
  print(`released ${{subRequestReleaseCount(device)}}`);
  subRequestPump(device);
  print(`fired ${{sink.count}}`);
  print(`released ${{subRequestReleaseCount(device)}}`);
  subDeviceRelease(device);
}}
"
    );
    assert_eq!(
        both_tiers(&program),
        b"start 1\nstart 2\nstart 3\nstart 4\nstart -1\nreleased 1\nfired 4\nreleased 5\n"
    );
}

/// §111.2: one subscription at a time. A second subscribe is refused,
/// and its registration ends at once instead of leaking.
fn a_second_subscribe_is_refused_and_released_at_once() {
    let program = format!(
        "{REQUEST_SINK}\
function subscribe(device: SubDevice, sink: RequestSink): i32 {{
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {{
      if (userdata1 !== null) {{
        const state = userdata1 as RequestSink;
        state.count = state.count + message.length;
        print(`note ${{state.count}}`);
      }}
    }},
    sink,
    null,
  );
  return subRequestSubscribe(device, info);
}}
export function main(): void {{
  const device: SubDevice = subDeviceCreate(null);
  const sink: RequestSink = new RequestSink();
  print(`first ${{subscribe(device, sink)}}`);
  print(`second ${{subscribe(device, sink)}}`);
  print(`released ${{subRequestReleaseCount(device)}}`);
  subRequestNotify(device, 4);
  subRequestPump(device);
  subRequestUnsubscribe(device);
  subRequestPump(device);
  print(`released ${{subRequestReleaseCount(device)}}`);
  subDeviceRelease(device);
}}
"
    );
    assert_eq!(
        both_tiers(&program),
        b"first 1\nsecond -1\nreleased 1\nnote 4\nreleased 2\n"
    );
}

/// The pump fires what is queued when the drain starts. A notification a
/// callback queues during the drain overwrites nothing and fires at the
/// next pump.
fn a_notification_queued_during_the_drain_fires_at_the_next_pump() {
    // The callback reaches the device through its own userdata, because
    // a boundary callback captures nothing (C5).
    let program = "\
class NotifySink {
  device: SubDevice;
  count: i32;
  constructor(device: SubDevice) {
    this.device = device;
    this.count = 0;
  }
}
export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const sink: NotifySink = new NotifySink(device);
  const info: SubRequestInfo = new SubRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const state = userdata1 as NotifySink;
        state.count = state.count + 1;
        print(`note ${state.count}:${message.length}`);
        if (state.count === 1) {
          // The first notification of the drain queues two more. The
          // drain reads its own copy, so every entry queued before the
          // pump still fires in this drain, in order, and these two
          // wait for the next pump.
          subRequestNotify(state.device, 9);
          subRequestNotify(state.device, 10);
        }
      }
    },
    sink,
    null,
  );
  subRequestSubscribe(device, info);
  subRequestNotify(device, 2);
  subRequestNotify(device, 3);
  subRequestNotify(device, 4);
  subRequestPump(device);
  print(\"drained\");
  subRequestPump(device);
  subRequestUnsubscribe(device);
  subRequestPump(device);
  print(`released ${subRequestReleaseCount(device)}`);
  subDeviceRelease(device);
}
";
    assert_eq!(
        both_tiers(program),
        b"note 1:2\nnote 2:3\nnote 3:4\ndrained\nnote 4:9\nnote 5:10\nreleased 1\n"
    );
}

fn foreign_call_keeps_borrowed_storage_corpus() {
    let program = include_str!("../../corpus/accept/a473-foreign-call-keeps-borrowed-storage.ts");
    let expected =
        include_bytes!("../../corpus/accept/a473-foreign-call-keeps-borrowed-storage.expected");
    assert_eq!(both_tiers(program), expected);
}

fn foreign_array_growth_and_nested_borrows_trap_at_the_push() {
    for nested in [false, true] {
        let inner = if nested {
            "const quiet = new SubCallbackInfo((message, first, second) => {}, null, null);\n      subBorrowArray(state.values, quiet);"
        } else {
            ""
        };
        let program = format!(
            r#"
class State {{
  values: i32[];
  constructor() {{ this.values = [1, 2, 3, 4]; }}
}}
export function main(): void {{
  const state = new State();
  const info = new SubCallbackInfo((message, first, second) => {{
    if (first !== null) {{
      const state = first as State;
      {inner}
      state.values.push(5);
    }}
  }}, state, null);
  subBorrowArray(state.values, info);
}}
"#
        );
        let sources = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("borrow.ts", &program),
        ];
        let libraries = [native_fixture::fixture().expect("native fixture").library()];
        let jit = run_jit_with_native_libraries(&sources, &libraries);
        let ship = run_c_aot_with_native_libraries(&sources, &libraries);
        let (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) = (jit, ship) else {
            panic!("borrowed growth must trap in each tier");
        };
        assert_eq!(jit, ship);
        assert_eq!(jit.rule, TrapKind::ArrayStorageBorrowed);
        assert_eq!(
            jit.message,
            "the array storage is borrowed by a native call"
        );
        assert_eq!(jit.pos.file, "borrow.ts");
        assert_eq!(
            jit.pos.line as usize,
            program
                .lines()
                .position(|line| line.contains("state.values.push(5)"))
                .expect("push position")
                + 1
        );
        assert_eq!(jit.pos.col, 7);
        println!(
            "nested={nested}: {}: {} at {}",
            jit.rule, jit.message, jit.pos
        );
    }
}

fn foreign_member_array_growth_traps_at_the_push() {
    let program = r#"
class State {
  values: u32[];
  constructor() { this.values = [1, 2, 3, 4]; }
}
export function main(): void {
  const state = new State();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as State;
      state.values.push(5);
    }
  }, state, null);
  subBorrowMember(new SubDrawList(0, state.values), info);
}
"#;
    let sources = [
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("member.ts", program),
    ];
    let libraries = [native_fixture::fixture().expect("native fixture").library()];
    let jit = run_jit_with_native_libraries(&sources, &libraries);
    let ship = run_c_aot_with_native_libraries(&sources, &libraries);
    let (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) = (jit, ship) else {
        panic!("borrowed member growth must trap in each tier");
    };
    assert_eq!(jit, ship);
    assert_eq!(jit.rule, TrapKind::ArrayStorageBorrowed);
    assert_eq!(
        jit.message,
        "the array storage is borrowed by a native call"
    );
    assert_eq!(jit.pos.file, "member.ts");
    assert_eq!(jit.pos.line, 11);
    assert_eq!(jit.pos.col, 7);
}

fn foreign_array_element_pointer_growth_traps_at_the_push() {
    let program = r#"
class State {
  values: SubByValueI32One[];
  constructor() { this.values = [new SubByValueI32One(1), new SubByValueI32One(2), new SubByValueI32One(3), new SubByValueI32One(4)]; }
}
export function main(): void {
  const state = new State();
  const info = new SubCallbackInfo((message, first, second) => {
    if (first !== null) {
      const state = first as State;
      state.values.push(new SubByValueI32One(5));
    }
  }, state, null);
  subBorrowPointer(state.values[0], info);
}
"#;
    let sources = [
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("element.ts", program),
    ];
    let libraries = [native_fixture::fixture().expect("native fixture").library()];
    let jit = run_jit_with_native_libraries(&sources, &libraries);
    let ship = run_c_aot_with_native_libraries(&sources, &libraries);
    let (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) = (jit, ship) else {
        panic!("growth must trap while C holds an element pointer");
    };
    assert_eq!(jit, ship);
    assert_eq!(jit.rule, TrapKind::ArrayStorageBorrowed);
    assert_eq!(
        jit.message,
        "the array storage is borrowed by a native call"
    );
    assert_eq!(jit.pos.file, "element.ts");
    assert_eq!(jit.pos.line, 11);
    assert_eq!(jit.pos.col, 7);
}

fn foreign_field_pointer_keeps_its_owner_across_a_later_await() {
    let program = r#"
class Owner {
  tag: i32;
  value: SubByValueI32One;
  constructor() { this.tag = 0; this.value = new SubByValueI32One(1); }
}
async function later(info: SubCallbackInfo): Promise<SubCallbackInfo> {
  await Context.suspend();
  Context.collect();
  return info;
}
export async function main(): Promise<void> {
  const owner = new Owner();
  const info = new SubCallbackInfo((message, first, second) => { Context.collect(); }, null, null);
  print(`${subBorrowPointer(owner.value, await later(info))}`);
}
"#;
    assert_eq!(both_tiers(program), b"101\n");
}

// compiler.md §326 rule 2: a byte element uses the compare scope of one embedded struct.
// Cost: one native run per tier, with one C compiler process for ship.
fn fixed_struct_array_byte_elements_compare_as_embedded_members() {
    let program = r#"
class ByteState { out: Sub326ByteCompare | null = null; }
let active: ByteState | null = null;
function run(state: ByteState): void {
  const out = state.out;
  if (out !== null) {
    const info = new SubCallbackInfo((message, userdata, userparam) => {
      const current = active;
      if (current !== null) {
        const value = current.out;
        if (value !== null) { value.ps[0].y = 88; value.ps[1].y = 99; }
      }
    }, null, null);
    sub326ByteCompare(out, info);
    print(`${out.ps[0].x} ${out.ps[0].y} ${out.ps[1].x} ${out.ps[1].y}`);
  }
}
export function main(): void {
  const state = new ByteState();
  state.out = new Sub326ByteCompare([new Sub326Point(1, 2), new Sub326Point(3, 4)], "name");
  active = state;
  run(state);
}
"#;
    assert_eq!(both_tiers(program), b"1 88 13 4\n");
}

// compiler.md §326 rule 2: nested elements retain arrays and indexed diagnostics.
// Cost: three programs per tier; each ship run invokes the C compiler once.
fn fixed_struct_arrays_keep_nested_receives_and_compares() {
    let libraries = [native_fixture::fixture().expect("fixture").library()];
    let nested = "const a = new Sub326Pairs(0, [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])]); const b = new Sub326Pairs(0, [new SubDescReadLay(3, [11]), new SubDescReadLay(4, [21, 22])]); const out = new Sub326Nested([a, b]);";
    for (source, expected, error) in [
        (format!("export function main(): void {{ {nested} sub326Nested(out, 0); Context.collect(); print(`${{out.groups[1].qs[1].items.length}} ${{out.groups[1].qs[1].items[0]}} ${{out.groups[0].qs[1].items[0]}}`); }}"), "1 21 8\n", None),
        (format!("export function main(): void {{ {nested} sub326Nested(out, 1); }}"), "", Some("sub326Nested.out.groups[1].qs[1].items: C returned an invalid pointer/count pair")),
        ("export function main(): void { const out = new Sub326Links([new Sub325Link(1, null), new Sub325Link(2, null)], \"name\"); sub326Links(out); }".into(), "", Some("sub326Links.out.links[1].leaf: C replaced a pointer that the script cannot receive")),
    ] {
        let files = [interop::mirror("interop.generated.d.ts", SourceFile::ambient), SourceFile::new("fixed.ts", source)];
        let jit = run_jit_with_native_libraries(&files, &libraries);
        let ship = run_c_aot_with_native_libraries(&files, &libraries);
        if let Some(message) = error {
            match (jit, ship) {
                (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
                    assert_eq!(jit, ship);
                    assert_eq!(jit.rule, TrapKind::ForeignPointerReplaced);
                    assert_eq!(jit.message, message);
                }
                (jit, ship) => panic!("indexed fixed array trap: {jit:?}, {ship:?}"),
            }
        } else {
            assert_eq!(jit.expect("nested JIT"), expected.as_bytes());
            assert_eq!(ship.expect("nested ship"), expected.as_bytes());
        }
    }
}

// compiler.md §327 acceptance 1: invalid bytes come from the C fixture.
// Cost: one corpus compilation and execution per tier.
fn c_bytes_become_a_valid_string() {
    assert_eq!(
        both_tiers(include_str!(
            "../../corpus/accept/a477-c-bytes-become-a-valid-string.ts"
        )),
        include_bytes!("../../corpus/accept/a477-c-bytes-become-a-valid-string.expected")
    );
}

// compiler.md §326 acceptance 1: one corpus run per tier.
fn fixed_struct_array_members_use_each_element_pass() {
    let source = include_str!("../../corpus/accept/a476-fixed-struct-array-members.ts");
    let golden = include_bytes!("../../corpus/accept/a476-fixed-struct-array-members.expected");
    assert_eq!(both_tiers(source), golden);
}

// compiler.md §324 acceptances 1, 2 and 6. One fixture run per tier.
fn fixed_array_parameters_share_places_and_materialize_temporaries() {
    let source = include_str!("../../corpus/accept/a474-c-array-parameter-storage.ts");
    let golden = include_bytes!("../../corpus/accept/a474-c-array-parameter-storage.expected");
    assert_eq!(both_tiers(source), golden);
}

// compiler.md §324 rule 1: an element place keeps its bounds check.
fn fixed_array_parameter_checks_an_element_place() {
    let files = [
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("element.ts", "export function main(): void { const values: FixedArray<i32, 4>[] = []; subFixedSum(values[0]); }"),
    ];
    let libraries = [native_fixture::fixture().expect("native fixture").library()];
    let jit = run_jit_with_native_libraries(&files, &libraries);
    let ship = run_c_aot_with_native_libraries(&files, &libraries);
    match (jit, ship) {
        (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
            assert_eq!(jit, ship);
            assert_eq!(jit.rule, TrapKind::IndexOutOfBounds);
            assert_eq!(jit.pos.file, "element.ts");
        }
        (jit, ship) => panic!("expected element bounds traps: jit={jit:?}, ship={ship:?}"),
    }
}

// compiler.md §324 rule 1: suspended locals retain their array storage.
fn fixed_array_locals_keep_storage_across_await_and_yield() {
    let program = r#"
function* filledAfterYield(): Generator<i32> {
  const values: FixedArray<i32, 4> = [10, 20, 30, 40];
  yield 0;
  Context.collect();
  subFixedInts(values);
  print(`${values[0]} ${values[1]} ${values[2]} ${values[3]}`);
}
export async function main(): Promise<void> {
  const values: FixedArray<i32, 4> = [1, 2, 3, 4];
  await Context.suspend();
  Context.collect();
  subFixedInts(values);
  print(`${values[0]} ${values[1]} ${values[2]} ${values[3]}`);
  for (const step of filledAfterYield()) { Context.collect(); }
}
"#;
    assert_eq!(both_tiers(program), b"2 4 6 8\n11 22 33 44\n");
}

// compiler.md §325 acceptance 1: C replacements enter script storage.
fn c_replaced_members_are_received() {
    assert_eq!(
        both_tiers(include_str!(
            "../../corpus/accept/a475-c-replaced-members.ts"
        )),
        include_bytes!("../../corpus/accept/a475-c-replaced-members.expected")
    );
}

// compiler.md §325 rule 2: retain input arrays before a callback can replace the script member.
fn a_count_shrink_keeps_the_pre_callback_array() {
    for call in ["sub325ShrinkReentrant(out)", "sub325ShrinkIndirect(holder)"] {
        let program = format!("let active: Sub325ReentrantHolder | null = null;\nexport function main(): void {{\n  const items: Sub325Link[] = [new Sub325Link(1, new Sub325Leaf(71)), new Sub325Link(2, null)];\n  const holder = new Sub325ReentrantHolder(new Sub325Reentrant(items, new SubCallbackInfo((message, userdata, userparam) => {{ const h = active; if (h !== null) {{ const o = h.out; if (o !== null) {{ const empty: Sub325Link[] = []; o.items = empty; }} }} }}, null, null)));\n  active = holder;\n  const out = holder.out;\n  if (out !== null) {{ {call}; const leaf = out.items[0].leaf; if (leaf !== null) print(`${{out.items.length}} ${{leaf.value}}`); }}\n}}\n");
        assert_eq!(both_tiers(&program), b"1 71\n", "{call}");
    }
    let program = "let active: Sub325WatchGroup[] = [];\nexport function main(): void {\n const items: Sub325Link[] = [new Sub325Link(1, new Sub325Leaf(71)), new Sub325Link(2, null)];\n const groups: Sub325WatchGroup[] = [new Sub325WatchGroup(\"kept\", items), new Sub325WatchGroup(\"removed\", items)];\n active = groups;\n const out = new Sub325ReentrantGroups(groups, new SubCallbackInfo((message, userdata, userparam) => { const empty: Sub325Link[] = []; active[0].items = empty; }, null, null));\n sub325ShrinkReentrantGroups(out);\n const leaf = out.groups[0].items[0].leaf;\n if (leaf !== null) print(`${out.groups.length} ${out.groups[0].items.length} ${leaf.value}`);\n}\n";
    assert_eq!(both_tiers(program), b"1 2 71\n");
}

// compiler.md §325 acceptance 2: every fixture executes the offending C write.
// Cost: twelve runs per tier; each ship run compiles one small program.
fn c_pointer_replacements_report_the_same_trap_in_both_tiers() {
    let callback = "new SubCallbackInfo((message, userdata, userparam) => {}, null, null)";
    let cases = [
        (
            "const items: Sub325Link[] = [new Sub325Link(0, new Sub325Leaf(9))]; const out = new Sub325MutableLinks(items);".to_string(),
            "sub325ShrinkMutable(out, 1)",
            "sub325ShrinkMutable.out.items",
            "C returned an invalid pointer/count pair",
        ),
        (
            "const items: Sub325Link[] = [new Sub325Link(0, new Sub325Leaf(9)), new Sub325Link(1, null)]; const out = new Sub325MutableLinks(items);".to_string(),
            "sub325ShrinkMutable(out, 2)",
            "sub325ShrinkMutable.out.items[0].leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const out = new Sub325Scratch([1], new Sub325Leaf(1));".to_string(),
            "sub325Pointer(out)",
            "sub325Pointer.out.leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            format!("const out = {callback};"),
            "sub325Userdata(out)",
            "sub325Userdata.out.userdata",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            format!("const out = {callback};"),
            "sub325Callback(out)",
            "sub325Callback.out.callback",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const out = new Sub325Link(1, new Sub325Leaf(1));".to_string(),
            "sub325Bytes(out)",
            "sub325Bytes.out.leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const out = new Sub325List(1, [1]);".to_string(),
            "sub325BadPair(out)",
            "sub325BadPair.out.items",
            "C returned an invalid pointer/count pair",
        ),
        (
            "const items: Sub325Link[] = []; const out = new Sub325Links(1, items);".to_string(),
            "sub325Received(out)",
            "sub325Received.out.items[0].leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const items: Sub325TextLink[] = []; const out = new Sub325TextLinks(1, items);"
                .to_string(),
            "sub325ReceivedScratch(out)",
            "sub325ReceivedScratch.out.items[0].leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const out = new Sub325Embedded(\"before\", new Sub325Link(0, null));".to_string(),
            "sub325Embedded(out)",
            "sub325Embedded.out.inner.leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const out = new Sub325Graph(new Sub325Link(0, null));".to_string(),
            "sub325Graph(out)",
            "sub325Graph.out.link.leaf",
            "C replaced a pointer that the script cannot receive",
        ),
        (
            "const items: Sub325Link[] = [new Sub325Link(0, null)]; const groups: Sub325MutableLinks[] = [new Sub325MutableLinks(items)]; const out = new Sub325Nested(groups);".to_string(),
            "sub325Nested(out)", "sub325Nested.out.groups[0].items[0].leaf", "C replaced a pointer that the script cannot receive",
        ),
    ];
    let libraries = [native_fixture::fixture().expect("native fixture").library()];
    for (setup, call, path, reason) in cases {
        let program = format!("export function main(): void {{\n  {setup}\n  {call};\n}}\n");
        let files = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("replacement.ts", program),
        ];
        let jit = run_jit_with_native_libraries(&files, &libraries);
        let ship = run_c_aot_with_native_libraries(&files, &libraries);
        match (jit, ship) {
            (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
                assert_eq!(jit, ship, "{call}");
                assert_eq!(jit.rule, TrapKind::ForeignPointerReplaced);
                assert_eq!(jit.message, format!("{path}: {reason}"));
                assert_eq!(
                    jit.pos,
                    subscript_compiler::Pos::new("replacement.ts", 3, 3)
                );
            }
            (jit, ship) => {
                panic!("{call}: expected pointer replacement traps: jit={jit:?}, ship={ship:?}")
            }
        }
    }
}

// compiler.md §325 rule 4: an embedded external-mirror userdata slot is compared.
// Cost: two programs per tier, each ship program has one C compiler process.
fn an_external_embedded_userdata_slot_is_compared() {
    let mirror = "// @subscript-c-header include=\"external-userdata.h\"\ndeclare class Sub325ExternalUd { tag: i32; inner: SubDescReadUd; constructor(tag: i32, inner: SubDescReadUd); }\ndeclare function sub325ExternalKeep(out: Sub325ExternalUd | null): void;\ndeclare function sub325ExternalReplace(out: Sub325ExternalUd | null): void;\n";
    let libraries = [native_fixture::fixture().expect("fixture").library()];
    for replace in [false, true] {
        let call = if replace {
            "sub325ExternalReplace"
        } else {
            "sub325ExternalKeep"
        };
        let files = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            interop::mirror("external-device.generated.d.ts", SourceFile::ambient),
            SourceFile::ambient("external-userdata.d.ts", mirror),
            SourceFile::new("external-userdata.ts", format!("export function main(): void {{\n  const out = new Sub325ExternalUd(0, new SubDescReadUd(4, null));\n  {call}(out);\n  print(`${{out.inner.k}} ${{out.inner.ud === null}}`);\n}}")),
        ];
        let jit = run_jit_with_native_libraries(&files, &libraries);
        let ship = run_c_aot_with_native_libraries(&files, &libraries);
        if replace {
            match (jit, ship) {
                (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
                    assert_eq!(jit, ship);
                    assert_eq!(jit.rule, TrapKind::ForeignPointerReplaced);
                    assert_eq!(
                        jit.pos,
                        subscript_compiler::Pos::new("external-userdata.ts", 3, 3)
                    );
                    assert_eq!(jit.message, "sub325ExternalReplace.out.inner.ud: C replaced a pointer that the script cannot receive");
                }
                (jit, ship) => panic!("expected replacement trap: {jit:?}, {ship:?}"),
            }
        } else {
            assert_eq!(jit.expect("unchanged JIT"), b"5 true\n");
            assert_eq!(ship.expect("unchanged ship"), b"5 true\n");
        }
    }
}

// compiler.md §325 rules 5 and 9: finite arrays use loops; recursive headers keep the walker.
// Cost: five programs per tier. Each ship program starts one C compiler.
fn finite_array_watches_and_recursive_headers_match_both_tiers() {
    let libraries = [native_fixture::fixture().expect("fixture").library()];
    let setup = "const items: Sub325Link[] = [new Sub325Link(0, null), new Sub325Link(1, null), new Sub325Link(2, null), new Sub325Link(3, null), new Sub325Link(4, null), new Sub325Link(5, null), new Sub325Link(6, null), new Sub325Link(7, null)]; const out = new Sub325MutableLinks(items);";
    for (recursive, replace) in [(false, false), (false, true), (true, false), (true, true)] {
        let setup = if recursive {
            "const out = new Sub325Recursive(1, new Sub325Recursive(2, null, null), null);"
        } else {
            setup
        };
        let call = if recursive {
            "sub325Recursive"
        } else {
            "sub325WatchEight"
        };
        let program = format!("export function main(): void {{\n  {setup}\n  {call}(out, {});\n  print(\"kept\");\n}}", i32::from(replace));
        let files = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("watch.ts", program),
        ];
        let emitted = subscript_codegen::emit_c(
            &subscript_compiler::check_program(&files).expect("watch checks"),
        )
        .expect("C emission");
        assert_eq!(
            emitted
                .source
                .contains("subscript_rt_boundary_watch_finish("),
            recursive
        );
        assert_eq!(
            emitted
                .source
                .contains("subscript_rt_boundary_watch_buffer("),
            !recursive
        );
        if !recursive {
            let call = emitted
                .source
                .split(" = subscript_rt_boundary_watch_buffer(")
                .nth(1)
                .expect("buffer acquisition");
            assert!(
                call.split('\n')
                    .next()
                    .expect("buffer call")
                    .contains("* 1u"),
                "one saved word per leaf slot"
            );
            let before_call = call.split("sub325WatchEight(").next().expect("call site");
            assert!(
                !before_call.contains("subscript_rt_array_len("),
                "length must be evaluated before the buffer call"
            );
            assert!(
                !before_call.contains("subscript_rt_array_data("),
                "data must be evaluated before the buffer call"
            );
            let loop_start = before_call.find("for (uint64_t").expect("capture loop");
            let body_start =
                before_call[loop_start..].find('{').expect("loop body") + loop_start + 1;
            let body_end = before_call[body_start..].find('}').expect("loop end") + body_start;
            let body = &before_call[body_start..body_end];
            assert!(!body.contains(" = 0;"), "each saved slot is written once");
            assert!(
                !body.contains("(uintptr_t)"),
                "slot addresses are derived from the saved span"
            );
            assert_eq!(
                body.matches("memcpy(").count(),
                1,
                "one before-value load per leaf slot"
            );
        }
        let jit = run_jit_with_native_libraries(&files, &libraries);
        let ship = run_c_aot_with_native_libraries(&files, &libraries);
        if replace {
            match (jit, ship) {
                (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
                    assert_eq!(jit, ship);
                    assert_eq!(jit.rule, TrapKind::ForeignPointerReplaced);
                    assert_eq!(jit.pos, subscript_compiler::Pos::new("watch.ts", 3, 3));
                    let member = if recursive {
                        "userdata"
                    } else {
                        "items[6].leaf"
                    };
                    assert_eq!(jit.message, format!("{call}.out.{member}: C replaced a pointer that the script cannot receive"));
                }
                (jit, ship) => panic!("replacement trap: {jit:?}, {ship:?}"),
            }
        } else {
            assert_eq!(jit.expect("JIT keeps slots"), b"kept\n");
            assert_eq!(ship.expect("ship keeps slots"), b"kept\n");
        }
    }
    let files = [interop::mirror("interop.generated.d.ts", SourceFile::ambient), SourceFile::new("null.ts", "export function main(): void { const out = new Sub325Graph(null); sub325Graph(out); print(`${out.link === null}`); }")];
    let emitted = subscript_codegen::emit_c(
        &subscript_compiler::check_program(&files).expect("watch checks"),
    )
    .expect("finite C emission");
    assert!(!emitted
        .source
        .contains("subscript_rt_boundary_watch_finish("));
    assert!(!emitted
        .source
        .contains("subscript_rt_boundary_watch_buffer("));
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("null JIT"),
        b"true\n"
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("null ship"),
        b"true\n"
    );
}

// compiler.md §325 rules 5 and 6: scratch outer elements keep watches of shared inner arrays.
// Cost: two programs per tier, each with one C compiler process for ship.
fn nested_scratch_elements_keep_nonzero_watch_indices() {
    let libraries = [native_fixture::fixture().expect("fixture").library()];
    for replace in [0, 1] {
        let program = format!("export function main(): void {{\n  const empty: Sub325Link[] = []; const items: Sub325Link[] = [new Sub325Link(0, null), new Sub325Link(1, null), new Sub325Link(2, null), new Sub325Link(3, null), new Sub325Link(4, null), new Sub325Link(5, null), new Sub325Link(6, null), new Sub325Link(7, null)];\n  const groups: Sub325WatchGroup[] = [new Sub325WatchGroup(\"empty\", empty), new Sub325WatchGroup(\"full\", items)]; const out = new Sub325WatchGroups(0, groups);\n  sub325WatchNested(out, {replace});\n  print(\"nested kept\");\n}}");
        let files = [
            interop::mirror("interop.generated.d.ts", SourceFile::ambient),
            SourceFile::new("nested-watch.ts", program),
        ];
        let jit = run_jit_with_native_libraries(&files, &libraries);
        let ship = run_c_aot_with_native_libraries(&files, &libraries);
        if replace == 0 {
            assert_eq!(jit.expect("nested JIT"), b"nested kept\n");
            assert_eq!(ship.expect("nested ship"), b"nested kept\n");
        } else {
            match (jit, ship) {
                (Err(RunError::Trap(jit)), Err(RunError::Trap(ship))) => {
                    assert_eq!(jit, ship);
                    assert_eq!(jit.rule, TrapKind::ForeignPointerReplaced);
                    assert_eq!(
                        jit.pos,
                        subscript_compiler::Pos::new("nested-watch.ts", 4, 3)
                    );
                    assert_eq!(jit.message, "sub325WatchNested.out.groups[1].items[6].leaf: C replaced a pointer that the script cannot receive");
                }
                (jit, ship) => panic!("nested replacement trap: {jit:?}, {ship:?}"),
            }
        }
    }
}

// compiler.md §325 acceptance 3: a const target receives no comparison code.
fn a_const_target_has_no_read_back_compare() {
    let files = [interop::mirror("interop.generated.d.ts", SourceFile::ambient), SourceFile::new("read.ts", "export function main(): void { const out = new Sub325List(0, [1]); print(`${sub325Read(out)}`); const fixed = new Sub326Pairs(0, [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])]); print(`${sub326Read(fixed)}`); }")];
    let module = subscript_compiler::check_program(&files).expect("const read checks");
    let emitted = subscript_codegen::emit_c(&module).expect("const read emits");
    assert!(!emitted.source.contains("subscript_rt_boundary_receive"));
    assert!(!emitted.source.contains("subscript_rt_boundary_pointer"));
    assert!(!emitted.source.contains("memcmp("));
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
pub(super) fn main() -> std::process::ExitCode {
    crate::main_thread::run(&crate::main_thread_tests![
        parallel: [
        ],
        main_thread: [
            c_bytes_become_a_valid_string,
            fixed_struct_array_members_use_each_element_pass,
            fixed_struct_array_byte_elements_compare_as_embedded_members,
            fixed_struct_arrays_keep_nested_receives_and_compares,
            nested_scratch_elements_keep_nonzero_watch_indices,
            finite_array_watches_and_recursive_headers_match_both_tiers,
            an_external_embedded_userdata_slot_is_compared,
            c_replaced_members_are_received,
            a_count_shrink_keeps_the_pre_callback_array,
            c_pointer_replacements_report_the_same_trap_in_both_tiers,
            a_const_target_has_no_read_back_compare,
            fixed_array_parameters_share_places_and_materialize_temporaries,
            fixed_array_locals_keep_storage_across_await_and_yield,
            fixed_array_parameter_checks_an_element_place,
            foreign_call_keeps_borrowed_storage_corpus,
            foreign_array_growth_and_nested_borrows_trap_at_the_push,
            foreign_member_array_growth_traps_at_the_push,
            foreign_array_element_pointer_growth_traps_at_the_push,
            foreign_field_pointer_keeps_its_owner_across_a_later_await,
            handle_create_retain_release,
            string_label_round_trips_through_the_callback,
            buffer_view_sum_through_the_callback,
            chain_slot_address_of_via_constructor,
            chain_slot_address_of_via_assignment,
            chain_extension_payload_is_read_through_its_embedded_header,
            deferred_completion_callback_fires_on_pump,
            callback_userdata_rooting_corpus_survives_collect_on_both_tiers,
            callback_userdata_fire_check_traps_identically_on_both_tiers,
            all_patterns_composed,
            foreign_call_arguments_run_left_to_right,
            foreign_call_operands_survive_a_later_suspension,
            foreign_call_without_suspension_uses_call_time_view,
            foreign_call_rule_7a_plain_twin_uses_call_time_view,
            foreign_call_rule_7a_later_suspension_uses_call_time_view,
            boundary_struct_field_initializers_run_left_to_right,
            boundary_struct_new_arguments_survive_suspension,
            two_deferred_one_shots_each_end_one_time,
            a_start_over_the_table_capacity_is_refused_and_released_at_once,
            a_second_subscribe_is_refused_and_released_at_once,
            a_notification_queued_during_the_drain_fires_at_the_next_pump,
        ],
    ])
}
