// corpus: reject/r68-cstruct-stack-frame-too-large
// purpose: Rejects the review probe obtained by removing r65's trailing u64 field and placing the value on the stack.
// exercises: CStruct local storage, accumulated stack-frame layout
// questions: Q2, Q3
// tsc: rejects TS2564
// expected-error: S100 at the local declaration
@CStruct
class Accumulated {
  prefix: FixedArray<u8, 2147483640>;
}

export function main(): void {
  const a: Accumulated = new Accumulated();
  print(`${a.prefix.length}`);
}
