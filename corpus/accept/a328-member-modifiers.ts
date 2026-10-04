// corpus: accept/a328-member-modifiers
// purpose: Runs each accepted member modifier form.
// exercises: private, protected, readonly, constructor, arrow
// questions: compiler.md §155
// tsc: accepts
// js-comparable: yes

class P {
  private x: i32 = 2;
  protected b: i32 = 3;
  private static n: i32 = 4;
  readonly value: i32 = 9;
  readonly assigned: i32;
  private constructor() {
    this.assigned = 1;
    this.assigned += 1;
    this.assigned++;
    this.value = 1;
    this.value += 1;
    this.value++;
  }
  private f(): i32 { return this.x; }
  protected g(): i32 { return this.b; }
  read(other: P): i32 { return other.x + other.b + other.f() + other.g(); }
  arrow(other: P): i32 {
    const receiver: P = other;
    const read: () => i32 = (): i32 => receiver.x + receiver.b + receiver.f() + receiver.g();
    return read();
  }
  static create(): P { return new P(); }
  static read(other: P): i32 { P.n += 1; return other.x + other.b + other.f() + other.g() + P.n; }
}
class Q {
  protected constructor() {}
  static create(): Q { return new Q(); }
  value(): i32 { return 7; }
}
abstract class S {
  static value: i32 = 8;
}
function useAbstract(value: S): void {}
export function main(): void {
  const p: P = P.create();
  const q: P = P.create();
  print(`${p.read(q)}`);
  print(`${p.arrow(q)}`);
  print(`${P.read(q)}`);
  print(`${p.assigned},${p.value}`);
  print(`${Q.create().value()}`);
  print(`${S.value}`);
}
