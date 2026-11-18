// corpus: accept/a484-interface-spread-then-handle-variance
// purpose: Interface methods, spread copies, and then callbacks take handle variance.
// observable: Each interface call, class call, copied function, and callback prints its result.
// exercises: interface, nullable-reference, array-spread, function-value, Promise.then
// questions: compiler.md §334
// tsc: accepts; js-comparable: yes
class C { v: i32 = 7; }
interface Result { m(): C | null; }
class K implements Result { m(): C { return new C(); } }
interface Text { m(): string | null; }
class T implements Text { m(): string { return "text"; } }
interface Parameter { m(x: C): i32; }
class P implements Parameter {
  m(x: C | null): i32 { return x === null ? 0 : x.v; }
}
function show(x: C | null): i32 { return x === null ? -1 : x.v; }
function text(x: string | null): string { return x === null ? "null" : x; }
function id(x: C): C { return x; }
function resultCall<U extends Result>(x: U): C | null { return x.m(); }
function textCall<U extends Text>(x: U): string | null { return x.m(); }
function parameterCall<U extends Parameter>(x: U, c: C): i32 { return x.m(c); }
async function mk(): Promise<C> { return new C(); }
export async function main(): Promise<void> {
  const k = new K();
  print(`result:${show(resultCall(k))}:${show(k.m())}`);
  const t = new T();
  print(`text:${text(textCall(t))}:${text(t.m())}`);
  const p = new P();
  print(`parameter:${parameterCall(p, new C())}:${p.m(new C())}`);
  const gs: ((x: C) => C)[] = [id];
  const fs: ((x: C) => C | null)[] = [...gs];
  print(`spread:${show(fs[0](new C()))}`);
  print(`then:${await mk().then((x: C | null): i32 => x === null ? -1 : x.v)}`);
}
