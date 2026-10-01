// corpus: accept/a306-generic-enum-index
// purpose: Accepts integer casts of enum-constrained type parameters.
// exercises: generic-function, generic-class, enum, as-cast, index
// questions: compiler.md §143
// tsc: accepts; js-comparable: yes
enum E { A = 0, B = 1 }
function ix<T extends E>(names: string[], x: T): string { return names[x as i32]; }
function small<T extends E>(names: string[], x: T): string { return names[(x as u8) as i32]; }
class Tagged<K extends E> {
  k: K;
  constructor(k: K) { this.k = k; }
  read(names: string[]): string { return names[this.k as i32]; }
}
export function main(): void {
  const names: string[] = ["a", "b"];
  print(ix<E>(names, E.B));
  print(small<E>(names, E.B));
  const tagged = new Tagged<E>(E.B);
  print(tagged.read(names));
}
