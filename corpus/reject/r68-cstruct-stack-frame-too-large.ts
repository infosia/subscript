// corpus: reject/r68-cstruct-stack-frame-too-large
// purpose: Rejects a value-class local whose layout fits the aggregate limit and crosses the stack-frame limit.
// exercises: CStruct local storage, accumulated stack-frame layout
// questions: Q2, Q3
// tsc: accepts
// expected-error: S100 at the local declaration
@CStruct
class Accumulated {
  prefix: FixedArray<u8, 2147483640>;

  constructor(prefix: FixedArray<u8, 2147483640>) {
    this.prefix = prefix;
  }
}

function build(source: FixedArray<u8, 2147483640>): void {
  const a: Accumulated = new Accumulated(source);
  print(`${a.prefix.length}`);
}

export function main(): void {}
