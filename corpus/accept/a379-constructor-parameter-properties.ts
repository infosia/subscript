// corpus: accept/a379-constructor-parameter-properties
// purpose: A constructor parameter property declares a field, takes its modifiers and default, and is assigned after the field initializers.
// exercises: class, constructor, parameter-property, field-order, JSON.stringify
// questions: compiler.md §198
// tsc: accepts; js-comparable: yes
class Point {
  label: string = "p";
  constructor(public x: i32, public readonly y: i32) {}
  length2(): i32 {
    return this.x * this.x + this.y * this.y;
  }
}

class Account {
  history: i32[] = [];
  total: i32;
  constructor(private owner: string, protected limit: i32 = 100) {
    this.total = this.limit * 2;
  }
  describe(): string {
    return `${this.owner} ${this.limit} ${this.total}`;
  }
}

export function main(): void {
  const p = new Point(3, 4);
  p.x = 6;
  print(`${p.x} ${p.y} ${p.label} ${p.length2()}`);
  print(JSON.stringify(p));
  const a = new Account("ann");
  const b = new Account("bob", 7);
  print(`${a.describe()} | ${b.describe()}`);
}
