// corpus: accept/a395-interface-async-member
// purpose: An interface member with a Promise result is implemented by an async method, and a constrained call awaits it.
// exercises: interface, implements, generic-constraint, async
// questions: compiler.md §208.1 rule 3
// tsc: accepts; js-comparable: yes
interface Job {
  run(n: i32): Promise<i32>;
  get label(): string;
}

class Double implements Job {
  async run(n: i32): Promise<i32> {
    return n * 2;
  }
  get label(): string {
    return "double";
  }
}

class Square implements Job {
  label: string = "square";
  async run(n: i32): Promise<i32> {
    await Context.suspend();
    return n * n;
  }
}

async function go<T extends Job>(job: T, n: i32): Promise<string> {
  const v = await job.run(n);
  return `${job.label}=${v}`;
}

export async function main(): Promise<void> {
  print(await go(new Double(), 5));
  print(await go(new Square(), 4));
}
