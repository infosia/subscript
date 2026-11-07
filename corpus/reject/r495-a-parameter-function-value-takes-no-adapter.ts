// corpus: reject/r495-a-parameter-function-value-takes-no-adapter
// purpose: A parameter of a function type takes no adapter; only a const local or a module const does.
// exercises: function-value, capture, parameter
// questions: compiler.md §246.1 rules 4 and 5
// tsc: accepts
// expected-error: S100 at line 15, function value `fv` needs an adapter for this place

type CB2 = (x: i32, y: i32) => void;

function run(cb: CB2): void {
  cb(1, 2);
}

function g(fv: (x: i32) => void): void {
  run(fv);
}

export function main(): void {
  g((x: i32): void => print(`${x}`));
}
