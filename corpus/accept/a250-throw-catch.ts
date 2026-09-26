// corpus: accept/a250-throw-catch
// purpose: Throws each Error class, catches it with each catch form, tests each class with instanceof, and continues after the handler.
// exercises: throw, try-catch, catch-binding, catch-without-binding, instanceof-narrowing, error-name, error-message
// questions: Q9
// tsc: accepts; js-comparable: yes
function fail(message: string): void {
  throw new Error(message);
}

function failSyntax(): void {
  throw new SyntaxError("bad token");
}

function failType(): void {
  throw new TypeError("wrong kind");
}

function classify(kind: i32): string {
  try {
    if (kind === 0) {
      fail("plain");
    } else if (kind === 1) {
      failSyntax();
    } else if (kind === 2) {
      failType();
    }
  } catch (e) {
    if (e instanceof SyntaxError) {
      return `SyntaxError branch: ${e.name}: ${e.message}`;
    }
    if (e instanceof TypeError) {
      return `TypeError branch: ${e.name}: ${e.message}`;
    }
    if (e instanceof Error) {
      return `Error branch: ${e.name}: ${e.message}`;
    }
  }
  return "no exception";
}

export function main(): void {
  print(classify(0));
  print(classify(1));
  print(classify(2));
  print(classify(3));

  let reached: i32 = 0;
  try {
    reached = 1;
    fail("unbound");
    reached = 2;
  } catch {
    print(`catch without a binding after step ${reached}`);
  }

  try {
    throw new Error();
  } catch (e: unknown) {
    if (e instanceof Error) {
      print(`empty message length ${e.message.length}`);
      e.name = "Renamed";
      e.message = "changed";
      print(`${e.name}: ${e.message}`);
    }
  }

  const held: TypeError = new TypeError("held");
  if (held instanceof SyntaxError) {
    print("held is a SyntaxError");
  }
  if (held instanceof TypeError) {
    print(`held is a TypeError: ${held.message}`);
  }
  print(`after the handlers ${reached}`);
}
