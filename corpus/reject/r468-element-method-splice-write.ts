// corpus: reject/r468-element-method-splice-write
// purpose: After a splice of its own array, a write through this reaches the element that took the removed position.
// exercises: value-struct, dynamic-array, method-receiver, Array.splice
// questions: compiler.md §221.1 rule 3
// tsc: accepts
// expected-error: S100, can move a dynamic array of `V`
@ValueType
class V {
  hp: i32;

  constructor(hp: i32) {
    this.hp = hp;
  }

  remove(list: V[], i: i32): void {
    list.splice(i, 1);
    this.hp = 0;
  }
}

export function main(): void {
  const list: V[] = [new V(1), new V(2), new V(3)];
  list[1].remove(list, 1);
  print(`${list.length} ${list[1].hp}`);
}
