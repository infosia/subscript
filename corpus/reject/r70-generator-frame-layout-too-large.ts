// corpus: reject/r70-generator-frame-layout-too-large
// purpose: Rejects two individually valid parameters whose generator frame exceeds the aggregate limit.
// exercises: generator frame layout, accumulated parameters, generator header
// questions: Q11
// expected-error: S100 at the parameter that crosses the limit

function* huge(
  left: FixedArray<u8, 1500000000>,
  right: FixedArray<u8, 1500000000>,
): Generator<void> {
  yield;
}

export function main(): void {}
