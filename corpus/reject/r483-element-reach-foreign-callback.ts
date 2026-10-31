// corpus: reject/r483-element-reach-foreign-callback
// purpose: A foreign call reaches a function value that crossed the boundary, so a stored callback that moves the array rejects the call on an element.
// exercises: value-struct, dynamic-array, method-receiver, interop-callback, foreign-call, reach
// questions: compiler.md §221.1 rule 10
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;
  b: i32 = 2;

  update(d: SubDevice): i32 {
    subDevicePump(d);
    this.a = 7;
    return this.b;
  }
}

const all: V[] = [new V()];

export function main(): void {
  const device: SubDevice = subDeviceCreate(null);
  const info: SubCompletionInfo = new SubCompletionInfo(
    (message, a, b) => {
      for (let i: i32 = 0; i < 5000; i = i + 1) {
        all.push(new V());
      }
    },
    null,
  );
  subDeviceOnComplete(device, info);
  subDeviceSubmit(device, [1]);

  print(`${all[0].update(device)}`);
  print(`${all[0].a}`);
}
