// corpus: accept/a419-operand-takes-context
// purpose: Operands of ||, the right operand of &&, the left operand of ?? or its optional call, and an await operand take the context.
// observable: each form prints the value of the operand that decides it.
// exercises: generic-inference, contextual-type, logical-or, logical-and, nullish-coalescing, optional-call, await
// questions: compiler.md §232, §212, §215
// tsc: accepts; js-comparable: yes
class P {
  v: i32 = 1;
}

class Buf<T> {
  items: T[] = [];
  tag: i32 = 0;
}

function pick<T>(text: string): T {
  return JSON.parse<T>(text);
}

function empty<T>(): Buf<T> {
  return new Buf<T>();
}

class K {
  tag: i32;
  constructor(tag: i32) {
    this.tag = tag;
  }

  make<T>(): Buf<T> {
    const b: Buf<T> = new Buf<T>();
    b.tag = this.tag;
    return b;
  }
}

async function af<T>(tag: i32): Promise<Buf<T>> {
  const b: Buf<T> = new Buf<T>();
  b.tag = tag;
  return b;
}

function orRight(c: boolean, text: string): boolean {
  return c || pick(text);
}

function orLeft(text: string, c: boolean): boolean {
  return pick(text) || c;
}

function andRight(c: boolean, text: string): boolean {
  return c && pick(text);
}

function optional(k: K | null): Buf<P> {
  return k?.make() ?? empty<P>();
}

function optionalBoth(k: K | null): Buf<P> {
  return k?.make() ?? empty();
}

function parsed(text: string): Buf<P> {
  return pick(text) ?? empty<P>();
}

async function awaited(c: boolean): Promise<Buf<P>> {
  return await (c ? af(2) : af(3));
}

export async function main(): Promise<void> {
  print(`or:${orRight(true, "false")}:${orRight(false, "true")}:${orRight(false, "false")}`);
  print(`orLeft:${orLeft("true", false)}:${orLeft("false", true)}:${orLeft("false", false)}`);
  print(`and:${andRight(false, "true")}:${andRight(true, "true")}:${andRight(true, "false")}`);
  print(`optional:${optional(new K(4)).tag}:${optional(null).tag}`);
  print(`optionalBoth:${optionalBoth(new K(6)).tag}:${optionalBoth(null).tag}`);
  print(`parsed:${parsed('{"items":[],"tag":5}').tag}:${parsed("null").tag}`);
  const first: Buf<P> = await awaited(true);
  const second: Buf<P> = await awaited(false);
  print(`await:${first.tag}:${second.tag}`);
}
