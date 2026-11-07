// corpus: accept/a427-named-function-takes-a-void-result-place
// purpose: A named function or a static method with a value result takes a place whose function type has the result `void`; the call discards the result.
// observable: each named function prints its argument, so the order of calls is visible.
// exercises: function-value, void, forEach, Map, Set, static-method, field, spread, Promise.finally
// questions: compiler.md §240, §235, §237
// tsc: accepts; js-comparable: yes
function named(x: i32): i32 {
  print(`named${x}`);
  return x * 2;
}

function two(x: i32, i: i32): boolean {
  print(`two${x}@${i}`);
  return i > 0;
}

function entry(value: string, key: string): i32 {
  print(`entry${key}=${value}`);
  return value.length;
}

function member(value: string): string {
  print(`member${value}`);
  return value;
}

class K {
  static twice(x: i32): i32 {
    print(`twice${x}`);
    return x * 2;
  }
}

class Holder {
  g: (x: i32) => void = named;
  f: (x: i32) => void;
  constructor(f: (x: i32) => void) {
    this.f = f;
  }
}

function each(xs: i32[], f: (x: i32) => void): void {
  for (const x of xs) {
    f(x);
  }
}

async function one(): Promise<i32> {
  print("one");
  return 1;
}

function tick(): boolean {
  print("tick");
  return true;
}

function pick(): (x: i32) => void {
  return named;
}

export async function main(): Promise<void> {
  const xs: i32[] = [1, 2];
  xs.forEach(named);
  xs.forEach(two);
  each(xs, named);
  each(xs, K.twice);
  const cb: (x: i32) => void = named;
  cb(3);
  const m = new Map<string, string>();
  m.set("a", "x");
  m.set("b", "yy");
  m.forEach(entry);
  const s = new Set<string>();
  s.add("p");
  s.add("q");
  s.forEach(member);
  const holder = new Holder(named);
  holder.f(4);
  holder.f = K.twice;
  holder.f(5);
  holder.g(7);
  pick()(6);
  let handler: ((x: i32) => void) | null = null;
  handler ??= named;
  if (handler !== null) {
    handler(8);
  } else {
    print("no handler");
  }
  const none: ((x: i32) => void) | null = null;
  const fallback = none ?? K.twice;
  fallback(9);
  const base: ((x: i32) => void)[] = [K.twice];
  const all: ((x: i32) => void)[] = [...base, named];
  for (const g of all) {
    g(10);
  }
  print(`${await one().finally(tick)}`);
}
