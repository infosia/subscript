// corpus: accept/a391-private-names
// purpose: A #name member is a private member with its own name; #x and x are two members; JSON.stringify omits #x.
// exercises: private-names, class-members, json-stringify
// questions: compiler.md §206
// tsc: accepts; js-comparable: yes
class Counter {
  #count: i32 = 0;
  static #made: i32 = 0;
  readonly #step: i32;
  count: i32 = 100;

  constructor(step: i32) {
    this.#step = step;
    Counter.#made += 1;
  }

  #bump(): void {
    this.#count += this.#step;
  }

  static #label(n: i32): string {
    return `n=${n}`;
  }

  get #level(): i32 {
    return this.#count * 2;
  }

  set #level(v: i32) {
    this.#count = v;
  }

  tick(): i32 {
    this.#bump();
    return this.#count;
  }

  same(o: Counter): boolean {
    return o.#count === this.#count;
  }

  peek(o: Counter | null): i32 {
    return o !== null ? o.#count : -1;
  }

  restart(): void {
    this.#level = 0;
  }

  static made(): string {
    return Counter.#label(Counter.#made);
  }

  double(): i32 {
    return this.#level;
  }
}

class Point {
  x: f64;
  #tag: string = "p";
  y: f64;
  constructor(x: f64, y: f64) {
    this.x = x;
    this.y = y;
  }
  tag(): string {
    return this.#tag;
  }
}

export function main(): void {
  const c = new Counter(2);
  c.tick();
  const d = new Counter(3);
  print(`${c.tick()} ${d.tick()} ${c.same(d)} ${Counter.made()} ${c.double()} ${c.count}`);
  print(`${c.peek(d)} ${c.peek(null)}`);
  c.restart();
  print(`${c.tick()}`);
  const p = new Point(1, 2);
  print(`${JSON.stringify(p)} ${p.tag()}`);
}
