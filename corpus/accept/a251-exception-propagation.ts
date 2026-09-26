// corpus: accept/a251-exception-propagation
// purpose: An exception raised in a callee reaches the caller's handler through a direct call, a nested call, recursion, a method, a function value, a lambda, and forEach callbacks.
// exercises: throw, try-catch, exception-propagation, recursion, method-call, indirect-call, lambda, array-for-each
// questions: Q9, Q10, Q22
// tsc: accepts; js-comparable: yes
function helper(): i32 {
  throw new Error("from the helper");
}

function middle(): i32 {
  return helper() + 1;
}

function outer(): i32 {
  return middle() * 2;
}

function countdown(depth: i32): i32 {
  if (depth === 0) {
    throw new Error("at the bottom of the recursion");
  }
  return countdown(depth - 1) + 1;
}

function failThroughValue(): void {
  throw new TypeError("through a function value");
}

function callThrough(action: () => void): void {
  action();
  print("callThrough continued");
}

function visit(value: i32): void {
  if (value === 3) {
    throw new SyntaxError(`visit stopped at ${value}`);
  }
  print(`visit ${value}`);
}

class Counter {
  count: i32 = 0;

  bump(): void {
    this.count += 1;
    if (this.count > 2) {
      throw new Error(`limit at ${this.count}`);
    }
  }
}

function show(label: string, error: Error): void {
  print(`${label}: ${error.name}: ${error.message}`);
}

export function main(): void {
  try {
    print(`${helper()}`);
  } catch (e) {
    if (e instanceof Error) {
      show("direct", e);
    }
  }

  try {
    print(`${outer()}`);
  } catch (e) {
    if (e instanceof Error) {
      show("nested", e);
    }
  }

  try {
    print(`${countdown(5)}`);
  } catch (e) {
    if (e instanceof Error) {
      show("recursion", e);
    }
  }

  const counter: Counter = new Counter();
  try {
    counter.bump();
    counter.bump();
    counter.bump();
    counter.bump();
  } catch (e) {
    if (e instanceof Error) {
      show(`method after ${counter.count} bumps`, e);
    }
  }

  const action: () => void = failThroughValue;
  try {
    callThrough(action);
  } catch (e) {
    if (e instanceof TypeError) {
      show("function value", e);
    }
  }

  const limit: i32 = 4;
  const check = (value: i32): void => {
    if (value > limit) {
      throw new Error(`${value} is above ${limit}`);
    }
  };
  try {
    check(2);
    check(9);
    print("check passed");
  } catch (e) {
    if (e instanceof Error) {
      show("lambda", e);
    }
  }

  const values: i32[] = [1, 2, 3, 4];
  try {
    values.forEach((value: i32): void => {
      if (value === 2) {
        throw new Error(`forEach lambda stopped at ${value}`);
      }
      print(`lambda ${value}`);
    });
  } catch (e) {
    if (e instanceof Error) {
      show("forEach lambda", e);
    }
  }

  try {
    values.forEach(visit);
  } catch (e) {
    if (e instanceof SyntaxError) {
      show("forEach function", e);
    }
  }

  const visitor: (value: i32) => void = visit;
  try {
    values.forEach(visitor);
  } catch (e) {
    if (e instanceof SyntaxError) {
      show("forEach function value", e);
    }
  }
  print("done");
}
