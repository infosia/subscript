// corpus: accept/a382-logical-assignment-value-projection
// purpose: Logical assignment through a value-class element or a FixedArray field writes the element that the program names.
// exercises: logical-assignment, value-class, FixedArray, evaluation-order
// questions: compiler.md §199.1 rule 2, compiler.md §199.3
// tsc: accepts; js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class P {
  on: boolean;
  n: i32;
  constructor(on: boolean) {
    this.on = on;
    this.n = 0;
  }
}

class H {
  p: P = new P(false);
  ps: P[] = [new P(false)];
  flags: FixedArray<boolean, 2> = [false, false];
}

const G = new H();

function getH(): H {
  return G;
}

function at(i: i32): i32 {
  return i;
}

export function main(): void {
  const grid: P[] = [new P(false), new P(false), new P(false), new P(false)];
  const w: i32 = 2;
  for (let y: i32 = 0; y < 2; y++) {
    for (let x: i32 = 0; x < 2; x++) {
      grid[y * w + x].on ||= x === 1;
    }
  }
  print(`grid ${grid[0].on} ${grid[1].on} ${grid[2].on} ${grid[3].on}`);
  const qs: P[] = [new P(false), new P(false), new P(true)];
  qs[0].on ||= true;
  qs[2].on &&= false;
  const v = (qs[1].on ||= true);
  print(`elements ${qs[0].on} ${qs[1].on} ${qs[2].on} ${v}`);
  getH().p.on ||= true;
  G.ps[0].on ||= true;
  getH().flags[1] ||= true;
  G.flags[at(0)] ||= true;
  print(`fields ${G.p.on} ${G.ps[0].on} ${G.flags[0]} ${G.flags[1]}`);
}
