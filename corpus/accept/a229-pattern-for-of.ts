// corpus: accept/a229-pattern-for-of
// purpose: Binds a pattern from each element of a for-of subject.
// observable: the element type decides the pattern, and a let binding rebinds.
// exercises: binding-pattern, for-of, fixed-array, class-field, reference-element
// questions: Q30, compiler section 107
// tsc: accepts; js-comparable: yes
class Name {
  text: string;
  constructor(text: string) {
    this.text = text;
  }
}

class Row {
  head: Name;
  count: i32;
  constructor(head: Name, count: i32) {
    this.head = head;
    this.count = count;
  }
}

export function main(): void {
  const rows: i32[][] = [[1, 2], [3, 4]];
  for (const [first, second] of rows) {
    print(`${first} ${second}`);
  }
  const fixed: FixedArray<i32, 2>[] = [[5, 6]];
  for (const [low, high] of fixed) {
    print(`${low} ${high}`);
  }
  const records: Row[] = [new Row(new Name("one"), 7), new Row(new Name("two"), 8)];
  for (const { head, count: repeats } of records) {
    print(`${head.text} ${repeats}`);
  }
  for (let [first, second] of rows) {
    first = first * 10;
    second = second * 10;
    print(`${first} ${second}`);
  }
}
