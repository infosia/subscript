// corpus: accept/a256-try-in-async
// purpose: A try block with no suspension catches a raise inside an async function, and its catch block awaits.
// exercises: try-catch, async-function, await-in-catch, held-handle, instanceof-narrowing
// questions: Q9, Q34
// tsc: accepts; js-comparable: yes
function check(value: i32): i32 {
  if (value < 0) {
    throw new TypeError(`negative ${value}`);
  }
  return value * 2;
}

async function report(tag: string): Promise<i32> {
  print(`report:${tag}`);
  return 7;
}

async function guarded(value: i32): Promise<i32> {
  print(`guarded:start ${value}`);
  let result: i32 = 0;
  try {
    result = check(value);
    print(`guarded:checked ${result}`);
  } catch (e) {
    if (e instanceof TypeError) {
      print(`guarded:caught ${e.message}`);
    }
    result = await report("catch");
    print(`guarded:after-await ${result}`);
    if (e instanceof TypeError) {
      print(`guarded:still ${e.name}`);
    }
  }
  const tail: i32 = await report("tail");
  return result + tail;
}

export async function main(): Promise<void> {
  const first: i32 = await guarded(3);
  print(`main:first=${first}`);
  const second: i32 = await guarded(-4);
  print(`main:second=${second}`);
  const held: Promise<i32> = guarded(-1);
  print("main:held");
  print(`main:held=${await held}`);
}
