// Compile-time interop slice (P5.2a): a program that USES the generated
// ambient mirror (corpus/interop/interop.generated.d.ts) — it declares a
// chain, builds a (pointer,count) descriptor, registers a callback with a
// `void*` userdata slot, holds an opaque handle, and passes a string-view
// label. It is type-checked only; nothing here is executed (foreign calls
// lower at P5.2b). It must type-check under both stock `tsc` (invariant 5)
// and this project's checker.

// A concrete reference class used as the callback userdata; it crosses the
// boundary as `object | null` and returns to its concrete type through a
// checked `as` narrowing (C3).
class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  // Declare a chain: build a chain header (a boundary struct) whose `next`
  // pointer is the null tail; pass it where `SubChainHeader | null` is
  // expected.
  const chain: SubChainHeader = new SubChainHeader(
    SubChainKind.SUB_CHAIN_KIND_BASE,
    null,
  );

  // Hold an opaque handle, obtained from the host create entry.
  const device: SubDevice = subDeviceCreate(chain);

  // Build a (pointer,count) descriptor: the array-pair maps to `u32[]`.
  const commands: u32[] = [1, 2, 3];
  subDeviceSubmit(device, commands);

  // Register a callback. The callback is a non-capturing function (C5) and
  // its `userdata` parameter is the boundary `object | null` slot, typed
  // contextually from the mirror's SubLogCallback — the program never
  // spells the boundary type. Inside, the userdata is narrowed to null and
  // then to its concrete class via `as` (C3).
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata) => {
      if (userdata !== null) {
        const s = userdata as LogSink;
        s.count = s.count + message.length;
      }
    },
    sink,
    null,
  );
  subDeviceSetLogger(device, info);

  // A string-view parameter maps to `string`.
  subDeviceSetLabel(device, "device-label");

  // Lifecycle.
  subDeviceRetain(device);
  subDeviceRelease(device);
}
