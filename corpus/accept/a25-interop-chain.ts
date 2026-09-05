// corpus: accept/a25-interop-chain
// interpreter: no — calls the synthetic native interop library
// purpose: Builds an intrusive extension chain and observes its depth through the callback.
// exercises: interop-chain, chain-header, struct-pointer-slot, callback, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// Q16: this entry creates its own handle through the synthetic
// `subDeviceCreate`, passing a chain it builds; the handle is not
// host-injected. The chain depth is the only value the callback surfaces
// (the command view sums to zero), so the printed number is the depth.

class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  // Intrusive extension chain: three chain-header nodes tagged with the
  // three kinds, each `next` slot pointing at the following node's
  // storage (the Struct | null boundary form).
  const tail: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_B, null);
  const mid: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_EXT_A, tail);
  const head: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, mid);

  // create walks the chain through `next` and records its depth (3 nodes).
  const device: SubDevice = subDeviceCreate(head);

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
  // No label was set, so setLogger fires the callback with a zero-length
  // message; the sink stays at 0.
  subDeviceSetLogger(device, info);

  // The command view sums to 0, so submit fires the callback with a
  // message whose length is (sum + chain depth) = the chain depth alone.
  const commands: u32[] = [0];
  subDeviceSubmit(device, commands);

  print(`${sink.count}`);
}
