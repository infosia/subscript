// corpus: accept/a205-using-exhaustive-switch
// purpose: Keeps execution facts exact when every switch arm returns through using disposal.
// exercises: using-declaration, nullable-reference, exhaustive-switch, return
// questions: compiler section 97
// tsc: accepts; js-comparable: yes
class Resource {
  [Symbol.dispose](): void { print("dispose"); }
}
function select(n: i32, a: i32[]): i32 {
  using resource: Resource | null = new Resource();
  switch (n) {
    case 0: return a[0];
    default: return a[1];
  }
}
export function main(): void {
  print(`${select(0, [10, 20])}`);
  print(`${select(1, [10, 20])}`);
}
