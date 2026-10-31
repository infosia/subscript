// corpus: reject/r477-element-reach-generator-next
// purpose: A method reaches a move through a generator body at next.
// exercises: value-struct, dynamic-array, method-receiver, generator, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
const all: V[] = [];

function* grower(): Generator<i32> {
  all.push(new V());
  yield 1;
}

@ValueType
class V {
  a: i32 = 1;

  step(values: Generator<i32>): void {
    values.next();
  }
}

export function main(): void {
  all.push(new V());
  all[0].step(grower());
}
