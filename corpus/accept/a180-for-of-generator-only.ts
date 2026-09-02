// corpus: accept/a180-for-of-generator-only
// purpose: Drives generators only through for-of across every loop exit shape and a value-class element.
// observable: Sums pin full traversal, break, continue, and value-class field reads.
// exercises: generator, for-of, break, continue, cstruct-field-read
// questions: C8, §83
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.

function* values(): Generator<i32> {
  yield 3;
  yield 5;
  yield 8;
}

@CStruct
class Item {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

function* items(): Generator<Item> {
  yield new Item(13);
  yield new Item(21);
}

export function main(): void {
  let full: i32 = 0;
  for (const value of values()) {
    full += value;
  }
  print(`full:${full}`);

  let stopped: i32 = 0;
  for (const value of values()) {
    stopped += value;
    if (value === 5) {
      break;
    }
  }
  print(`break:${stopped}`);

  let skipped: i32 = 0;
  for (const value of values()) {
    if (value === 5) {
      continue;
    }
    skipped += value;
  }
  print(`continue:${skipped}`);

  let fields: i32 = 0;
  for (const item of items()) {
    fields += item.value;
  }
  print(`fields:${fields}`);
}
