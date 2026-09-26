// corpus: accept/a252-nested-handlers-rethrow
// purpose: Nested handlers catch in the nearest try, a rethrow reaches the outer handler with the same object, and break, continue, and return leave try and catch blocks.
// exercises: try-catch, nested-handlers, rethrow, reference-identity, break, continue, return
// questions: Q9
// tsc: accepts; js-comparable: yes
let saved: Error | null = null;

function rethrowFirst(): void {
  try {
    throw new Error("first");
  } catch (e) {
    if (e instanceof Error) {
      saved = e;
      print(`inner caught ${e.message}`);
    }
    throw e;
  }
}

function replaceInner(): string {
  try {
    try {
      throw new SyntaxError("inner");
    } catch (e) {
      if (e instanceof SyntaxError) {
        print(`nearest handler caught ${e.message}`);
      }
      throw new TypeError("replacement");
    }
  } catch (e) {
    if (e instanceof TypeError) {
      return `outer handler caught ${e.message}`;
    }
  }
  return "nothing caught";
}

function scan(limit: i32): string {
  let trace: string = "";
  for (let i: i32 = 0; i < limit; i++) {
    try {
      if (i === 1) {
        trace += "c";
        continue;
      }
      if (i === 2 || i === 4) {
        throw new Error(`${i}`);
      }
      if (i === 6) {
        trace += "b";
        break;
      }
      trace += `${i}`;
    } catch (e) {
      if (e instanceof Error) {
        trace += `[${e.message}]`;
        if (e.message === "4") {
          continue;
        }
      }
      trace += "!";
    }
    trace += ";";
  }
  return trace;
}

function search(values: i32[], wanted: i32): i32 {
  for (const value of values) {
    try {
      if (value === wanted) {
        return value * 10;
      }
      if (value < 0) {
        throw new Error("negative");
      }
    } catch (e) {
      return -1;
    }
  }
  return 0;
}

function countUntilFailure(): i32 {
  let count: i32 = 0;
  while (count < 10) {
    try {
      count += 1;
      if (count === 3) {
        throw new Error("stop");
      }
    } catch {
      break;
    }
  }
  return count;
}

export function main(): void {
  try {
    rethrowFirst();
  } catch (e) {
    const first: Error | null = saved;
    if (e instanceof Error && first !== null) {
      print(`outer caught ${e.message}`);
      print(`same object: ${e === first}`);
    }
  }
  print(replaceInner());
  print(scan(9));
  print(`${search([4, 7, 9], 7)}`);
  print(`${search([4, -7, 9], 9)}`);
  print(`${search([4, 7], 5)}`);
  print(`${countUntilFailure()}`);
}
