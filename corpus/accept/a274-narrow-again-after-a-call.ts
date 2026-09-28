// corpus: accept/a274-narrow-again-after-a-call
// purpose: Shared values require a new null check or a local copy after a call.
// exercises: shared-location-narrowing, local-copy
// questions: compiler.md §124, C17
// tsc: accepts; js-comparable: yes
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function clear(h: Holder): void { h.c = null; }
function keep(h: Holder): void { }
function narrowAgain(h: Holder): void {
  if (h.c !== null) {
    keep(h);
    if (h.c !== null) { print(`again=${h.c.v}`); }
  }
  if (h.c !== null) {
    clear(h);
    if (h.c !== null) { print(`unexpected=${h.c.v}`); }
    else { print("cleared"); }
  }
}
function copyFirst(h: Holder): void {
  const c = h.c;
  if (c !== null) {
    clear(h);
    print(`copy=${c.v}`);
  }
}
export function main(): void {
  narrowAgain(new Holder());
  copyFirst(new Holder());
}
