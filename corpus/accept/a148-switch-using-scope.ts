// corpus: accept/a148-switch-using-scope
// purpose: Disposes resources at the exit of one switch-body scope.
// exercises: using-declaration, switch-body-scope, fallthrough, reverse-disposal
// questions: §67, §60
// tsc: accepts
class Resource {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose:${this.label}`);
  }
}

export function main(): void {
  const selected: i32 = 0;
  switch (selected) {
    case 0:
      using a = new Resource("a");
      print("case0");
    case 1:
      using b = new Resource("b");
      print("case1");
      break;
    default:
      break;
  }
  print("end");
}
