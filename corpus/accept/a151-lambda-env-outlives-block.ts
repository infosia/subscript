// corpus: accept/a151-lambda-env-outlives-block
// purpose: Keeps a loop-body lambda environment live after its source block exits.
// exercises: closures, lambda-environment, loop-body-scope, live-range-storage
// questions: §68
// tsc: accepts
export function main(): void {
  let f = (): i32 => 0;
  for (let i: i32 = 0; i < 3; i = i + 1) {
    const k: i32 = i * 10;
    f = (): i32 => k + 2;
  }
  print(`v=${f()}`);
}
