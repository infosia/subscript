// corpus: accept/a420-then-result-takes-context
// purpose: The body of a then callback takes the value type of the contextual Promise as its context, also under await.
// observable: each form prints the value that its callback gives under the context type.
// exercises: Promise.then, contextual-type, string-literal-union, generic-inference, await
// questions: compiler.md §233, §229, §186
// tsc: accepts; js-comparable: yes
type Mode = "a" | "b";

class P {
  v: i32 = 1;
}

class Buf<T> {
  items: T[] = [];
}

function empty<T>(): Buf<T> {
  return new Buf<T>();
}

async function v(n: i32): Promise<i32> {
  if (n < 0) {
    throw new Error("negative");
  }
  return n;
}

function literal(n: i32): Promise<Mode> {
  return v(n).then((x: i32) => "a");
}

function buffer(n: i32): Promise<Buf<P>> {
  return v(n).then((x: i32) => empty());
}

function both(n: i32): Promise<Mode> {
  return v(n).then((x: i32) => "a", (e: Error) => "b");
}

async function awaited(n: i32): Promise<Mode> {
  const r: Mode = await v(n).then((x: i32) => "b");
  return r;
}

export async function main(): Promise<void> {
  const literalHandle: Promise<Mode> = literal(1);
  const m: Mode = await literalHandle;
  print(`literal:${m}`);
  const bufferHandle: Promise<Buf<P>> = buffer(2);
  const b: Buf<P> = await bufferHandle;
  print(`buffer:${b.items.length}`);
  const fulfilledHandle: Promise<Mode> = both(3);
  const fulfilled: Mode = await fulfilledHandle;
  const rejectedHandle: Promise<Mode> = both(-1);
  const rejected: Mode = await rejectedHandle;
  print(`both:${fulfilled}:${rejected}`);
  print(`awaited:${await awaited(4)}`);
}
