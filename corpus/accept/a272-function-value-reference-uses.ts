// corpus: accept/a272-function-value-reference-uses
// purpose: Function references support Map.get and field calls.
// exercises: map, function-field, narrowing, receiver-order
// questions: Q8, Q10, Q24
// tsc: accepts; js-comparable: no Q24: Map.getOr is a language intrinsic; Map.get returns null on a miss.
function dbl(x: i32): i32 { return x * 2; }
function fallback(x: i32): i32 { return x + 100; }
class Holder {
  cb: (x: i32) => i32 = dbl;
  optional: ((x: i32) => i32) | null = null;
  call(x: i32): i32 { return this.cb(x); }
}
let order: i32 = 0;
function receiver(h: Holder): Holder { order = order * 10 + 1; return h; }
function argument(): i32 { order = order * 10 + 2; return 7; }
function narrowed(h: Holder): i32 {
  if (h.optional !== null) { return h.optional(6); }
  return -1;
}
export function main(): void {
  const functions = new Map<string, (x: i32) => i32>();
  functions.set("double", dbl);
  const hit = functions.get("double") ?? fallback;
  const miss = functions.get("missing");
  print(`${hit(3)} ${miss === null}`);
  const present = functions.getOr("double", fallback);
  const absent = functions.getOr("missing", fallback);
  print(`${present(4)} ${absent(4)}`);
  const h = new Holder();
  print(`${h.cb(4)} ${h.call(5)} ${narrowed(h)}`);
  h.optional = dbl;
  print(`${narrowed(h)} ${h.optional !== null ? h.optional(8) : -1}`);
  print(`${receiver(h).cb(argument())} ${order}`);
}
