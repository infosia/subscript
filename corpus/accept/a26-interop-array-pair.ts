// corpus: accept/a26-interop-array-pair
// purpose: Submits a (pointer,count) command view and observes its sum through the callback.
// exercises: interop-array-pair, pointer-count-view, callback, foreign-call
// questions: Q13, Q16
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// Q16: self-created handle via subDeviceCreate(null). The chain is null
// (depth 0), so the callback's message length is the array sum alone —
// the printed number is exactly the sum of the submitted u32 view.

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
  // No label: setLogger fires the callback with a zero-length message.
  subDeviceSetLogger(device, info);

  // A u32[] argument lowers to its (pointer, count) pair; the callee sums
  // the count items and fires the callback with a message of that length
  // (chain depth is 0). 10 + 20 + 30 + 40 = 100.
  const commands: u32[] = [10, 20, 30, 40];
  subDeviceSubmit(device, commands);

  print(`${sink.count}`);
}
