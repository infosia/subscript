// corpus: accept/a387-type-alias-generic-instance
// purpose: An alias of a generic-class instance resolves before the members of that instance, in any declaration order.
// exercises: type-alias, generic-class, declaration-order
// questions: compiler.md §202.1 rule 3
// tsc: accepts; js-comparable: yes
type Stack = Pile<i32>;
type Count = i32;
type List = Link<i32>;

class Pile<T> {
  n: Count = 0;
  bump(): Count {
    this.n = this.n + 1;
    return this.n;
  }
}

class Link<T> {
  v: T;
  next: List | null = null;
  constructor(v: T) {
    this.v = v;
  }
}

export function main(): void {
  const s: Stack = new Pile<i32>();
  s.bump();
  print(`${s.bump()} ${s.n}`);

  const a: List = new Link<i32>(1);
  a.next = new Link<i32>(2);
  const n: List | null = a.next;
  print(`${n !== null ? n.v : 0}`);
}
