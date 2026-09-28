// corpus: reject/r253-narrowing-alias-store
// purpose: Rejects a shared read after its narrowing ends.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S100 at line 13
function value(x: i32): i32 { return x; }
class Holder { cb: ((x: i32) => i32) | null = value; }
function read(h: Holder): void {
  const alias = h;
  if (h.cb !== null) {
    alias.cb = null;
    h.cb(3);
  }
}
