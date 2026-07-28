// example: e06-arrays-and-closures
// teaches: Contrast fixed and growable arrays, use checked indexing, and transform values with map, filter, and reduce.
// differs-from-typescript: C5 permits const capture only while the callback remains non-escaping.
// see: corpus/accept/a06-fixed-array.ts, corpus/accept/a07-slice-pair.ts, corpus/accept/a13-closures-noncapture.ts, corpus/accept/a14-closures-capture.ts, corpus/accept/a44-array.ts, corpus/accept/a45-array-fn.ts, corpus/trap/t02-statements-after-fault.ts, corpus/reject/r10-escaping-capture.ts, collisions.md C5, collisions.md Q3-Q4, compiler.md §7

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // Q3: FixedArray stores exactly four i32 elements in place.
  const fixed: FixedArray<i32, 4> = [2, 4, 6, 8];
  // Q4: T[] owns Context-allocated growable storage.
  const growable: i32[] = [1, 2, 3, 4];
  growable.push(5);

  // Q3/Q4 and compiler.md §7: indexing either array is bounds checked.
  // An out-of-range access traps with index-out-of-bounds and stops the
  // Context; corpus/trap/t02-statements-after-fault.ts pins that rule.
  print(`arrays=${fixed.length},${fixed[2]},${growable.length},${growable[4]}`);

  const scale: i32 = 3;
  const minimum: i32 = 8;
  // C5: map and filter finish before returning, so their callbacks may
  // capture these const values by value.
  const mapped: i32[] =
    growable.map((value: i32): i32 => value * scale);
  const filtered: i32[] =
    mapped.filter((value: i32): boolean => value >= minimum);
  const total: i32 = filtered.reduce(
    (sum: i32, value: i32): i32 => sum + value,
    0,
  );

  // Rejected alternative: returning a callback that captures scale is S009,
  // "capturing lambdas may not escape their defining function";
  // corpus/reject/r10-escaping-capture.ts pins it.
  print(`mapped=${mapped.join(",")}`);
  print(`filtered=${filtered.join(",")}`);
  print(`reduced=${total}`);
}
