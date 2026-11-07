// corpus: accept/a426-recursive-void-lambda
// purpose: A recursive lambda with a block body and no `return` operand decides its `void` type from its syntax, so its body can call the binding that holds it.
// observable: each recursive call prints one line, in the order node prints them.
// exercises: lambda, void, recursion, async-arrow, await
// questions: compiler.md §239, §238
// tsc: accepts; js-comparable: yes
let total: i32 = 0;

function note(n: i32): void {
  print(`note${n}`);
}

const rec = (n: i32) => {
  print(`r${n}`);
  if (n > 0) rec(n - 1);
};

const ping = (n: i32) => {
  if (n > 0) pong(n - 1);
};

const pong = (n: i32) => {
  print(`p${n}`);
  ping(n);
};

const arec = async (n: i32) => {
  print(`a${n}`);
  if (n > 0) {
    await arec(n - 1);
  }
};

const count = (n: i32) => {
  total = total + n;
  note(n);
  if (n > 0) count(n - 1);
};

export async function main(): Promise<void> {
  rec(2);
  ping(3);
  await arec(2);
  count(3);
  print(`total=${total}`);
}
