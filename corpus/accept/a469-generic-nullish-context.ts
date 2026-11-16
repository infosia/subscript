// corpus: accept/a469-generic-nullish-context
// purpose: A missing generic fallback candidate and an empty array take the nullish context.
// exercises: generic-inference, nullish, contextual-type
// questions: compiler.md §319
// tsc: accepts; js-comparable: yes
class Buf<T> { n: i32 = 0; }
function empty<T>(): Buf<T> { return new Buf<T>(); }
function f<U>(a: Buf<U> | null): Buf<U> { return a ?? empty(); }
function g<U>(a: U[] | null): U[] { return a ?? []; }
export function main(): void {
  const b = new Buf<i32>();
  b.n = 7;
  print(`${f<i32>(null).n}`);
  print(`${f<i32>(b).n}`);
  print(`${g<i32>(null).length}`);
  print(`${g<i32>([9]).length}`);
}
