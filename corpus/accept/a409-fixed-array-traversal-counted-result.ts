// corpus: accept/a409-fixed-array-traversal-counted-result
// purpose: A traversal of a call result of counted elements has one owner in its hidden local, on a normal exit, on a break, on a return, and on a raise.
// exercises: for-of, fixed-array, generator, async, await, break, return, try-catch, Context.collect
// questions: compiler.md §220.1 rule 7
// tsc: accepts; js-comparable: yes
function* nums(n: i32): Generator<i32> {
  yield n;
  yield n + 1;
}

function gs(): FixedArray<Generator<i32>, 2> {
  return [nums(1), nums(5)];
}

async function value(n: i32): Promise<i32> {
  await Context.suspend();
  return n;
}

function ps(): FixedArray<Promise<i32>, 2> {
  return [value(3), value(4)];
}

function generators(): void {
  for (const g of gs()) {
    for (const v of g) {
      print(`${v}`);
    }
  }
}

function earlyExit(): void {
  for (const g of gs()) {
    print(`${g.next().value}`);
    break;
  }
  Context.collect();
}

function earlyReturn(): i32 {
  for (const g of gs()) {
    return g.next().value + 10;
  }
  return 0;
}

function raised(): void {
  try {
    for (const g of gs()) {
      if (g.next().value === 1) {
        throw new Error("stop");
      }
    }
  } catch (e) {
    Context.collect();
    print("caught");
  }
}

function* relay(): Generator<i32> {
  for (const g of gs()) {
    yield g.next().value;
    Context.collect();
    yield g.next().value;
  }
}

function* placeRelay(): Generator<i32> {
  const a = gs();
  for (const g of a) {
    yield g.next().value;
    Context.collect();
    yield g.next().value;
  }
}

function* namedRelay(): Generator<i32> {
  const a = gs();
  for (let i: i32 = 0; i < 2; i += 1) {
    const g = a[i];
    yield g.next().value;
  }
}

async function promises(): Promise<void> {
  for (const p of ps()) {
    print(`${await p}`);
  }
  for (const p of ps()) {
    print(`${await p}`);
    break;
  }
  Context.collect();
}

export async function main(): Promise<void> {
  generators();
  earlyExit();
  print(`${earlyReturn()}`);
  raised();
  for (const v of relay()) {
    print(`${v}`);
  }
  for (const v of placeRelay()) {
    print(`${v}`);
    break;
  }
  for (const v of placeRelay()) {
    print(`${v}`);
  }
  for (const v of namedRelay()) {
    print(`${v}`);
  }
  await promises();
}
