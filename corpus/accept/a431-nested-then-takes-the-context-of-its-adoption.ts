// corpus: accept/a431-nested-then-takes-the-context-of-its-adoption
// purpose: A then callback whose body is itself a then call gives that call the context Promise<C>, at each level.
// observable: each level prints its argument, and the result prints the literal of the inner callback.
// exercises: Promise.then, Promise.catch, string-literal-union, async-function, await, exception
// questions: compiler.md §245, §233
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";

async function v(n: i32): Promise<i32> {
  print(`v${n}`);
  return n + 1;
}

async function bad(n: i32): Promise<i32> {
  print(`bad${n}`);
  throw new Error("bad");
}

async function badMode(): Promise<Mode> {
  print("badMode");
  throw new Error("badMode");
}

async function h2(): Promise<Mode> {
  return v(1).then((x: i32) => v(x).then((y: i32) => "a"));
}

async function h3(): Promise<Mode> {
  return v(10).then((x: i32) => v(x).then((y: i32) => v(y).then((z: i32) => "b")));
}

export async function main(): Promise<void> {
  const m2: Mode = await h2();
  print(`h2=${m2}`);
  const p: Promise<Mode> = v(20).then((x: i32) => v(x).then((y: i32) => "b"));
  const m: Mode = await p;
  print(`p=${m}`);
  const m3: Mode = await h3();
  print(`h3=${m3}`);
  const r: Promise<Mode> = bad(1).then(
    (x: i32): Mode => "a",
    (e: Error) => v(30).then((y: i32) => "b"),
  );
  const mr: Mode = await r;
  print(`r=${mr}`);
  const q: Promise<Mode> = badMode();
  const c: Promise<Mode> = q.catch((e: Error) => v(40).then((y: i32) => "a"));
  const mc: Mode = await c;
  print(`c=${mc}`);
}
