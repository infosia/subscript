// corpus: accept/a30-interop-compose
// purpose: Composes all five C interop patterns in one program.
// exercises: interop-chain, interop-array-pair, interop-string-view, interop-callback, interop-handle, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// Q16: self-created handle via subDeviceCreate(chain), passing a chain the
// entry builds. All five plan-§4 patterns appear together: an intrusive
// extension chain (1), a (pointer,count) command view (2), a
// length-carrying string label (3), a callback plus userdata (4), and the
// opaque handle's create/retain/release lifecycle (5). The callback
// accumulates every effect into one userdata sink; the printed number is
// 12 (label) + 6 (command sum) + 2 (chain depth) = 20.

class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  // Pattern 1: a two-node intrusive extension chain (depth 2).
  const tail: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, null);
  const head: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, tail);

  // Pattern 5: create the opaque handle from the chain.
  const device: SubDevice = subDeviceCreate(head);

  // Pattern 4: a callback with a userdata sink, narrowed via `as`.
  const sink: LogSink = new LogSink();
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        const s = userdata1 as LogSink;
        s.count = s.count + message.length;
      }
    },
    sink,
    null,
  );

  // Pattern 3: a length-carrying string label ("device-label", 12 bytes).
  subDeviceSetLabel(device, "device-label");
  // setLogger fires the callback once with the stored label.
  subDeviceSetLogger(device, info);

  // Pattern 2: a (pointer,count) command view; submit sums it (6) and
  // fires the callback with a message of length (sum + chain depth) = 8.
  const commands: u32[] = [1, 2, 3];
  subDeviceSubmit(device, commands);

  // Pattern 5 (cont.): balanced retain/release lifecycle.
  subDeviceRetain(device);
  subDeviceRelease(device);
  subDeviceRelease(device);

  print(`${sink.count}`);
}
