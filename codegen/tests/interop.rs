//! Foreign C-header binding: cross-tier differential tests (P5.2b).
//!
//! Each program exercises one Q13 boundary pattern through a real foreign
//! call against the committed synthetic header (`corpus/interop/`), whose
//! implementation (`interop.c`) is linked into both tiers. The assertion
//! is the one the ship=C decision rests on: **dev-JIT bytes == ship-C-AOT
//! bytes**, plus a check that the observable effect is the expected one.
//! P5.3 will add committed goldens; here the two tiers are each other's
//! oracle, and no golden is committed yet.
//!
//! interop.c's behaviour (which defines the P5.3 goldens): setLabel stores
//! a label; setLogger fires the callback once with the stored label as the
//! message; submit sums the (ptr,count) commands and fires the callback
//! with a message of length (sum + chain depth). The language callback
//! accumulates `message.length` into a userdata sink, so a program
//! surfaces every effect by printing the sink's count.

use subscript_codegen::{run_c_aot, run_jit};
use subscript_compiler::SourceFile;

/// The committed ambient mirror, ingested as a global `.d.ts` surface.
const MIRROR: &str = include_str!("../../corpus/interop/interop.generated.d.ts");

/// Runs `program` under both tiers, asserts byte-identical output, and
/// returns those bytes. A divergence is a hard failure (the cross-tier
/// equivalence the ship=C decision rests on), never papered over.
fn both_tiers(program: &str) -> Vec<u8> {
    let files = || {
        vec![
            SourceFile::ambient("interop.generated.d.ts", MIRROR),
            SourceFile::new("prog.ts", program),
        ]
    };
    let jit = run_jit(&files()).unwrap_or_else(|e| panic!("dev-JIT run failed: {e}"));
    let ship = run_c_aot(&files()).unwrap_or_else(|e| panic!("ship-C-AOT run failed: {e}"));
    assert_eq!(
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&ship),
        "dev-JIT and ship-C-AOT diverged"
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
#[test]
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
#[test]
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
#[test]
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
#[test]
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
#[test]
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

/// P6.3 async model: a completion callback is REGISTERED (subDeviceOnComplete)
/// but not fired; intervening work runs; a host-driven pump (subDevicePump)
/// fires it AFTER the registering call returned. The sink is 0 at
/// registration and after the intervening submit, then nonzero after the
/// pump — proving the deferred fire and that the userdata (and the
/// Context-held callback binding behind it) outlived the registration.
#[test]
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

/// All five patterns composed in one program (the P5.3 "compose all five"
/// shape): chain + handle + string + (ptr,count) + callback with userdata.
#[test]
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

/// §14.2 HFA guard: a foreign call returning a pure Homogeneous
/// Floating-point Aggregate by value (all-f32 / all-f64, 1–4 members) is
/// returned in SIMD registers, which the dev-JIT register-return path does
/// not model. It must fail LOUD at lowering rather than silently mis-marshal
/// against ship-C (compiler.md §12.3a / §2). Verified on the AAPCS64 gate
/// machine (a supported by-value-aggregate ABI, so the arch-gate is passed
/// and the HFA guard is what fires).
#[test]
fn hfa_float_struct_return_fails_loud() {
    const HFA_MIRROR: &str = "\
declare class SubVec2f {
  x: f32;
  y: f32;
  constructor(x: f32, y: f32);
}
declare function subVec2Make(seed: u32): SubVec2f;
";
    let files = vec![
        SourceFile::ambient("hfa.d.ts", HFA_MIRROR),
        SourceFile::new(
            "prog.ts",
            "export function main(): void {\n  const v: SubVec2f = subVec2Make(1);\n  print(`${v.x}`);\n}\n",
        ),
    ];
    let err = run_jit(&files).expect_err("HFA return must fail loud, not silently mis-marshal");
    let msg = err.to_string();
    assert!(
        msg.contains("homogeneous floating-point aggregate"),
        "expected the HFA guard to fire; got: {msg}"
    );
}
