// corpus: accept/a381-logical-assignment
// purpose: Logical assignment tests the place once, evaluates the right operand only when it assigns, and narrows a nullable place.
// exercises: nullish-assignment, logical-assignment, narrowing, evaluation-order
// questions: compiler.md §199
// tsc: accepts; js-comparable: yes
class Box {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

let created: i32 = 0;

function make(v: i32): Box {
  created += 1;
  return new Box(v);
}

class Holder {
  cache: Box | null = null;
  ready: boolean = false;
  get(): Box {
    this.cache ??= make(1);
    return this.cache;
  }
}

let calls: i32 = 0;

function pick(h: Holder): Holder {
  calls += 1;
  return h;
}

export function main(): void {
  let x: Box | null = null;
  x ??= make(2);
  x ??= make(3);
  print(`${x.v} ${created}`);
  const h = new Holder();
  print(`${h.get().v} ${h.get().v} ${created}`);
  const slots: (Box | null)[] = [null, new Box(9)];
  for (let i: i32 = 0; i < slots.length; i++) {
    slots[i] ??= make(5);
  }
  print(`${slots[0]?.v ?? -1} ${slots[1]?.v ?? -1} ${created}`);
  pick(h).cache = null;
  pick(h).cache ??= make(6);
  print(`${h.get().v} ${calls} ${created}`);
  let a: boolean = true;
  let b: boolean = false;
  a &&= false;
  b ||= true;
  h.ready ||= a;
  print(`${a} ${b} ${h.ready}`);
  let guard: boolean = false;
  guard &&= make(7).v > 0;
  print(`${guard} ${created}`);
  const h2 = new Holder();
  const got = (h2.cache ??= make(8));
  const again = (h2.cache ??= make(9));
  print(`${got.v} ${again.v} ${created}`);
}
