// corpus: accept/a213-using-for-break
// purpose: Retains disposal after a targeting break and at each iteration exit.
// exercises: using-declaration, scope-exit, infinite-loop
// questions: compiler section 101
// tsc: accepts; js-comparable: yes
class R {
  label: string;
  constructor(label: string) { this.label = label; }
  [Symbol.dispose](): void { print(this.label); }
}
export function main(): void {
  using outer: R = new R("outer");
  let i: i32 = 0;
  for (;;) {
    using inner: R | null = new R(`inner:${i}`);
    i = i + 1;
    if (i < 2) { continue; }
    break;
  }
  print("after");
}
