// corpus: accept/a138-using-dispose
// purpose: Runs synchronous disposal at every completed scope exit.
// exercises: using-declaration, symbol-dispose, reverse-disposal, scope-exit
// questions: §60, R31

class Resource {
  label: string;

  constructor(label: string) {
    this.label = label;
  }

  [Symbol.dispose](): void {
    print(`dispose:${this.label}`);
  }
}

function valueForReturn(): i32 {
  print("return:value");
  return 37;
}

function returnWithResource(): i32 {
  using resource = new Resource("return");
  return valueForReturn();
}

function returnEarly(stop: boolean): void {
  using outer = new Resource("early:outer");
  if (stop) {
    using inner = new Resource("early:inner");
    print("early:return");
    return;
  }
  using late = new Resource("early:late");
  print("early:fallthrough");
}

export function main(): void {
  {
    using a = new Resource("a"), b = new Resource("b");
    print("body");
  }

  const returned: i32 = returnWithResource();
  print(`returned:${returned}`);
  returnEarly(true);

  for (let iteration: i32 = 0; iteration < 3; iteration += 1) {
    using resource = new Resource(`loop${iteration}`);
    print(`iter${iteration}`);
    if (iteration === 1) {
      break;
    }
  }
}
