// corpus: accept/a388-do-while
// purpose: A do…while body runs once before the first test; continue goes to the test; the body assigns definitely; the exit narrows.
// exercises: do-while, loop-control, definite-assignment, narrowing
// questions: compiler.md §203
// tsc: accepts; js-comparable: yes
class Item {
  v: i32;
  constructor(v: i32) {
    this.v = v;
  }
}

let tests: i32 = 0;

function more(i: i32): boolean {
  tests += 1;
  return i < 5;
}

let made: i32 = 0;

function maybe(): Item | null {
  made += 1;
  return made < 3 ? null : new Item(made);
}

function* countdown(from: i32): Generator<i32> {
  let n = from;
  do {
    yield n;
    n -= 1;
  } while (n > 0);
}

export function main(): void {
  let i: i32 = 0;
  let sum: i32 = 0;
  do {
    i += 1;
    if (i === 2) {
      continue;
    }
    if (i === 4) {
      break;
    }
    sum += i;
  } while (more(i));
  print(`${i} ${sum} ${tests}`);

  let once: i32 = 10;
  do {
    once += 1;
  } while (once < 5);
  print(`${once}`);

  let n: i32;
  do {
    n = 3;
  } while (false);
  print(`${n}`);

  let it: Item | null = null;
  do {
    it = maybe();
  } while (it === null);
  print(`${it.v} ${made}`);

  let rows = "";
  let r: i32 = 0;
  do {
    let c: i32 = 0;
    do {
      rows += `${r}${c} `;
      c += 1;
    } while (c < 2);
    r += 1;
  } while (r < 2);
  print(rows);

  const seen: i32[] = [];
  for (const v of countdown(3)) {
    seen.push(v);
  }
  print(seen.join(","));
}
