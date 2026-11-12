// corpus: accept/a441-a-parameter-default-discharges-its-handle
// purpose: The default of a parameter whose type holds a handle discharges the handle that its call makes.
// observable: each form prints the awaited value, so each default runs and each handle completes.
// exercises: async-function, await, default-parameter, method, held-handle
// questions: compiler.md §257, §188
// tsc: accepts; js-comparable: yes
async function v(): Promise<i32> {
  return 1;
}

let done: i32 = 0;

async function u(): Promise<void> {
  done += 1;
}

async function one(p: Promise<i32> = v()): Promise<i32> {
  return await p;
}

async function two(p: Promise<i32> = v(), k: i32 = 2): Promise<i32> {
  return (await p) + k;
}

async function three(p: Promise<void> = u()): Promise<void> {
  await p;
}

class C {
  async m(p: Promise<i32> = v()): Promise<i32> {
    return (await p) + 10;
  }
}

async function all(xs: Promise<i32>[] = [v(), v()]): Promise<i32> {
  let total = 0;
  for (const h of xs) {
    total += await h;
  }
  return total;
}

export async function main(): Promise<void> {
  print(`one ${await one()}`);
  print(`two ${await two()}`);
  await three();
  print(`three ${done}`);
  const c = new C();
  print(`method ${await c.m()}`);
  print(`all ${await all()}`);
}
