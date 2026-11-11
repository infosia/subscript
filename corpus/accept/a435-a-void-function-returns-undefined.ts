// corpus: accept/a435-a-void-function-returns-undefined
// purpose: A function, a method, or a lambda with the result void, and an async function with the result Promise<void>, returns undefined as return.
// observable: an early return undefined skips a print, so the end of each body is visible.
// exercises: return, undefined, void, lambda, async-function, await, method
// questions: compiler.md §250, §238, §237, §195.2
// tsc: accepts; js-comparable: yes
type CB = () => void;

function f(): void {
  print("f");
  return undefined;
}

function g(b: boolean): void {
  if (b) return undefined;
  print("g");
}

class Logger {
  count: i32 = 0;
  log(skip: boolean): void {
    if (skip) {
      return (undefined);
    }
    this.count = this.count + 1;
    print(`log ${this.count}`);
  }
}

async function a(): Promise<void> {
  print("a");
  return undefined;
}

async function early(b: boolean): Promise<void> {
  if (b) {
    return undefined;
  }
  print("early");
}

function side(): void {
  print("side");
}

function leaf(b: boolean): void {
  return b ? undefined : side();
}

function run(cb: CB): void {
  cb();
}

export async function main(): Promise<void> {
  f();
  g(true);
  g(false);
  const h = (): void => {
    print("h");
    return undefined;
  };
  h();
  const k = (): void => undefined;
  k();
  print("k");
  run(() => undefined);
  run(() => {
    print("run");
    return undefined;
  });
  leaf(true);
  leaf(false);
  const logger = new Logger();
  logger.log(true);
  logger.log(false);
  await a();
  await early(true);
  await early(false);
  const al = async (): Promise<void> => {
    print("al");
    return undefined;
  };
  await al();
}
