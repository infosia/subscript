// corpus: accept/a28-interop-callback
// purpose: Registers a callback with userdata, fires it twice, and narrows the userdata via `as`.
// exercises: interop-callback, userdata-narrowing, as-narrowing, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// Q16: self-created handle via subDeviceCreate(null). The callback is a
// non-capturing function (C5) plus two userdata slots (§14.4; the boundary
// `object | null` form); inside, the first userdata is narrowed to its
// concrete class through a checked `as` (C3). The sink accumulates across
// two separate callback firings (setLogger and submit), proving the
// userdata outlives the registration.

class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);

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

  // First firing: setLogger replays the stored label (2 bytes).
  subDeviceSetLabel(device, "cb");
  subDeviceSetLogger(device, info);

  // Second firing: submit sums the command view (1 + 2 + 3 = 6) and fires
  // the callback with a message of that length (chain depth 0).
  const commands: u32[] = [1, 2, 3];
  subDeviceSubmit(device, commands);

  // 2 (label) + 6 (sum) accumulated in the same userdata sink.
  print(`${sink.count}`);
}
