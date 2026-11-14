// corpus: accept/a451-a-return-of-a-void-operand-keeps-a-lambda-void
// purpose: A return of a void operand keeps an unannotated block lambda void.
// observable: synchronous, asynchronous, recursive and nested lambdas log in order.
// exercises: lambda, void, async-arrow, self-call, nested-lambda
// questions: compiler.md §286, §238
// tsc: accepts; js-comparable: yes
const log: string[] = [];
function v(): void { log.push("v"); }
async function w1(): Promise<void> { log.push("w1"); }
export async function main(): Promise<void> {
  const h = (c: boolean) => { if (c) return v(); log.push("h"); };
  const a = async (c: boolean) => { if (c) return await w1(); log.push("a"); };
  const z = (c: boolean) => { if (c) { return; } return v(); };
  const rec = (n: i32) => { if (n <= 0) return v(); log.push(`r${n}`); rec(n - 1); };
  const nested = () => { const inner = (): i32 => { return 7; }; log.push(`inner=${inner()}`); return v(); };
  h(true); h(false);
  await a(true); await a(false);
  z(true); z(false);
  rec(2); nested();
  print(log.join(" "));
}
