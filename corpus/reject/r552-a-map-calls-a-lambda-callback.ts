// corpus: reject/r552-a-map-calls-a-lambda-callback
// purpose: A built-in callback receives escaped function values.
// exercises: map, callbacks, initializer-order
// questions: compiler.md §332 rules 3 and 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through a lambda -> an indirect call

const later = (): void => { print(`${LATER}`); };
const callbacks: Map<string, () => void> = new Map<string, () => void>();
callbacks.set("later", later);
callbacks.forEach((h: () => void, k: string): void => { h(); });
const LATER: i32 = 5;
