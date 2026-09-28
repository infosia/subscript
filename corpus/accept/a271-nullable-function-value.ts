// corpus: accept/a271-nullable-function-value
// purpose: Nullable function values narrow, join, and retain captures.
// exercises: nullable-function, narrowing, nullish-coalescing, conditional-join
// questions: Q8, Q10
// tsc: accepts; js-comparable: yes
function dbl(x: i32): i32 { return x * 2; }
function viaIf(f: ((x: i32) => i32) | null): i32 {
  if (f !== null) { return f(3); }
  return 0;
}
function viaConditional(f: ((x: i32) => i32) | null): i32 {
  return f !== null ? f(4) : 0;
}
function viaReturn(f: ((x: i32) => i32) | null): i32 {
  if (f === null) { return 0; }
  return f(5);
}
function joins(f: ((x: i32) => i32) | null, offset: i32): void {
  const capturedOffset = offset;
  const named = f ?? dbl;
  const literal = f ?? ((x: i32): i32 => x + 100);
  const captured = f ?? ((x: i32): i32 => x + capturedOffset);
  const conditional = f !== null ? f : dbl;
  const nullable: ((x: i32) => i32) | null = f === null ? null : f;
  print(`${named(3)} ${literal(3)} ${captured(3)} ${conditional(3)}`);
  print(`${f === null} ${f !== null} ${null === f} ${null !== f} ${nullable === null}`);
}
class Holder {
  callback: ((x: i32) => i32) | null = null;
}
function field(holder: Holder): i32 {
  const callback = holder.callback;
  if (callback !== null) { return callback(6); }
  return 0;
}
export function main(): void {
  print(`${viaIf(null)} ${viaIf(dbl)} ${viaConditional(null)} ${viaConditional(dbl)} ${viaReturn(null)} ${viaReturn(dbl)}`);
  joins(null, 20);
  joins(dbl, 20);
  const offset: i32 = 7;
  const captured: ((x: i32) => i32) | null = (x: i32): i32 => x + offset;
  print(`${viaIf(captured)} ${viaConditional(captured)} ${viaReturn(captured)}`);
  const holder = new Holder();
  print(`${field(holder)}`);
  holder.callback = dbl;
  print(`${field(holder)}`);
}
