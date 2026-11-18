// corpus: reject/r551-an-indirect-call-runs-an-immediate-lambda
// purpose: An indirect call reads a later global.
// exercises: callbacks, initializer-order
// questions: compiler.md §332 rules 1 to 4
// tsc: accepts
// expected-error: S100: `LATER` is accessed before its declaration, through `read` -> an indirect call

function read(): i32 { return ((): i32 => LATER)(); }
const n: i32 = read();
const LATER: i32 = 5;
