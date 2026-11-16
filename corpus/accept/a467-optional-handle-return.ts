// corpus: accept/a467-optional-handle-return
// purpose: An async void result adopts an optional handle or completes with no value.
// observable: The present receiver prints am, and both returns complete before end.
// exercises: async-function, optional-chain, return, await
// questions: compiler.md §318 rule 2
// tsc: accepts; js-comparable: yes
class S { async am(): Promise<void> { print("am"); } }
async function g(s: S | null): Promise<void> { return s?.am(); }

export async function main(): Promise<void> {
  await g(new S());
  await g(null);
  print("end");
}
