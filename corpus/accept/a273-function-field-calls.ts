// corpus: accept/a273-function-field-calls
// purpose: Script function fields support direct, optional, static, and narrowed calls.
// exercises: function-field, optional-call, narrowing, receiver-order
// questions: Q8, Q10
// tsc: accepts; js-comparable: yes
function dbl(x: i32): i32 { return x * 2; }
function show(x: i32): void { print(`${x * 3}`); }
class Holder {
  cb: (x: i32) => i32 = dbl;
  emit: (x: i32) => void = show;
  optional: ((x: i32) => i32) | null = null;
  static cb: (x: i32) => i32 = dbl;
  call(x: i32): i32 { return this.cb(x); }
}
let order: i32 = 0;
function receiver(h: Holder): Holder { order = order * 10 + 1; return h; }
function argument(): i32 { order = order * 10 + 2; return 7; }
function optionalCall(h: Holder | null): void {
  h?.cb(argument());
  h?.emit(9);
}
function narrowed(h: Holder): i32 {
  if (h.optional !== null) { return h.optional(6); }
  return -1;
}
export function main(): void {
  const h = new Holder();
  print(`${h.cb(4)} ${h.call(5)} ${Holder.cb(6)}`);
  print(`${narrowed(h)}`);
  h.optional = dbl;
  print(`${narrowed(h)}`);
  print(`${receiver(h).cb(argument())} ${order}`);
  order = 0;
  optionalCall(null);
  print(`${order}`);
  optionalCall(h);
  print(`${order}`);
}
