// corpus: accept/a238-sandbox-clean
// profile: sandbox
// purpose: Runs a loop, a call, Context.bytesOf, and Context.collect under the sandbox profile.
// exercises: sandbox-profile, sandbox-enter, sandbox-poll, Context.bytesOf, Context.collect
// questions: R34, Q7
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct({ align: 4 })
class Sample {
  x: i32 = 0;
  y: i32 = 0;

  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

function contribute(index: i32): i32 {
  return index % 7;
}

export function main(): void {
  let checksum: i32 = 0;
  for (let index: i32 = 0; index < 1000; index = index + 1) {
    checksum = checksum + contribute(index);
  }
  const sample: Sample = new Sample(checksum, 2);
  const bytes: u8[] = Context.bytesOf<Sample>(sample);
  Context.collect();
  print(`${checksum},${bytes.length}`);
}
