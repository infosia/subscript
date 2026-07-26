// corpus: trap/t21-narrow-class-mismatch
// purpose: Traps when boundary-opaque userdata is narrowed to the wrong class.
// exercises: interop-callback, object, as-narrowing, class-mismatch
// questions: Q13, Q16
// expected-trap: class-mismatch at the `as Expected` expression

class Actual {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

class Expected {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const actual: Actual = new Actual(7);
  const info: SubCallbackInfo = new SubCallbackInfo(
    (message, userdata1, userdata2) => {
      print("before class narrowing");
      const ignored: Expected = userdata1 as Expected;
      print("after class narrowing");
    },
    actual,
    null,
  );
  subDeviceSetLabel(device, "mismatch");
  subDeviceSetLogger(device, info);
  print("after callback");
}
