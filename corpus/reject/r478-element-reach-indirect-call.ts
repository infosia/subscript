// corpus: reject/r478-element-reach-indirect-call
// purpose: A method reaches a move through an indirect call of a function value with the same signature.
// exercises: value-struct, dynamic-array, method-receiver, function-value, reach
// questions: compiler.md §221.1 rule 4
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  a: i32 = 1;

  step(hook: (n: i32) => void): void {
    hook(1);
  }
}

const all: V[] = [new V()];

function grow(n: i32): void {
  all.push(new V());
}

export function main(): void {
  const hook: (n: i32) => void = grow;
  all[0].step(hook);
}
