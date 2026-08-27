// corpus: accept/a136-index-signature
// purpose: Uses class index signatures as read and write accessor sugar.
// exercises: class-index-signature, readonly-index-read, mutable-index-write, generic-class
// questions: §58, R29
// tsc: accepts
class ReadOnlyIndex<T> {
  readonly [index: u32]: T;
  data: T[] = [];

  constructor(value: T) {
    this.data.push(value);
  }

  get(index: u32): T {
    return this.data[index as i32];
  }
}

class MutableIndex<T> {
  [index: u32]: T;
  data: T[] = [];

  fill(value: T): void {
    this.data.push(value);
  }

  get(index: u32): T {
    return this.data[index as i32];
  }

  set(index: u32, value: T): void {
    this.data[index as i32] = value;
  }
}

export function main(): void {
  const integer: MutableIndex<i32> = new MutableIndex<i32>();
  integer.fill(10);
  const integerIndex: u32 = 0;
  integer[integerIndex] = 42;
  print(`${integer[integerIndex]} ${integer[integerIndex] === integer.get(integerIndex)}`);

  const word: MutableIndex<string> = new MutableIndex<string>();
  word.fill("old");
  const wordIndex: u32 = 0;
  word[wordIndex] = "new";
  print(`${word[wordIndex]}`);

  const readonlyInteger: ReadOnlyIndex<i32> = new ReadOnlyIndex<i32>(7);
  const readonlyWord: ReadOnlyIndex<string> = new ReadOnlyIndex<string>("fixed");
  print(`${readonlyInteger[integerIndex]} ${readonlyWord[wordIndex]}`);
}
