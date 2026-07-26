// corpus: trap/t20-narrow-null
// purpose: Traps when boundary-opaque null is narrowed to a reference class.
// exercises: interop-callback, object-null, as-narrowing, null-narrowing
// questions: Q13, Q16
// expected-trap: null-narrowing at the `as Sink` expression

class Sink {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const sink: Sink = new Sink(7);
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      print("before null narrowing");
      const ignored: Sink = userdata2 as Sink;
      print("after null narrowing");
    },
    sink,
    null,
  );
  subDeviceSetLabel(device, "null");
  subDeviceSetLogger(device, info);
  print("after callback");
}
