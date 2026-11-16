// corpus: accept/a465-conditional-handle-discharge
// purpose: Await each existing handle, and discharge handles made in a selected operand.
// observable: Both conditions print the same values in each tier and node.
// exercises: async-function, await, held-handle, conditional, default-parameter, destructuring
// questions: compiler.md §317, §188, §257; collisions.md C8
// tsc: accepts; js-comparable: yes
async function v(n: i32): Promise<i32> { return n; }

async function f(p: Promise<i32>, q: Promise<i32> = v(2)): Promise<i32> {
  const a = await p;
  const b = await q;
  return a + b;
}

async function each(c: boolean): Promise<void> {
  const p = v(1);
  const q = v(2);
  await p;
  await q;
  print(`${c ? await p : await q}`);
  print(`${c ? await v(1) : await v(2)}`);
  print(`${await (c ? v(1) : v(2))}`);
  print(`${await (c ? p : p)}`);
  const r = c ? v(3) : v(4);
  const alias = r;
  print(`${await alias}`);
  const [s] = c ? [v(5)] : [v(6)];
  print(`${await s}`);
  print(`${await f(v(1))}`);
  print(`${await f(v(1), v(7))}`);
}

export async function main(): Promise<void> {
  await each(true);
  await each(false);
}
