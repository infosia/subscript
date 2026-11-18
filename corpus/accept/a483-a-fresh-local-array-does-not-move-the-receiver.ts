// corpus: accept/a483-a-fresh-local-array-does-not-move-the-receiver
// purpose: A fresh local array does not move the receiver.
// observable: Both element calls print their results.
// exercises: value-types, array-literals, element-receivers
// questions: compiler.md §333 rules 1 to 4
// tsc: accepts; js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class V {
  a: i32;
  constructor(a: i32) { this.a = a; }
  twice(): V[] {
    const out: V[] = [];
    out.push(this);
    out.push(this);
    return out;
  }
  plus(): i32 {
    let xs: V[] = [new V(1)];
    xs.push(new V(2));
    return this.a + xs.length;
  }
}
const all: V[] = [new V(3), new V(7)];
const result: V[] = all[0].twice();
print(`${result[0].a},${result[1].a}`);
print(`${all[1].plus()}`);
export function main(): void {}
