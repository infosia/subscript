// corpus: accept/a403-for-of-fixed-array-handles
// purpose: A for-of over a FixedArray of handles and a direct index on a call result read each stored element, with a collection and allocation in the body.
// exercises: for-of, fixed-array, nullable-string, nullable-element, reference-class, forEach, Context.collect
// questions: compiler.md §218
// tsc: accepts; js-comparable: yes
class C {
  n: i32;
  constructor(n: i32) {
    this.n = n;
  }
}

function built(a: string, b: string): string {
  return a + b;
}

function pair(): FixedArray<string, 2> {
  return [built("r", "1"), built("r", "2")];
}

export function main(): void {
  // Each array has no read after its last loop.
  const plain: FixedArray<string, 2> = [built("a", "1"), "b"];
  for (const x of plain) {
    print(x);
  }

  // Two loops over one array, and a body that allocates and collects.
  const twice: FixedArray<string, 3> = [built("t", "1"), built("t", "2"), "t3"];
  for (const x of twice) {
    const longer = built(x, "!");
    Context.collect();
    print(longer);
  }
  for (const x of twice) {
    print(`${x.length} ${x}`);
  }

  const nullable: FixedArray<string | null, 3> = [null, built("n", "1"), ""];
  for (const x of nullable) {
    const fresh = built("x", "y");
    Context.collect();
    print(`${x ?? "none"} ${fresh}`);
  }
  for (const x of nullable) {
    print(`${x === null}`);
  }

  const objects: FixedArray<C | null, 3> = [new C(1), null, new C(3)];
  for (const x of objects) {
    const fresh = new C(9);
    Context.collect();
    print(`${x?.n ?? -1} ${fresh.n}`);
  }
  for (const x of objects) {
    print(`${x === null}`);
  }

  // A subject that no binding names, a nested loop, and `forEach`.
  for (const x of pair()) {
    Context.collect();
    for (const y of pair()) {
      print(`${x}${y}`);
    }
  }
  const each: FixedArray<string, 2> = [built("e", "1"), built("e", "2")];
  each.forEach((value: string, index: i32): void => {
    Context.collect();
    print(`${index} ${value}`);
  });

  // A direct index on a call result.
  print(pair()[0]);
  print(`${pair()[1].length}`);
}
