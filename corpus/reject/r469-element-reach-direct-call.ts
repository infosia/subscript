// corpus: reject/r469-element-reach-direct-call
// purpose: A method reaches a move through a direct call of a function.
// exercises: value-struct, dynamic-array, method-receiver, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): i32 {
    return grow();
  }
}

const all: V[] = [new V()];

function grow(): i32 {
  all.push(new V());
  return 1;
}

export function main(): void {
  print(`${all[0].step()}`);
}
