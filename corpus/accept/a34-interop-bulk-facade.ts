// corpus: accept/a34-interop-bulk-facade
// purpose: Passes an f32[] zero-copy through a typed facade to an untyped void*+byte-size C API.
// exercises: interop-untyped-facade, void-pointer-byte-size, zero-copy-slice, foreign-call
// questions: Q13, Q4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// The untyped API takes `const void* data, size_t size` (byte size); a
// thin typed C facade (subBulkConsumeF32) takes a typed f32 slice, computes
// `size = count * sizeof(f32)`, and forwards the borrowed run zero-copy
// (compiler.md §13.2). The subscript program hands an `f32[]` to the facade
// — bound as `T[]` — and the untyped API records the byte size and the raw
// bytes in a checksum. The documented path for `void*`+byte-size APIs.

export function main(): void {
  const data: f32[] = [1.5, 2.5, 3.5, 4.5];
  print(`${subBulkConsumeF32(data)}`);
}
