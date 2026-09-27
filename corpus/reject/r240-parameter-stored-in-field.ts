// corpus: reject/r240-parameter-stored-in-field
// purpose: Reject a function parameter escape (compiler.md §118).
// exercises: function-parameter, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at caller argument
class Holder {
  callback: (() => i32) | null = null;
}
function store(holder: Holder, cb: () => i32): void {
  holder.callback = cb;
}
export function main(): void {
  const holder = new Holder();
  const value: i32 = 5;
  store(holder, (): i32 => value);
}
