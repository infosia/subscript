// corpus: accept/a390-trivial-erasures
// purpose: Global isNaN and isFinite on a number, an array in a template, satisfies, and debugger.
// exercises: number-predicates, template-literal, satisfies, debugger
// questions: compiler.md §204
// tsc: accepts; js-comparable: yes
enum Tone {
  Low,
  High,
}

class P {
  x: i32 = 1;
}

let reads: i32 = 0;

function values(): i32[] {
  reads += 1;
  return [4, 5];
}

export function main(): void {
  const z: f64 = 0;
  const nan = z / z;
  print(`${isNaN(nan)} ${isNaN(1.5)} ${isFinite(1 / z)} ${isFinite(2)} ${isFinite(nan)}`);

  const xs: i32[] = [1, 2, 3];
  const ss: string[] = ["a", "b"];
  const fs: f64[] = [0.5, nan, -0];
  const ts: Tone[] = [Tone.Low, Tone.High];
  const bs: boolean[] = [true, false];
  const empty: i32[] = [];
  print(`${xs} [${ss}] ${fs} ${ts} ${bs} <${empty}>`);
  print(`${values()} ${reads}`);

  const p = new P() satisfies P;
  const n = 3 satisfies i64;
  print(`${p.x} ${n}`);

  debugger;
  print("end");
}
