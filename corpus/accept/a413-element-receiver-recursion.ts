// corpus: accept/a413-element-receiver-recursion
// purpose: A recursion through an element writes one object at each level, because this and the element name one object.
// exercises: value-struct, dynamic-array, method-receiver, recursion
// questions: compiler.md §221.1 rule 8
// tsc: accepts
// js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class Counter {
  a: i32 = 2;

  recur(d: i32): void {
    if (d > 0) {
      all[0].recur(d - 1);
      this.a += 1;
    }
  }
}

const all: Counter[] = [new Counter()];

export function main(): void {
  all[0].recur(3);
  print(`${all[0].a}`);
}
