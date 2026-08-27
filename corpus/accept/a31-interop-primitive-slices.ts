// corpus: accept/a31-interop-primitive-slices
// purpose: Passes primitive-typed arrays zero-copy to typed C slice facades (f32/i32/f64/i64) and prints each checksum.
// exercises: interop-array-pair, pointer-count-view, zero-copy-slice, foreign-call, multi-primitive
// questions: Q13, Q4
// tsc: accepts
// A primitive `T[]` lowers to its (pointer, count) pair (Q4); a typed C
// descriptor `{ const T*; size_t }` binds it as `T[]` and borrows it
// zero-copy (Q13). Each `subSliceChecksum*` reads every element straight
// from the array's own backing store and returns an order-sensitive,
// i32-wrapping rolling hash, so each printed checksum depends on the
// actual element values — the read is a genuine borrow, not a copy. This
// is the generic typed-descriptor facade that hands a primitive array to
// a C API with no copy, demonstrated across four element types beyond the
// u32 case (a26). No handle is needed: the facades are self-contained.

export function main(): void {
  // Four primitive arrays, built in-language.
  const floats: f32[] = [1.5, 2.5, 3.5, 4.5];
  const ints: i32[] = [10, 20, 30];
  const doubles: f64[] = [100.9, 200.9];
  const longs: i64[] = [7, 8, 9, 10];

  // Each array is passed zero-copy to its typed C slice facade; the
  // returned i32 checksum is a function of every element value.
  print(`${subSliceChecksumF32(floats)}`);
  print(`${subSliceChecksumI32(ints)}`);
  print(`${subSliceChecksumF64(doubles)}`);
  print(`${subSliceChecksumI64(longs)}`);
}
