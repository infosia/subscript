// corpus: accept/a482-an-indirect-call-runs-the-values-that-reach-it
// purpose: Indirect calls follow their arguments and const values.
// observable: Four calls print before unrelated globals initialize.
// exercises: callbacks, parameters, const, array-map
// questions: compiler.md §332 rules 1 to 4
// tsc: accepts; js-comparable: yes
function run(f: () => void): void { f(); }
function outer(g: () => void): void { run(g); }
function mapped(xs: i32[], f: (x: i32) => i32): i32[] { return xs.map(f); }
const later = (): void => { print(`${LATER}`); };
const scale = (x: i32): i32 => x * FACTOR;
const early = (): void => { print("early"); };
run((): void => { print("first"); });
const xs: i32[] = [1, 2];
const ys: i32[] = mapped(xs, (x: i32): i32 => x + 1);
print(`${ys.join(",")}`);
outer((): void => { print("two"); });
early();
const LATER: i32 = 5;
const FACTOR: i32 = 3;
export function main(): void {}
