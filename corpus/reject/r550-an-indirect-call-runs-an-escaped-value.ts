// corpus: reject/r550-an-indirect-call-runs-an-escaped-value
// purpose: An indirect call reads a later global.
// exercises: callbacks, initializer-order
// questions: compiler.md §332 rules 1 to 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through an indirect call

let hook: () => void = (): void => {};
hook = (): void => { print(`${LATER}`); };
hook();
const LATER: i32 = 5;
