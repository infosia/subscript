// corpus: accept/a200-using-nullable-async
// purpose: Keeps nullable disposal receivers reachable across suspension and collection.
// exercises: using-declaration, nullable-reference, async-function, explicit-collection
// questions: compiler section 97, compiler section 60
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
class Resource {
  label: string;
  constructor(label: string) { this.label = label; }
  [Symbol.dispose](): void { print(`dispose:${this.label}`); }
}
function make(live: boolean): Resource | null {
  if (live) { return new Resource("async"); }
  return null;
}
async function hold(): Promise<void> {
  using absent = make(false), live = make(true);
  print("hold:before");
  await Context.suspend();
  print("hold:after");
}
export async function main(): Promise<void> {
  const pending = hold();
  print("collect:before");
  Context.collect();
  print("collect:after");
  await pending;
  print("main:after");
}
