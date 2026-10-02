// corpus: accept/a317-function-field-initializer-calls
// purpose: Calls an earlier initialized function field with both call forms.
// exercises: field-initializer, this-binding, function-value
// questions: compiler.md §147, collisions.md C9
// tsc: accepts; js-comparable: yes
class A {
  cb: () => i32 = (): i32 => 3;
  direct: i32 = this.cb();
  parenthesized: i32 = (this.cb)();
}
export function main(): void {
  const a: A = new A();
  print(`${a.direct} ${a.parenthesized}`);
}
