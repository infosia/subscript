// corpus: accept/a201-using-nullable-switch
// purpose: Tests the active flag before nullable switch disposal storage.
// exercises: using-declaration, nullable-reference, switch-body-scope, fallthrough
// questions: compiler section 97, compiler section 60
// tsc: accepts; js-comparable: yes
class Resource {
  label: string;
  constructor(label: string) { this.label = label; }
  [Symbol.dispose](): void { print(`dispose:${this.label}`); }
}
function make(label: string, live: boolean): Resource | null {
  print(`make:${label}`);
  if (live) { return new Resource(label); }
  return null;
}
function select(selected: i32): void {
  switch (selected) {
    case 0:
      using skipped = make("skipped", true);
    case 1:
      print("entry:1");
    case 2:
      using live = make("live", true), absent = make("null", false);
      print("entry:2");
      break;
    default:
      print("default");
  }
  print("end");
}
export function main(): void {
  select(1);
  select(3);
}
