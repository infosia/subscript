// corpus: reject/r249-unnarrowed-nullable-field-call
// purpose: Reject a call through an unnarrowed nullable function field.
// exercises: nullable-function, field-call
// questions: Q8, Q10
// tsc: rejects TS2721
// expected-error: S100 at nullable field call
class Holder { cb: ((x: i32) => i32) | null = null; }
export function main(): void {
  const h = new Holder();
  print(`${h.cb(4)}`);
}
