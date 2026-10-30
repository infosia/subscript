// corpus: accept/a397-contextual-result-inference
// purpose: A generic function or method call takes a type argument from its contextual type through its declared result type.
// exercises: generic-inference, contextual-type, generic-constraint, using
// questions: compiler.md §212
// tsc: accepts; js-comparable: yes
interface Creation {
  id(): i32;
}

class Device implements Creation {
  id(): i32 {
    return 7;
  }
}

class Particle {
  x: i32 = 1;
}

class Buffer<T> {
  size: i32;
  items: T[] = [];
  constructor(size: i32) {
    this.size = size;
  }
  [Symbol.dispose](): void {
    print(`dispose ${this.size}`);
  }
}

function createBuffer<T, D extends Creation>(device: D, count: i32): Buffer<T> {
  return new Buffer<T>(count + device.id());
}

function empty<T>(): Buffer<T> {
  return new Buffer<T>(0);
}

function wrap<T>(value: T): Buffer<T> {
  const b = new Buffer<T>(1);
  b.items.push(value);
  return b;
}

function sizeOf(b: Buffer<Particle>): i32 {
  return b.size;
}

function forward<U>(device: Device): Buffer<U> {
  return createBuffer(device, 2);
}

class Factory {
  make<T>(size: i32): Buffer<T> {
    return new Buffer<T>(size);
  }
  static build<T>(): Buffer<T> {
    return new Buffer<T>(40);
  }
}

class Holder {
  parts: Buffer<Particle> = empty();
}

export function main(): void {
  const device = new Device();

  const a: Buffer<Particle> = createBuffer(device, 1);
  print(`local ${a.size}`);
  print(`argument ${sizeOf(createBuffer(device, 2))}`);
  print(`return ${forward<Particle>(device).size}`);
  print(`field ${new Holder().parts.size}`);

  let later: Buffer<Particle>;
  later = createBuffer(device, 3);
  print(`assign ${later.size}`);

  const maybe: Buffer<Particle> | null = empty();
  const list: Buffer<Particle>[] = [empty(), createBuffer(device, 4)];
  print(`nullable ${maybe !== null ? maybe.size : -1} ${list[1].size}`);

  const f = new Factory();
  const m: Buffer<Particle> = f.make(30);
  const s: Buffer<Particle> = Factory.build();
  print(`method ${m.size} ${s.size}`);

  const widened: Buffer<i64> = wrap(7);
  const nested: Buffer<Buffer<Particle>> = wrap(empty());
  print(`literal ${widened.items[0]} ${nested.items[0].size}`);

  using scoped: Buffer<Particle> = createBuffer(device, 5);
  scoped.items.push(new Particle());
  print(`using ${scoped.size} ${scoped.items[0].x}`);
}
