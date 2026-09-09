// corpus: accept/a199-using-nullable
// purpose: Skips null disposal and preserves evaluation and scope exit order.
// exercises: using-declaration, nullable-reference, scope-exit, null-narrowing
// questions: compiler section 97, compiler section 60
// tsc: accepts; js-comparable: yes
let evaluations: i32 = 0;
class Resource {
  label: string;
  constructor(label: string) { this.label = label; }
  [Symbol.dispose](): void { print(`dispose:${this.label}`); }
}
function make(label: string, live: boolean): Resource | null {
  evaluations += 1;
  print(`make:${label}:${evaluations}`);
  if (live) { return new Resource(label); }
  return null;
}
function result(): i32 { print("return:value"); return 7; }
function early(): i32 {
  using outer = make("outer", true);
  {
    using absent = make("inner:null", false), inner = make("inner", true);
    return result();
  }
}
export function main(): void {
  {
    using absent = make("null", false);
    print(`after:null:${evaluations}`);
    using live = make("live", true);
    print(`after:live:${evaluations}`);
    if (live !== null) { print(`read:${live.label}`); }
    print("natural");
  }
  {
    using annotated: Resource | null = null;
    print("annotated:body");
  }
  {
    using first = make("first", true), hole = make("hole", false), last = make("last", true);
    print("multi:body");
  }
  const returned = early();
  print(`returned:${returned}`);
  for (let i: i32 = 0; i < 3; i += 1) {
    using live = make(`loop:${i}`, true), absent = make("loop:null", false);
    if (i === 0) { print("continue"); continue; }
    print("break");
    break;
  }
  print(`evaluations:${evaluations}`);
}
