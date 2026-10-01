// corpus: reject/r292-generic-uninitialized-field
// purpose: Rejects an uninitialized field in a generic class.
// exercises: generic-body, type-parameter
// questions: compiler.md §143
// tsc: rejects TS2564
// expected-error: S100 at line 10, Rejects an uninitialized field in a generic class.


class G<T> {
  v: i32;
  w: T | null = null;
}
export function main(): void {}
