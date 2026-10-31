// corpus: reject/r475-element-reach-using-dispose
// purpose: A method reaches a move through the dispose hook of a using declaration.
// exercises: value-struct, dynamic-array, method-receiver, using, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): void {
    using held: Held = new Held();
    this.a = 2;
  }
}

const all: V[] = [new V()];

class Held {
  [Symbol.dispose](): void {
    all.push(new V());
  }
}

export function main(): void {
  all[0].step();
}
