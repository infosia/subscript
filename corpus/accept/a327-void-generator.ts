// corpus: accept/a327-void-generator
// purpose: Runs value-less generator suspensions on every tier.
// exercises: generator, bare-yield, next, for-of, early-return, class-method, while
// questions: C8, C21, compiler section 151
// tsc: accepts; js-comparable: yes
function* tick(n: i32): Generator<void> {
  for (let i: i32 = 0; i < n; i++) { print(`tick ${i}`); yield; }
}
function* early(stop: boolean): Generator<void> {
  if (stop) { return; }
  print("early");
  yield;
  return;
}
function* methodTicks(n: i32): Generator<void> {
  for (let i: i32 = 0; i < n; i++) { print(`method ${i}`); yield; }
}
class Clock {
  steps(n: i32): void {
    for (const step of methodTicks(n)) { print("method step"); }
  }
}
export function main(): void {
  const g: Generator<void> = tick(2);
  print(`${g.next().done}`);
  print(`${g.next().done}`);
  print(`${g.next().done}`);
  for (const step of tick(1)) { print("step"); }
  const stopped: Generator<void> = early(true);
  print(`stopped ${stopped.next().done}`);
  const running: Generator<void> = early(false);
  print(`early ${running.next().done}`);
  print(`early ${running.next().done}`);
  print(`early ${running.next().done}`);
  const clock: Clock = new Clock();
  clock.steps(2);
  const loop: Generator<void> = tick(2);
  let r = loop.next();
  while (!r.done) { print("while step"); r = loop.next(); }
  print(`while ${r.done}`);
}
