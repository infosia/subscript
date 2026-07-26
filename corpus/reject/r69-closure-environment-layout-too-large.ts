// corpus: reject/r69-closure-environment-layout-too-large
// purpose: Rejects two individually valid captures whose closure environment exceeds the aggregate limit.
// exercises: closure environment layout, accumulated captures
// expected-error: S100 at the lambda

function probe(
  leftInput: FixedArray<u8, 1600000000>,
  rightInput: FixedArray<u8, 1600000000>,
): void {
  const left: FixedArray<u8, 1600000000> = leftInput;
  const right: FixedArray<u8, 1600000000> = rightInput;
  const size: () => i32 = (): i32 => left.length + right.length;
  print(`${size()}`);
}

export function main(): void {}
