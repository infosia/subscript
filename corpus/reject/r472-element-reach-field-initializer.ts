// corpus: reject/r472-element-reach-field-initializer
// purpose: A method reaches a move through a field initializer of a class that it constructs.
// exercises: value-struct, dynamic-array, method-receiver, field-initializer, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): void {
    print(`${new Maker().n}`);
  }
}

const all: V[] = [new V()];

function grow(): i32 {
  all.push(new V());
  return all.length;
}

class Maker {
  n: i32 = grow();
}

export function main(): void {
  all[0].step();
}
