// corpus: accept/a452-an-async-void-lambda-returns-a-promise-of-void
// purpose: An async void lambda returns a Promise of void.
// observable: async returns await void handles and log in order.
// exercises: lambda, void, async-arrow, promise
// questions: compiler.md §287, §286
// tsc: accepts; js-comparable: yes
const log: string[] = [];
async function w1(): Promise<void> { log.push("w1"); }
export async function main(): Promise<void> {
  const g = async (c: boolean) => { if (c) return w1(); log.push("g"); };
  const k = async () => { return w1(); };
  await g(true); await g(false); await k();
  const r: Promise<void> = g(true);
  await r;
  print(log.join(" "));
}
