// corpus: accept/a398-inference-promise-array
// purpose: Generic inference reads `Promise<T>` and `Array<T>`, and `await` gives its operand call a contextual type.
// exercises: generic-inference, contextual-type, async-call, generic-method, using
// questions: compiler.md §215
// tsc: accepts; js-comparable: yes
class Particle {
  x: i32 = 1;
}

class Buf<T> {
  size: i32;
  items: T[] = [];
  constructor(size: i32) {
    this.size = size;
  }
  [Symbol.dispose](): void {
    print(`dispose ${this.size}`);
  }
}

async function af<T>(size: i32): Promise<Buf<T>> {
  return new Buf<T>(size);
}

async function particle(): Promise<Particle> {
  return new Particle();
}

async function fr<T>(p: Promise<T>): Promise<T> {
  return await p;
}

async function boxed<T>(value: T): Promise<Buf<T>> {
  const b = new Buf<T>(1);
  b.items.push(value);
  return b;
}

async function forward<U>(size: i32): Promise<Buf<U>> {
  return await af(size);
}

function mk<T>(): Array<T> {
  return [];
}

function first<T>(items: Array<T>): T {
  return items[0];
}

class Factory {
  async make<T>(size: i32): Promise<Buf<T>> {
    return new Buf<T>(size);
  }
}

export async function main(): Promise<void> {
  const local: Buf<Particle> = await af(1);
  print(`await ${local.size}`);

  const held: Promise<Buf<Particle>> = af(2);
  print(`held ${(await held).size}`);

  const source: Promise<Particle> = particle();
  const passed = await fr(source);
  print(`argument ${passed.x}`);

  const empty: Particle[] = mk();
  empty.push(new Particle());
  print(`array ${first(empty).x}`);

  const maybe: Buf<Particle> | null = await af(3);
  print(`nullable ${maybe !== null ? maybe.size : -1}`);

  const f = new Factory();
  const method: Buf<Particle> = await f.make(4);
  print(`method ${method.size}`);

  const nested: Buf<Buf<Particle>> = await boxed(await af(5));
  print(`nested ${nested.items[0].size}`);

  print(`return ${(await forward<Particle>(6)).size}`);

  using scoped: Buf<Particle> = await af(7);
  scoped.items.push(new Particle());
  print(`using ${scoped.size} ${scoped.items[0].x}`);
}
