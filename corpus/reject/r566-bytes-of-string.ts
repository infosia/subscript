// corpus: reject/r566-bytes-of-string
// purpose: Rejects storage-byte access for a string, and names the accepted types.
// exercises: Context.bytesOf, string-rejection, scalar-bytes
// questions: R34, compiler.md §360
// tsc: accepts
// expected-error: S100 at the bytesOf member; it is not a @ValueType value class, a FixedArray, a sized numeric, a boolean, or an enum
export function main(): void {
  Context.bytesOf<string>("x");
}
