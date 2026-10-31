// corpus: reject/r471-element-reach-constructor
// purpose: A method reaches a move through a constructor.
// exercises: value-struct, dynamic-array, method-receiver, constructor, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): void {
    const made: Maker = new Maker();
    print(`${made.n}`);
  }
}

const all: V[] = [new V()];

class Maker {
  n: i32 = 0;

  constructor() {
    all.shift();
    this.n = 1;
  }
}

export function main(): void {
  all[0].step();
}
