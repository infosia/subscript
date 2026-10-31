// corpus: reject/r474-element-reach-parameter-default
// purpose: A method reaches a move through a parameter default of a function that it calls.
// exercises: value-struct, dynamic-array, method-receiver, default-parameter, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): i32 {
    return take();
  }
}

const all: V[] = [new V()];

function grow(): i32 {
  all.push(new V());
  return 1;
}

function take(n: i32 = grow()): i32 {
  return n;
}

export function main(): void {
  print(`${all[0].step()}`);
}
