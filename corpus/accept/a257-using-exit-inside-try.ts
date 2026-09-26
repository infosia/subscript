// corpus: accept/a257-using-exit-inside-try
// purpose: A dispose hook that raises on a return, a break, or a continue inside a try block runs outside that try; the inner catch does not catch it, each hook runs once, and the outer handler catches the exception.
// exercises: using-declaration, symbol-dispose, exception-exit, dispose-raises, return, break, continue, try-catch
// questions: Q9, §60
// tsc: accepts; js-comparable: yes
class Failing {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose ${this.label}`);
    throw new Error(`${this.label} failed`);
  }
}

class Quiet {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose ${this.label}`);
  }
}

function returnInsideTry(): void {
  using r = new Failing("return");
  try {
    return;
  } catch {
    print("return: caught inside");
  }
  print("return: after try");
}

function valueInsideTry(): i32 {
  using outer = new Quiet("value outer");
  using inner = new Failing("value inner");
  try {
    return 1;
  } catch {
    print("value: caught inside");
    return 2;
  }
}

function breakInsideTry(): void {
  let i: i32 = 0;
  while (i < 3) {
    using r = new Failing(`break ${i}`);
    try {
      i = i + 1;
      break;
    } catch {
      print("break: caught inside");
    }
    print("break: after try");
  }
  print(`break: after loop ${i}`);
}

function continueInsideTry(): void {
  for (let i: i32 = 0; i < 3; i = i + 1) {
    using r = new Failing(`continue ${i}`);
    try {
      continue;
    } catch {
      print("continue: caught inside");
    }
    print("continue: after try");
  }
  print("continue: after loop");
}

function enclosingHandler(): void {
  try {
    using r = new Failing("enclosing");
    try {
      return;
    } catch {
      print("enclosing: caught inside");
    }
    print("enclosing: after inner try");
  } catch (e) {
    if (e instanceof Error) {
      print(`enclosing: caught ${e.message}`);
    }
  }
  print("enclosing: after outer try");
}

export function main(): void {
  try {
    returnInsideTry();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught outside: ${e.message}`);
    }
  }
  try {
    print(`value ${valueInsideTry()}`);
  } catch (e) {
    if (e instanceof Error) {
      print(`caught outside: ${e.message}`);
    }
  }
  try {
    breakInsideTry();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught outside: ${e.message}`);
    }
  }
  try {
    continueInsideTry();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught outside: ${e.message}`);
    }
  }
  enclosingHandler();
  print("done");
}
