// corpus: reject/r549-an-indirect-call-runs-a-forwarded-argument
// purpose: An indirect call reads a later global.
// exercises: callbacks, initializer-order
// questions: compiler.md §332 rules 1 to 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through `outer` -> `run` -> an indirect call

function run(f: () => void): void { f(); }
function outer(g: () => void): void { run(g); }
const later = (): void => { print(`${LATER}`); };
outer(later);
const LATER: i32 = 5;
