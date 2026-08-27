// corpus: reject/r139-bytes-of-string-element
// purpose: Rejects a FixedArray whose storage contains string handles.
// exercises: Context.bytesOf, FixedArray, string-storage-rejection
// questions: R34
// tsc: accepts
// expected-error: S100 at the bytesOf member
export function main(): void {
  const values: FixedArray<string, 2> = ["a", "b"];
  Context.bytesOf<FixedArray<string, 2>>(values);
}
