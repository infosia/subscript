// corpus: accept/a27-interop-string-view
// purpose: Round-trips a length-carrying string label through setLabel and the callback.
// exercises: interop-string-view, string-boundary, callback, foreign-call
// questions: Q13, Q16

// Q16: self-created handle via subDeviceCreate(null). A `string` argument
// lowers to a length-carrying view (pointer + byte length, no NUL); the
// callee stores it and setLogger replays it to the callback, whose
// message length is the label's byte length. The printed number is that
// byte length.

class LogSink {
  count: i32;
  constructor() {
    this.count = 0;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);

  // Store the label first (a string-view boundary argument).
  subDeviceSetLabel(device, "device-label");

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
  // setLogger fires the callback once with the stored label as the
  // message; "device-label" is 12 bytes.
  subDeviceSetLogger(device, info);

  print(`${sink.count}`);
}
