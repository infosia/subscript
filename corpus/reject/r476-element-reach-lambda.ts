// corpus: reject/r476-element-reach-lambda
// purpose: A method reaches a move through a lambda in its body.
// exercises: value-struct, dynamic-array, method-receiver, arrow-function, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(): void {
    const grow = (): void => {
      all.push(new V());
    };
    grow();
  }
}

const all: V[] = [new V()];

export function main(): void {
  all[0].step();
}
