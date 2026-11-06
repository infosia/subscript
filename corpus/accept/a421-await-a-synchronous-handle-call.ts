// corpus: accept/a421-await-a-synchronous-handle-call
// purpose: An await of a synchronous call that returns a Promise reads the handle of the call, as an await of a held handle does.
// observable: each form prints the value of the async call that the synchronous call starts.
// exercises: await, async-function, held-handle, class-method, static-method, generic-function, explicit-type-argument, type-alias
// questions: compiler.md §234, §167
// tsc: accepts; js-comparable: yes
type H = Promise<i32>;

class P {
  v: i32 = 7;
}

async function v(n: i32): Promise<i32> {
  return n;
}

async function ag<T>(x: T): Promise<T> {
  return x;
}

function s(): Promise<i32> {
  return v(1);
}

function sn(n: i32): Promise<i32> {
  return v(n + 10);
}

function sg<T>(x: T): Promise<T> {
  return ag<T>(x);
}

function first<T>(xs: T[]): T {
  return xs[0];
}

function aliased(n: i32): H {
  return v(n);
}

class G {
  pick<T>(x: T): T {
    return x;
  }
}

class K {
  base: i32 = 20;

  s(): Promise<i32> {
    return v(this.base);
  }

  static t(): Promise<i32> {
    return v(30);
  }
}

export async function main(): Promise<void> {
  print(`template:${await s()}`);
  const a: i32 = await new K().s();
  const b: i32 = await K.t();
  const c: i32 = await sn(4);
  print(`method:${a}:static:${b}:argument:${c}`);
  await s();
  const p: P = await sg<P>(new P());
  print(`generic:${p.v}`);
  const hs: Promise<i32>[] = [v(5)];
  const d: i32 = await first<Promise<i32>>(hs);
  const e: i32 = await new G().pick<Promise<i32>>(v(8));
  const f: i32 = await aliased(9);
  print(`instantiated:${d}:${e}:alias:${f}`);
}
