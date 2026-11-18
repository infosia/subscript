// corpus: reject/r548-an-indirect-call-runs-its-argument
// purpose: An indirect call reads a later global.
// exercises: callbacks, initializer-order
// questions: compiler.md §332 rules 1 to 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through `run` -> an indirect call

function run(f: () => void): void { f(); }
const later = (): void => { print(`${LATER}`); };
run(later);
const LATER: i32 = 5;
