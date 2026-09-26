// corpus: accept/a253-using-exception-exit
// purpose: An exception exit runs each dispose hook once, in reverse declaration order and innermost scope first, skips a null binding, and the normal exits keep their order.
// exercises: using-declaration, symbol-dispose, exception-exit, reverse-disposal, nullable-using, switch-using, try-catch
// questions: Q9, §60, §97
// tsc: accepts; js-comparable: yes
class Resource {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose ${this.label}`);
  }
}

function fail(message: string): void {
  throw new Error(message);
}

function check(raise: boolean, message: string): void {
  if (raise) {
    fail(message);
  }
}

function marker(text: string): i32 {
  print(`return ${text}`);
  return 7;
}

function twoScopes(): void {
  using outer = new Resource("outer");
  print("outer body");
  {
    using first = new Resource("first");
    using second = new Resource("second");
    print("inner body");
    fail("two scopes");
    print("unreached");
  }
}

function nullBinding(): void {
  using first = new Resource("first");
  using absent: Resource | null = null;
  using last = new Resource("last");
  print("null body");
  fail("null binding");
}

function normalExits(stop: boolean): i32 {
  using outer = new Resource("normal outer");
  {
    using inner = new Resource("normal inner");
    if (stop) {
      return marker("early");
    }
    check(false, "not raised");
  }
  print("after block");
  return marker("late");
}

function loopExits(): void {
  for (let i: i32 = 0; i < 3; i++) {
    using item = new Resource(`loop ${i}`);
    if (i === 0) {
      continue;
    }
    check(i === 1, `loop ${i}`);
    print("unreached in loop");
  }
}

function switchExit(n: i32): void {
  using outer = new Resource("switch outer");
  switch (n) {
    case 1:
      using held = new Resource("switch held");
      check(true, "switch");
      break;
    default:
      print("switch default");
  }
}

export function main(): void {
  try {
    twoScopes();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  try {
    nullBinding();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  print(`value ${normalExits(true)}`);
  print(`value ${normalExits(false)}`);
  try {
    loopExits();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  try {
    switchExit(1);
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
  switchExit(2);
  print("done");
}
