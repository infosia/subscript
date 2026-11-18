// corpus: reject/r553-a-map-calls-a-declared-callback
// purpose: A built-in callback receives escaped function values.
// exercises: map, callbacks, initializer-order
// questions: compiler.md §332 rules 3 and 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through `invoke` -> an indirect call

class Box { v: i32 = 5; }
function invoke(h: () => void, k: string): void { h(); }
const later = (): void => { print(`${LATER.v}`); };
const callbacks: Map<string, () => void> = new Map<string, () => void>();
callbacks.set("later", later);
callbacks.forEach(invoke);
const LATER: Box = new Box();
