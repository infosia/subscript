// corpus: accept/a254-using-hook-raises
// purpose: A dispose hook that raises on a normal exit turns the exit into an exception exit; the remaining hooks run once, and an outer handler catches the exception.
// exercises: using-declaration, symbol-dispose, exception-exit, dispose-raises, return, break, try-catch
// questions: Q9, §60
// tsc: accepts; js-comparable: yes
class Resource {
  label: string;
  fails: boolean;

  constructor(label: string, fails: boolean) {
    this.label = label;
    this.fails = fails;
  }

  [Symbol.dispose](): void {
    print(`dispose ${this.label}`);
    if (this.fails) {
      throw new Error(`${this.label} failed`);
    }
  }
}

function value(): i32 {
  print("return value");
  return 3;
}

function endOfScope(): void {
  using first = new Resource("end first", false);
  using second = new Resource("end second", true);
  using third = new Resource("end third", false);
  print("end body");
}

function earlyReturn(): i32 {
  using outer = new Resource("return outer", false);
  {
    using inner = new Resource("return inner", true);
    return value();
  }
}

function breakLoop(): void {
  using outer = new Resource("break outer", false);
  while (true) {
    using item = new Resource("break item", true);
    break;
  }
  print("unreached after loop");
}

export function main(): void {
  try {
    endOfScope();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  try {
    print(`value ${earlyReturn()}`);
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  try {
    breakLoop();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  print("done");
}
