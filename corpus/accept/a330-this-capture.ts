// corpus: accept/a330-this-capture
// purpose: Captures a reference receiver in callbacks and across suspensions.
// exercises: this, capture, forEach, map, filter, sort, nested-lambda, async, accessor, constructor
// questions: C5, compiler.md §157
// tsc: accepts; js-comparable: yes

async function pause(): Promise<void> {}

class Counter {
  total: i32;
  constructor() {
    this.total = 1;
    const read = (): i32 => this.total;
    print(`constructor ${read()}`);
  }
  callbacks(): void {
    const xs: i32[] = [3, 1, 2];
    xs.forEach((v: i32): void => { this.total += v; });
    const mapped: i32[] = xs.map((v: i32): i32 => {
      this.total += 1;
      return v + this.total;
    });
    const filtered: i32[] = mapped.filter((v: i32): boolean => {
      this.total += 1;
      return v > this.total;
    });
    xs.sort((a: i32, b: i32): i32 => {
      this.total += 0;
      return (a + this.total) - (b + this.total);
    });
    const outer = (): i32 => {
      const inner = (): i32 => { this.total += 2; return this.total; };
      return inner();
    };
    print(`callbacks ${mapped.join(",")} ${filtered.join(",")} ${xs.join(",")} ${outer()}`);
  }
  get value(): i32 {
    const read = (): i32 => this.total;
    return read();
  }
  async resume(): Promise<void> {
    const advance = (): i32 => { this.total += 3; return this.total; };
    await pause();
    print(`async ${advance()}`);
  }

}

export async function main(): Promise<void> {
  const counter: Counter = new Counter();
  counter.callbacks();
  print(`accessor ${counter.value}`);
  await counter.resume();
}
