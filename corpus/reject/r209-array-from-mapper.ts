// corpus: reject/r209-array-from-mapper
// purpose: Rejects the Array.from mapper overload.
// exercises: array-from, callback-argument
// questions: Q22, compiler section 105
// tsc: accepts
// expected-error: S014 naming the callback typing and traversal work
export function main(): void {
  const xs: i32[] = [1, 2];
  const doubled = Array.from(xs, (value: i32): i32 => value * 2);
  print(`${doubled.length}`);
}
