// corpus: accept/a425-lambda-with-no-value-result-is-void
// purpose: A lambda with no result annotation has the result `void` when its body is a `void` expression, or a block with no `return` operand.
// observable: each lambda logs its calls in order; a `then` chain logs its `void` callback after its value callback.
// exercises: lambda, void, async-arrow, Promise.then, generic-function, generic-method
// questions: compiler.md §238, §237
// tsc: accepts; js-comparable: yes
const log: string[] = [];
const ys: i32[] = [];

function v(): void {
  log.push("v");
}

async function w1(): Promise<void> {
  log.push("w1");
}

async function wi(): Promise<i32> {
  log.push("wi");
  return 5;
}

function each<T>(xs: T[], f: (x: T) => void): void {
  for (const x of xs) {
    f(x);
  }
}

class Walker {
  label: string;
  constructor(label: string) {
    this.label = label;
  }
  visit<T>(items: T[], f: (x: T) => void): void {
    log.push(this.label);
    for (const x of items) {
      f(x);
    }
  }
}

async function chained(): Promise<void> {
  return w1().then(() => v());
}

export async function main(): Promise<void> {
  const f = () => v();
  f();
  const af = async () => v();
  await af();
  const g = () => {
    v();
  };
  g();
  const early = (b: boolean) => {
    if (b) {
      log.push("early");
      return;
    }
    log.push("late");
  };
  early(true);
  early(false);
  const outer = () => {
    const inner = (): i32 => {
      return 7;
    };
    log.push(`inner=${inner()}`);
  };
  outer();
  await w1().then(() => {
    v();
  });
  await wi().then((x: i32) => v());
  await wi()
    .then((x: i32) => x + 1)
    .then((x: i32) => {
      log.push(`after=${x}`);
    });
  await chained();
  const xs: i32[] = [1, 2];
  each(xs, (x: i32) => v());
  each(xs, (x: i32) => ys.push(x));
  each(xs, (x: i32) => {
    log.push(`each${x}`);
  });
  const walker = new Walker("walk");
  walker.visit([3, 4], (x: i32) => {
    log.push(`visit${x}`);
  });
  walker.visit([5], (x: i32) => v());
  log.push(`ys=${ys.join(",")}`);
  print(log.join(" "));
}
